fn main() {
    let input = b"Toucan independently selects host and runtime zstd binding generators.";
    let compressed = zstd::bulk::compress(input, 3).unwrap();
    std::fs::write(std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("probe.zst"), compressed).unwrap();
}
