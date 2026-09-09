fn ffi_test() {
    let input = b"Toucan calls the upstream zstd library. Toucan calls the upstream zstd library.";
    unsafe {
        assert_eq!(b::ZSTD_versionNumber(), 10507);
        let mut compressed =
            vec![0_u8; b::ZSTD_compressBound(input.len().try_into().unwrap()) as usize];
        let compressed_len = b::ZSTD_compress(
            compressed.as_mut_ptr().cast(),
            compressed.len().try_into().unwrap(),
            input.as_ptr().cast(),
            input.len().try_into().unwrap(),
            3,
        );
        assert_eq!(b::ZSTD_isError(compressed_len), 0);
        let mut output = vec![0_u8; input.len()];
        let output_len = b::ZSTD_decompress(
            output.as_mut_ptr().cast(),
            output.len().try_into().unwrap(),
            compressed.as_ptr().cast(),
            compressed_len,
        );
        assert_eq!(b::ZSTD_isError(output_len), 0);
        assert_eq!(output_len as usize, input.len());
        assert_eq!(output, input);
        let context = b::ZSTD_createCCtx();
        assert!(!context.is_null());
        assert_eq!(b::ZSTD_freeCCtx(context), 0);
    }
}
