fn ffi_test() {
    let input = b"Toucan calls the upstream zlib library. Toucan calls the upstream zlib library.";
    unsafe {
        assert_eq!(
            std::ffi::CStr::from_ptr(b::zlibVersion()).to_bytes(),
            b"1.3.1"
        );
        let mut compressed = vec![0_u8; b::compressBound(input.len().try_into().unwrap()) as usize];
        let mut compressed_len: b::uLongf = compressed.len().try_into().unwrap();
        assert_eq!(
            b::compress2(
                compressed.as_mut_ptr(),
                &mut compressed_len,
                input.as_ptr(),
                input.len().try_into().unwrap(),
                b::Z_BEST_COMPRESSION
            ),
            b::Z_OK
        );
        let mut output = vec![0_u8; input.len()];
        let mut output_len: b::uLongf = output.len().try_into().unwrap();
        assert_eq!(
            b::uncompress(
                output.as_mut_ptr(),
                &mut output_len,
                compressed.as_ptr(),
                compressed_len
            ),
            b::Z_OK
        );
        assert_eq!(output_len as usize, input.len());
        assert_eq!(output, input);
        let initial = b::crc32(0, std::ptr::null(), 0);
        assert_ne!(
            b::crc32(initial, input.as_ptr(), input.len().try_into().unwrap()),
            initial
        );
    }
}
