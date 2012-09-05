use std;
use comm::Chan;
use comm::send;
use comm::Port;

// tests that ctrl's type gets inferred properly
type command<K: send, V: send> = {key: K, val: V};

fn cache_server<K: send, V: send>(c: Chan<Chan<command<K, V>>>) {
    let ctrl = Port();
    send(c, Chan(ctrl));
}
fn main() { }
