use std::cmp::Ordering;
use std::ops;

use rustc::mir::{self, traversal, BasicBlock, Location};
use rustc_data_structures::bit_set::BitSet;
use rustc_data_structures::indexed_vec::{Idx, IndexVec};
use rustc_data_structures::work_queue::WorkQueue;

use crate::dataflow::BottomValue;

/// A specific kind of dataflow analysis.
///
/// To run a dataflow analysis, one must set the initial state of the `START_BLOCK` via
/// `initialize_start_block` and define a transfer function for each statement or terminator via
/// the various `effect` methods. The entry set for all other basic blocks is initialized to
/// `Self::BOTTOM_VALUE`. The dataflow `Engine` then iteratively updates the various entry sets for
/// each block with the cumulative effects of the transfer functions of all preceding blocks.
///
/// You should use an `Engine` to actually run an analysis, and a `ResultsCursor` to inspect the
/// results of that analysis like so:
///
/// ```ignore
/// fn do_my_analysis(body: &mir::Body<'tcx>, dead_unwinds: &BitSet<BasicBlock>) {
///     let analysis = MyAnalysis::new();
///     let results = Engine::new(body, dead_unwinds, analysis).iterate_to_fixpoint();
///     let mut cursor = dataflow::ResultsCursor::new(body, results);
///
///     for statement_index in body.block_data[START_BLOCK].statements.iter() {
///         cursor.seek_after(Location { block: START_BLOCK, statement_index });
///         let state = cursor.get();
///         println!("{:?}", state);
///     }
/// }
/// ```
pub trait Analysis<'tcx>: BottomValue {
    /// The index type used to access the dataflow state.
    type Idx: Idx;

    /// A name describing the dataflow analysis being implemented.
    ///
    /// The name should be suitable as part of a filename, so avoid whitespace, slashes or periods
    /// and try to keep it short.
    fn name() -> &'static str;

    /// The size of each bitvector allocated for each block.
    fn bits_per_block(&self, body: &mir::Body<'tcx>) -> usize;

    /// Mutates the entry set of the `START_BLOCK` to containthe initial state for dataflow
    /// analysis.
    fn initialize_start_block(&self, body: &mir::Body<'tcx>, state: &mut BitSet<Self::Idx>);

    /// Updates the current dataflow state with the effect of evaluating a statement.
    fn apply_statement_effect(
        &self,
        state: &mut BitSet<Self::Idx>,
        statement: &mir::Statement<'tcx>,
        location: Location,
    );

    /// Updates the current dataflow state with the effect of evaluating a statement.
    ///
    /// Note that the effect of a successful return from a `Call` terminator should **not** be
    /// acounted for in this function. That should go in `apply_call_return_effect`. For example,
    /// in the `InitializedPlaces` analyses, the return place is not marked as initialized here.
    fn apply_terminator_effect(
        &self,
        state: &mut BitSet<Self::Idx>,
        terminator: &mir::Terminator<'tcx>,
        location: Location,
    );

    /// Updates the current dataflow state with the effect of a successful return from a `Call`
    /// terminator.
    ///
    /// This is separated from `apply_terminator_effect` to properly track state across
    /// unwind edges for `Call`s.
    fn apply_call_return_effect(
        &self,
        state: &mut BitSet<Self::Idx>,
        block: BasicBlock,
        func: &mir::Operand<'tcx>,
        args: &[mir::Operand<'tcx>],
        return_place: &mir::Place<'tcx>,
    );

    /// Applies the cumulative effect of an entire basic block to the dataflow state (except for
    /// `call_return_effect`, which is handled in the `Engine`).
    ///
    /// The default implementation calls `statement_effect` for every statement in the block before
    /// finally calling `terminator_effect`. However, some dataflow analyses are able to coalesce
    /// transfer functions for an entire block and apply them at once. Such analyses should
    /// override `block_effect`.
    fn apply_whole_block_effect(
        &self,
        state: &mut BitSet<Self::Idx>,
        block: BasicBlock,
        block_data: &mir::BasicBlockData<'tcx>,
    ) {
        for (statement_index, stmt) in block_data.statements.iter().enumerate() {
            let location = Location { block, statement_index };
            self.apply_statement_effect(state, stmt, location);
        }

        let location = Location { block, statement_index: block_data.statements.len() };
        self.apply_terminator_effect(state, block_data.terminator(), location);
    }

    /// Applies the cumulative effect of a sequence of statements (and possibly a terminator)
    /// within a single basic block.
    ///
    /// When called with `0..block_data.statements.len() + 1` as the statement range, this function
    /// is equivalent to `apply_whole_block_effect`.
    fn apply_partial_block_effect(
        &self,
        state: &mut BitSet<Self::Idx>,
        block: BasicBlock,
        block_data: &mir::BasicBlockData<'tcx>,
        mut range: ops::Range<usize>,
    ) {
        if range.is_empty() {
            return;
        }

        // The final location might be a terminator, so iterate through all statements until the
        // final one, then check to see whether the final one is a statement or terminator.
        //
        // This can't cause the range to wrap-around since we check that the range contains at
        // least one element above.
        range.end -= 1;
        let final_location = Location { block, statement_index: range.end };

        for statement_index in range {
            let location = Location { block, statement_index };
            let stmt = &block_data.statements[statement_index];
            self.apply_statement_effect(state, stmt, location);
        }

        if final_location.statement_index == block_data.statements.len() {
            let terminator = block_data.terminator();
            self.apply_terminator_effect(state, terminator, final_location);
        } else {
            let stmt = &block_data.statements[final_location.statement_index];
            self.apply_statement_effect(state, stmt, final_location);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CursorPosition {
    AtBlockStart(BasicBlock),
    After(Location),
}

impl CursorPosition {
    fn block(&self) -> BasicBlock {
        match *self {
            Self::AtBlockStart(block) => block,
            Self::After(Location { block, .. }) => block,
        }
    }
}

/// Inspect the results of dataflow analysis.
///
/// This cursor has linear performance when visiting statements in a block in order. Visiting
/// statements within a block in reverse order is `O(n^2)`, where `n` is the number of statements
/// in that block.
pub struct ResultsCursor<'mir, 'tcx, A>
where
    A: Analysis<'tcx>,
{
    body: &'mir mir::Body<'tcx>,
    results: Results<'tcx, A>,
    state: BitSet<A::Idx>,

    pos: CursorPosition,

    /// Whether the effects of `apply_call_return_effect` are currently stored in `state`.
    ///
    /// This flag ensures that multiple calls to `seek_after_assume_call_returns` with the same
    /// target only result in one invocation of `apply_call_return_effect`.
    is_call_return_effect_applied: bool,
}

impl<'mir, 'tcx, A> ResultsCursor<'mir, 'tcx, A>
where
    A: Analysis<'tcx>,
{
    /// Returns a new cursor for `results` that points to the start of the `START_BLOCK`.
    pub fn new(body: &'mir mir::Body<'tcx>, results: Results<'tcx, A>) -> Self {
        ResultsCursor {
            body,
            pos: CursorPosition::AtBlockStart(mir::START_BLOCK),
            is_call_return_effect_applied: false,
            state: results.entry_sets[mir::START_BLOCK].clone(),
            results,
        }
    }

    /// Resets the cursor to the start of the given `block`.
    pub fn seek_to_block_start(&mut self, block: BasicBlock) {
        self.state.overwrite(&self.results.entry_sets[block]);
        self.pos = CursorPosition::AtBlockStart(block);
        self.is_call_return_effect_applied = false;
    }

    /// Updates the cursor to hold the dataflow state immediately before `target`.
    #[allow(unused)]
    pub fn seek_before(&mut self, target: Location) {
        assert!(target <= self.body.terminator_loc(target.block));

        if target.statement_index == 0 {
            self.seek_to_block_start(target.block);
        } else {
            self._seek_after(Location {
                block: target.block,
                statement_index: target.statement_index - 1,
            });
        }
    }

    /// Updates the cursor to hold the dataflow state at `target`.
    ///
    /// If `target` is a `Call` terminator, `apply_call_return_effect` will not be called. See
    /// `seek_after_assume_call_returns` if you wish to observe the dataflow state upon a
    /// successful return.
    #[allow(unused)]
    pub fn seek_after(&mut self, target: Location) {
        assert!(target <= self.body.terminator_loc(target.block));

        // This check ensures the correctness of a call to `seek_after_assume_call_returns`
        // followed by one to `seek_after` with the same target.
        if self.is_call_return_effect_applied {
            self.seek_to_block_start(target.block);
        }

        self._seek_after(target);
    }

    /// Equivalent to `seek_after`, but also calls `apply_call_return_effect` if `target` is a
    /// `Call` terminator whose callee is convergent.
    #[allow(unused)]
    pub fn seek_after_assume_call_returns(&mut self, target: Location) {
        assert!(target <= self.body.terminator_loc(target.block));

        self._seek_after(target);

        if target != self.body.terminator_loc(target.block) {
            return;
        }

        let term = self.body.basic_blocks()[target.block].terminator();
        if let mir::TerminatorKind::Call {
            destination: Some((return_place, _)),
            func,
            args,
            ..
        } = &term.kind {
            if !self.is_call_return_effect_applied {
                self.results.analysis.apply_call_return_effect(
                    &mut self.state,
                    target.block,
                    func,
                    args,
                    return_place,
                );
            }
        }
    }

    fn _seek_after(&mut self, target: Location) {
        let Location { block: target_block, statement_index: target_index } = target;

        if self.pos.block() != target_block {
            self.seek_to_block_start(target_block);
        }

        // If we're in the same block but after the target statement, we need to reset to the start
        // of the block.
        if let CursorPosition::After(Location { statement_index: curr_index, .. }) = self.pos {
            match curr_index.cmp(&target_index) {
                Ordering::Equal => return,
                Ordering::Less => {},
                Ordering::Greater => self.seek_to_block_start(target_block),
            }
        }

        // The cursor is now in the same block as the target location pointing at an earlier
        // statement.
        debug_assert_eq!(self.pos.block(), target_block);
        if let CursorPosition::After(Location { statement_index, .. }) = self.pos {
            debug_assert!(statement_index < target_index);
        }

        let first_unapplied_statement = match self.pos {
            CursorPosition::AtBlockStart(_) => 0,
            CursorPosition::After(Location { statement_index, .. }) => statement_index + 1,
        };

        let block_data = &self.body.basic_blocks()[target_block];
        self.results.analysis.apply_partial_block_effect(
            &mut self.state,
            target_block,
            block_data,
            first_unapplied_statement..target_index + 1,
        );

        self.pos = CursorPosition::After(target);
        self.is_call_return_effect_applied = false;
    }

    /// Gets the dataflow state at the current location.
    pub fn get(&self) -> &BitSet<A::Idx> {
        &self.state
    }
}

/// A completed dataflow analysis.
pub struct Results<'tcx, A>
where
    A: Analysis<'tcx>,
{
    analysis: A,
    entry_sets: IndexVec<BasicBlock, BitSet<A::Idx>>,
}

/// All information required to iterate a dataflow analysis to fixpoint.
pub struct Engine<'a, 'tcx, A>
where
    A: Analysis<'tcx>,
{
    analysis: A,
    bits_per_block: usize,
    body: &'a mir::Body<'tcx>,
    dead_unwinds: &'a BitSet<BasicBlock>,
    entry_sets: IndexVec<BasicBlock, BitSet<A::Idx>>,
}

impl<A> Engine<'a, 'tcx, A>
where
    A: Analysis<'tcx>,
{
    pub fn new(
        body: &'a mir::Body<'tcx>,
        dead_unwinds: &'a BitSet<BasicBlock>,
        analysis: A,
    ) -> Self {
        let bits_per_block = analysis.bits_per_block(body);

        let bottom_value_set = if A::BOTTOM_VALUE == true {
            BitSet::new_filled(bits_per_block)
        } else {
            BitSet::new_empty(bits_per_block)
        };

        let mut entry_sets = IndexVec::from_elem(bottom_value_set, body.basic_blocks());
        analysis.initialize_start_block(body, &mut entry_sets[mir::START_BLOCK]);

        Engine {
            analysis,
            bits_per_block,
            body,
            dead_unwinds,
            entry_sets,
        }
    }

    pub fn iterate_to_fixpoint(mut self) -> Results<'tcx, A> {
        let mut temp_state = BitSet::new_empty(self.bits_per_block);

        let mut dirty_queue: WorkQueue<BasicBlock> =
            WorkQueue::with_none(self.body.basic_blocks().len());

        for (bb, _) in traversal::reverse_postorder(self.body) {
            dirty_queue.insert(bb);
        }

        // Add blocks that are not reachable from START_BLOCK to the work queue. These blocks will
        // be processed after the ones added above.
        for bb in self.body.basic_blocks().indices() {
            dirty_queue.insert(bb);
        }

        while let Some(bb) = dirty_queue.pop() {
            let bb_data = &self.body[bb];
            let on_entry = &self.entry_sets[bb];

            temp_state.overwrite(on_entry);
            self.analysis.apply_whole_block_effect(&mut temp_state, bb, bb_data);

            self.propagate_bits_into_graph_successors_of(
                &mut temp_state,
                (bb, bb_data),
                &mut dirty_queue,
            );
        }

        Results {
            analysis: self.analysis,
            entry_sets: self.entry_sets,
        }
    }

    fn propagate_bits_into_graph_successors_of(
        &mut self,
        in_out: &mut BitSet<A::Idx>,
        (bb, bb_data): (BasicBlock, &'a mir::BasicBlockData<'tcx>),
        dirty_list: &mut WorkQueue<BasicBlock>,
    ) {
        match bb_data.terminator().kind {
            mir::TerminatorKind::Return
            | mir::TerminatorKind::Resume
            | mir::TerminatorKind::Abort
            | mir::TerminatorKind::GeneratorDrop
            | mir::TerminatorKind::Unreachable => {}

            mir::TerminatorKind::Goto { target }
            | mir::TerminatorKind::Assert { target, cleanup: None, .. }
            | mir::TerminatorKind::Yield { resume: target, drop: None, .. }
            | mir::TerminatorKind::Drop { target, location: _, unwind: None }
            | mir::TerminatorKind::DropAndReplace { target, value: _, location: _, unwind: None } =>
            {
                self.propagate_bits_into_entry_set_for(in_out, target, dirty_list);
            }

            mir::TerminatorKind::Yield { resume: target, drop: Some(drop), .. } => {
                self.propagate_bits_into_entry_set_for(in_out, target, dirty_list);
                self.propagate_bits_into_entry_set_for(in_out, drop, dirty_list);
            }

            mir::TerminatorKind::Assert { target, cleanup: Some(unwind), .. }
            | mir::TerminatorKind::Drop { target, location: _, unwind: Some(unwind) }
            | mir::TerminatorKind::DropAndReplace {
                target,
                value: _,
                location: _,
                unwind: Some(unwind),
            } => {
                self.propagate_bits_into_entry_set_for(in_out, target, dirty_list);
                if !self.dead_unwinds.contains(bb) {
                    self.propagate_bits_into_entry_set_for(in_out, unwind, dirty_list);
                }
            }

            mir::TerminatorKind::SwitchInt { ref targets, .. } => {
                for target in targets {
                    self.propagate_bits_into_entry_set_for(in_out, *target, dirty_list);
                }
            }

            mir::TerminatorKind::Call { cleanup, ref destination, ref func, ref args, .. } => {
                if let Some(unwind) = cleanup {
                    if !self.dead_unwinds.contains(bb) {
                        self.propagate_bits_into_entry_set_for(in_out, unwind, dirty_list);
                    }
                }

                if let Some((ref dest_place, dest_bb)) = *destination {
                    // N.B.: This must be done *last*, after all other
                    // propagation, as documented in comment above.
                    self.analysis.apply_call_return_effect(in_out, bb, func, args, dest_place);
                    self.propagate_bits_into_entry_set_for(in_out, dest_bb, dirty_list);
                }
            }

            mir::TerminatorKind::FalseEdges { real_target, imaginary_target } => {
                self.propagate_bits_into_entry_set_for(in_out, real_target, dirty_list);
                self.propagate_bits_into_entry_set_for(in_out, imaginary_target, dirty_list);
            }

            mir::TerminatorKind::FalseUnwind { real_target, unwind } => {
                self.propagate_bits_into_entry_set_for(in_out, real_target, dirty_list);
                if let Some(unwind) = unwind {
                    if !self.dead_unwinds.contains(bb) {
                        self.propagate_bits_into_entry_set_for(in_out, unwind, dirty_list);
                    }
                }
            }
        }
    }

    fn propagate_bits_into_entry_set_for(
        &mut self,
        in_out: &BitSet<A::Idx>,
        bb: BasicBlock,
        dirty_queue: &mut WorkQueue<BasicBlock>,
    ) {
        let entry_set = &mut self.entry_sets[bb];
        let set_changed = self.analysis.join(entry_set, &in_out);
        if set_changed {
            dirty_queue.insert(bb);
        }
    }
}
