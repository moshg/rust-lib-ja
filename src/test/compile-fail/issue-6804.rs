// Matching against NaN should result in a warning

use std::float::NaN;

fn main() {
    let x = NaN;
    match x {
        NaN => {},
        _ => {},
    };
    //~^^^ WARNING unmatchable NaN in pattern, use is_NaN() in a guard instead
}

// At least one error is needed so that compilation fails
#[static_assert]
static b: bool = false; //~ ERROR static assertion failed
