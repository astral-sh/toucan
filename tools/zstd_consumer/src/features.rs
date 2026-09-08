use zstd::zstd_safe;

pub fn record_output(name: &str, bytes: &[u8]) {
    if let Some(directory) = std::env::var_os("TOUCAN_CONSUMER_OUTPUT") {
        std::fs::write(std::path::Path::new(&directory).join(name), bytes).unwrap();
    }
}

// A reproducible input fingerprint lets the harness compare actual compressed
// bytes across the upstream and generated bindings without storing large files.
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn dictionary_round_trip(name: &str, dictionary: &[u8], input: &[u8]) {
    let id = zstd_safe::get_dict_id_from_dict(dictionary).unwrap();
    let encoder = zstd::dict::EncoderDictionary::copy(dictionary, 3);
    let decoder = zstd::dict::DecoderDictionary::copy(dictionary);
    let mut compressor = zstd::bulk::Compressor::with_prepared_dictionary(&encoder).unwrap();
    let mut decompressor = zstd::bulk::Decompressor::with_prepared_dictionary(&decoder).unwrap();
    let compressed = compressor.compress(input).unwrap();
    assert_eq!(zstd_safe::get_dict_id_from_frame(&compressed), Some(id));
    assert_eq!(
        decompressor.decompress(&compressed, input.len()).unwrap(),
        input
    );
    assert!(zstd::bulk::decompress(&compressed, input.len()).is_err());
    record_output(&format!("{name}.dict"), dictionary);
    record_output(&format!("{name}.zst"), &compressed);
    println!(
        "dictionary: {} bytes, frame {} bytes, fingerprint {:016x}",
        dictionary.len(),
        compressed.len(),
        fingerprint(&compressed)
    );
}

pub fn verify() {
    let samples: Vec<Vec<u8>> = (0..1024).map(|index| {
        format!("package package-{index:04}: version {}.{}.{}; platform {}; dependencies: parser, compiler, native-library, archive; source: https://example.invalid/packages/package-{index:04}/archive.tar.zst\n",
            index % 13, index % 17, index % 31, ["linux", "macos", "windows"][index % 3]).into_bytes()
    }).collect();
    let input = samples.concat();
    let dictionary = zstd::dict::from_samples(&samples, 2048).unwrap();
    dictionary_round_trip("trained", &dictionary, &input);

    #[cfg(feature = "experimental")]
    experimental(&samples, &input);
    #[cfg(feature = "zstdmt")]
    threaded(&input);
    #[cfg(all(feature = "experimental", feature = "zstdmt"))]
    shared_thread_pool(&input);
}

#[cfg(feature = "experimental")]
fn experimental(samples: &[Vec<u8>], input: &[u8]) {
    use zstd_safe::{CCtx, CParameter, DCtx, DParameter, FrameFormat};

    let mut compressor = CCtx::create();
    compressor
        .set_parameter(CParameter::Format(FrameFormat::Magicless))
        .unwrap();
    let mut compressed = Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    compressor.compress2(&mut compressed, input).unwrap();
    assert!(!zstd_safe::is_frame(&compressed));
    assert!(zstd::bulk::decompress(&compressed, input.len()).is_err());
    let mut decompressor = DCtx::create();
    decompressor
        .set_parameter(DParameter::Format(FrameFormat::Magicless))
        .unwrap();
    let mut decoded = Vec::with_capacity(input.len());
    decompressor.decompress(&mut decoded, &compressed).unwrap();
    assert_eq!(decoded, input);
    record_output("magicless.zst", &compressed);
    println!(
        "experimental magicless: {} bytes, fingerprint {:016x}",
        compressed.len(),
        fingerprint(&compressed)
    );

    // Exercise the experimental zdict file too: COVER takes a nested record by
    // value, and returns a dictionary consumed through the safe Rust wrappers.
    use zstd_safe::zstd_sys;
    let sizes: Vec<usize> = samples.iter().map(Vec::len).collect();
    let parameters = zstd_sys::ZDICT_cover_params_t {
        k: 64,
        d: 8,
        steps: 4,
        nbThreads: 1,
        splitPoint: 1.0,
        shrinkDict: 0,
        shrinkDictMaxRegression: 0,
        zParams: zstd_sys::ZDICT_params_t {
            compressionLevel: 3,
            notificationLevel: 0,
            dictID: 123456,
        },
    };
    let mut dictionary = vec![0; 2048];
    // The input is the concatenation of `samples`, and every pointer remains
    // live for the synchronous call. The output buffer has its advertised size.
    let length = unsafe {
        zstd_sys::ZDICT_trainFromBuffer_cover(
            dictionary.as_mut_ptr().cast(),
            dictionary.len(),
            input.as_ptr().cast(),
            sizes.as_ptr(),
            sizes.len() as u32,
            parameters,
        )
    };
    assert_eq!(unsafe { zstd_sys::ZDICT_isError(length) }, 0);
    assert!(length <= dictionary.len());
    dictionary.truncate(length);
    assert_eq!(
        zstd_safe::get_dict_id_from_dict(&dictionary).unwrap().get(),
        123456
    );
    dictionary_round_trip("cover", &dictionary, input);
}

