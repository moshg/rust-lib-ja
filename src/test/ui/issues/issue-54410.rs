use std::os::raw::c_char;
extern "C" {
    pub static mut symbol: [c_char];
    //~^ ERROR the size for values of type `[i8]` cannot be known at compilation time
}

fn main() {
    println!("{:p}", unsafe { &symbol });
}
