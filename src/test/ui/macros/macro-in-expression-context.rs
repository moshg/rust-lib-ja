// run-rustfix

macro_rules! foo {
    () => {
        assert_eq!("A", "A");
        assert_eq!("B", "B");
    }
    //~^^ ERROR macro expansion ignores token `assert_eq` and any following
    //~| NOTE the usage of `foo!` is likely invalid in expression context
}

fn main() {
    foo!()
    //~^ NOTE caused by the macro expansion here
}
