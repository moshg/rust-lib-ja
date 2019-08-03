// only-x86_64

// FIXME https://github.com/rust-lang/rust/issues/59774
// normalize-stderr-test "thread.*panicked.*Metadata module not compiled.*\n" -> ""
// normalize-stderr-test "note:.*RUST_BACKTRACE=1.*\n" -> ""
const HUGE_SIZE: usize = !0usize / 8;


pub struct TooBigArray {
    arr: [u8; HUGE_SIZE],
}

impl TooBigArray {
    pub const fn new() -> Self {
        TooBigArray { arr: [0x00; HUGE_SIZE], }
    }
}

static MY_TOO_BIG_ARRAY_1: TooBigArray = TooBigArray::new();
//~^ ERROR the type `[u8; 2305843009213693951]` is too big for the current architecture
static MY_TOO_BIG_ARRAY_2: [u8; HUGE_SIZE] = [0x00; HUGE_SIZE];
//~^ ERROR the type `[u8; 2305843009213693951]` is too big for the current architecture

fn main() { }
