pub fn main() {
    let bytes: [u8; 8] = unsafe { ::std::mem::transmute(0u64) };
    let _: &[u8] = &bytes;
}
