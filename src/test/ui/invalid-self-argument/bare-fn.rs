fn b(foo: u32, &mut self) { }
//~^ ERROR unexpected `self` argument in function
//~| NOTE `self` is only valid as the first argument of a trait function

fn main() { }
