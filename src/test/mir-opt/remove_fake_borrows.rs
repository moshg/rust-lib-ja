// Test that the fake borrows for matches are removed after borrow checking.

// ignore-wasm32-bare

#![feature(nll)]

fn match_guard(x: Option<&&i32>, c: bool) -> i32 {
    match x {
        Some(0) if c => 0,
        _ => 1,
    }
}

fn main() {
    match_guard(None, true);
}

// END RUST SOURCE

// START rustc.match_guard.CleanFakeReadsAndBorrows.before.mir
// bb0: {
//     FakeRead(ForMatchedPlace, _1);
//     _3 = discriminant(_1);
//     switchInt(move _3) -> [1isize: bb5, otherwise: bb2];
// }
// bb1: {
//     goto -> bb8;
// }
// bb2: {
//     goto -> bb9;
// }
// bb3: {
//     unreachable;
// }
// bb4: {
//     goto -> bb2;
// }
// bb5: {
//     switchInt((*(*((_1 as Some).0: &'<empty> &'<empty> i32)))) -> [0i32: bb1, otherwise: bb2];
// }
// bb6: {
//     StorageDead(_8);
//     return;
// }
// bb7: {
//     _0 = const 0i32;
//     goto -> bb6;
// }
// bb8: {
//     _4 = &shallow _1;
//     _5 = &shallow ((_1 as Some).0: &'<empty> &'<empty> i32);
//     _6 = &shallow (*((_1 as Some).0: &'<empty> &'<empty> i32));
//     _7 = &shallow (*(*((_1 as Some).0: &'<empty> &'<empty> i32)));
//     StorageLive(_8);
//     _8 = _2;
//     FakeRead(ForMatchGuard, _4);
//     FakeRead(ForMatchGuard, _5);
//     FakeRead(ForMatchGuard, _6);
//     FakeRead(ForMatchGuard, _7);
//     switchInt(move _8) -> [false: bb4, otherwise: bb7];
// }
// bb9: {
//     _0 = const 1i32;
//     goto -> bb6;
// }
// bb10: {
//     resume;
// }
// END rustc.match_guard.CleanFakeReadsAndBorrows.before.mir

// START rustc.match_guard.CleanFakeReadsAndBorrows.after.mir
// bb0: {
//     nop;
//     _3 = discriminant(_1);
//     switchInt(move _3) -> [1isize: bb5, otherwise: bb2];
// }
// bb1: {
//     goto -> bb8;
// }
// bb2: {
//     goto -> bb9;
// }
// bb3: {
//     unreachable;
// }
// bb4: {
//     goto -> bb2;
// }
// bb5: {
//     switchInt((*(*((_1 as Some).0: &'<empty> &'<empty> i32)))) -> [0i32: bb1, otherwise: bb2];
// }
// bb6: {
//     StorageDead(_8);
//     return;
// }
// bb7: {
//     _0 = const 0i32;
//     goto -> bb6;
// }
// bb8: {
//     nop;
//     nop;
//     nop;
//     nop;
//     StorageLive(_8);
//     _8 = _2;
//     nop;
//     nop;
//     nop;
//     nop;
//     switchInt(move _8) -> [false: bb4, otherwise: bb7];
// }
// bb9: {
//     _0 = const 1i32;
//     goto -> bb6;
// }
// bb10: {
//     resume;
//    }
// END rustc.match_guard.CleanFakeReadsAndBorrows.after.mir
