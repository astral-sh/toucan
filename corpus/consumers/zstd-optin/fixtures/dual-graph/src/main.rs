fn main() {
    let compressed = include_bytes!(concat!(env!("OUT_DIR"), "/probe.zst"));
    let input = b"Toucan independently selects host and runtime zstd binding generators.";
    let decoded = zstd::bulk::decompress(compressed, input.len()).unwrap();
    assert_eq!(decoded, input);
    println!("build-time zstd compression and runtime decompression match");
}
