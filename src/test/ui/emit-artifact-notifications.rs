// compile-flags:--emit=metadata --error-format=json -Z emit-artifact-notifications
// compile-pass
// ignore-pass
// ^-- needed because `--pass check` does not emit the output needed.

// A very basic test for the emission of artifact notifications in JSON output.

fn main() {}
