pub fn cases() -> Vec<(&'static str, String)> {
    let chain = |length: usize| {
        (0..length)
            .map(|index| format!("value_{index}"))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    let mixed = (0..32)
        .map(|index| format!("value_{index} * 3 + 7 / 2"))
        .collect::<Vec<_>>()
        .join(" - ");
    vec![
        ("add-16", chain(16)),
        ("add-128", chain(128)),
        ("mixed-128", mixed),
        (
            "assignment-conditional-comma",
            "result = ready ? value + offset * stride : limit - 1, result += flags & 255, result".into(),
        ),
        // Pinned corpus zstd 1.5.7, lib/zstd.h (comments removed by preprocessing).
        // Preserve its parenthesization:
        // parenthesized expressions are owned leaves in this initial prototype.
        ("zstd-blocksize-max", "(1<<ZSTD_BLOCKSIZELOG_MAX)".into()),
        (
            "zstd-compressbound",
            "(((size_t)(srcSize) >= ZSTD_MAX_INPUT_SIZE) ? 0 : (srcSize) + ((srcSize)>>8) + (((srcSize) < (128<<10)) ? (((128<<10) - (srcSize)) >> 11) : 0))".into(),
        ),
        // Pinned corpus zlib 1.3.1, zlib.h: ZLIB_VERNUM.
        ("zlib-vernum", "0x1310".into()),
    ]
}