#[cfg(feature = "zstdmt")]
fn threaded(input: &[u8]) {
    use std::io::Write;

    // Multiple one-MiB jobs exercise the worker implementation, rather than
    // enabling the feature while only compressing a single small block.
    let input = input.repeat(32);
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
    encoder.multithread(2).unwrap();
    encoder
        .set_parameter(zstd_safe::CParameter::JobSize(1024 * 1024))
        .unwrap();
    encoder.include_checksum(true).unwrap();
    encoder.write_all(&input).unwrap();
    let compressed = encoder.finish().unwrap();
    assert_eq!(
        zstd::stream::decode_all(compressed.as_slice()).unwrap(),
        input
    );
    record_output("threaded.zst", &compressed);
    println!(
        "threaded: {} bytes, frame {} bytes, fingerprint {:016x}",
        input.len(),
        compressed.len(),
        fingerprint(&compressed)
    );
}

#[cfg(all(feature = "experimental", feature = "zstdmt"))]
fn shared_thread_pool(input: &[u8]) {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{ZSTD_e_continue, ZSTD_e_end};
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer, ThreadPool};

    let input = input.repeat(32);
    let pool = ThreadPool::new(2);
    let mut compressor = CCtx::create();
    compressor.ref_thread_pool(&pool).unwrap();
    compressor.set_parameter(CParameter::NbWorkers(2)).unwrap();
    compressor
        .set_parameter(CParameter::JobSize(1024 * 1024))
        .unwrap();
    let mut compressed = Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut source = InBuffer::around(&input);
    let mut output = OutBuffer::around(&mut compressed);
    while source.pos() < input.len() {
        compressor
            .compress_stream2(&mut output, &mut source, ZSTD_e_continue)
            .unwrap();
    }
    while compressor
        .compress_stream2(&mut output, &mut source, ZSTD_e_end)
        .unwrap()
        != 0
    {}
    let progress = compressor.get_frame_progression();
    assert_eq!(progress.ingested, input.len() as u64);
    assert_eq!(progress.consumed, input.len() as u64);
    assert_eq!(progress.produced, compressed.len() as u64);
    assert_eq!(progress.flushed, compressed.len() as u64);
    assert!(progress.currentJobID > 1);
    assert_eq!(progress.nbActiveWorkers, 0);
    assert_eq!(
        zstd::bulk::decompress(&compressed, input.len()).unwrap(),
        input
    );
    record_output("shared-pool.zst", &compressed);
    println!(
        "shared pool: {} jobs, frame {} bytes, fingerprint {:016x}",
        progress.currentJobID,
        compressed.len(),
        fingerprint(&compressed)
    );
}
