// xfail-win32
extern mod std;

fn f() {
    let a = @0;
    fail;
}

fn main() {
    task::spawn_unlinked(f);
}
