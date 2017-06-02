struct Bar(i32); // ZSTs are tested separately

static mut DROP_COUNT: usize = 0;

impl Drop for Bar {
    fn drop(&mut self) {
        unsafe { DROP_COUNT += 1; }
    }
}

fn main() {
    let b = [Bar(0), Bar(0), Bar(0), Bar(0)];
    assert_eq!(unsafe { DROP_COUNT }, 0);
    drop(b);
    assert_eq!(unsafe { DROP_COUNT }, 4);

    // check empty case
    let b : [Bar; 0] = [];
    drop(b);
    assert_eq!(unsafe { DROP_COUNT }, 4);
}
