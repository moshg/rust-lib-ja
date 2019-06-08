// pp-exact - Make sure we actually print the attributes

#![allow(unused)]
#![feature(rustc_attrs)]

struct Cat {
    name: String,
}

impl Drop for Cat {
    #[rustc_dummy]
    fn drop(&mut self) { println!("{} landed on hir feet" , self . name); }
}


#[rustc_dummy]
fn cat(name: String) -> Cat { Cat{name: name,} }

fn main() { let _kitty = cat("Spotty".to_string()); }
