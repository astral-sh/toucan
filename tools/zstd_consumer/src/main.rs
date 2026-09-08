use std::io::Write;

fn main() {
    let input = b"Toucan bindings used through the real zstd Rust consumer. ".repeat(1000);
    let compressed = zstd::bulk::compress(&input, 3).unwrap();
    let decoded = zstd::bulk::decompress(&compressed, input.len()).unwrap();
    assert_eq!(decoded, input);

    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
    encoder.include_checksum(true).unwrap();
    encoder.write_all(&input).unwrap();
    let stream = encoder.finish().unwrap();
    assert_eq!(zstd::stream::decode_all(stream.as_slice()).unwrap(), input);

    println!(
        "zstd 0.13.3: {} bytes round-tripped through bulk ({} bytes) and streaming ({} bytes) APIs",
        input.len(),
        compressed.len(),
        stream.len()
    );
}
