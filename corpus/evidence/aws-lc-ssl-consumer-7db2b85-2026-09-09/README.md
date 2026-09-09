# AWS-LC SSL consumer on native Linux x64

The frozen Toucan frontend at `7db2b85027108be1515ad76263720e665cba2ab9`
successfully built and ran pinned `aws-lc-sys 0.44.0` and `aws-lc-rs 1.18.0`
beside the original bindgen build on `x86_64-unknown-linux-gnu`. The fixture
enabled the sys crate's `ssl` feature, which selected `all-bindings`, `bindgen`,
`prebuilt-nasm`, and `ssl` in **both** dependency graphs. `aws-lc-rs` has no
separate `ssl` feature. Both unmodified upstream build scripts selected fresh
`OUT_DIR/bindings.rs` via `use_bindgen_pregenerated`, set `BUILD_LIBSSL=ON`,
and linked the native crypto and SSL archives. The only change to the copied
upstream sources was the candidate's `aws-lc-sys/Cargo.toml` build dependency
pointing at frozen `toucan_bindgen`; the reference crate's sources were
unchanged. The complete dependency graphs, source hashes, build-script output,
generated bindings, and dep-info are captured.

Both builds passed **106/106 generated Rust layout tests**, and Rust and an
independently compiled C executable each exercised `TLS_method`, `SSL_CTX_new`,
`SSL_new`, min-protocol setters and getters, and their destructors against
linked libssl: `ssl tls min 771 772`. The C program independently checked the
sizes and alignments of six crypto records. The original crypto/BIO consumer
also passed 41 matching deterministic output artifacts, including 27 AEAD
round trips and error checks, SHA/HMAC cases, Ed25519, and an allocated BIO.
Both native builds compiled the same 403 object source paths and produced
identical crypto archive hashes. The SSL archives contain the same 41 member
names but differ bytewise in 37 C++ objects: upstream C++ flags do not apply
`-ffile-prefix-map`, and `bio_ssl.cc.o`, for example, embeds the reference or
Toucan checkout's different absolute path. The archive retains actual object
digests, embedded paths, CMake caches, and compiler flags. Both paired builds
explicitly set `CXX=clang++-18`; GCC cannot compile this upstream SSL profile's
Clang-only C++ warnings.

The raw Rust API analyzer sees bindgen's leading LLVM ELF `\u{1}` marker on
2,617 functions and 60 globals as a different `link_name`. Replaying the
analyzer with the ELF-only, collision-checked comparator at `1beb88d4`
matches **3,230/3,230 functions**, **60/60 globals**, **4,894/4,894 constants**,
and **436/436 shared aliases**. Its compiled Rust probes also match all 105
shared record layouts, 473 shared field offsets, 4,894 constants, and three
enum variants. Exact generated API equality remains **false**: Toucan adds 15
internal aliases; four record field shapes differ (opaque tag and private
padding representations); generated static-assertion tag line numbers differ;
and bindgen emits five additional private padding/opaque fields in compiled
layout observations. Both full comparator reports record each entry, the
unmatched record names, and zero unsupported items or mapping conflicts.

[summary.json](summary.json) lists revisions, checksums, counts, and selected
features. [capture.tar.gz](capture.tar.gz) preserves 161 entries: complete
frontend/upstream source hash inventories; both generated files and Cargo
dependency/build/layout/runtime logs; CMake and object provenance; harness and
fixture sources; and the raw and ELF-aware parsed/native comparison reports.
`archive-member-sha256.json` inside the archive hashes every other member.
Packaging verified each decompressed member against its input bytes and SHA-256;
the archive SHA-256 is recorded in the summary. The harness records its original
local paths and can be replayed with the pinned source, toolchain, and cached
crate versions after adapting those paths. The original scripts, fixture, and
build commands are included to make that adaptation explicit.

This result covers native Linux x64 and the selected SSL profile. It does not
cover FIPS, Windows or macOS, full TLS handshakes, every SSL function call, or
an external application using the generated bindings. The paired run is a
correctness check, not a speed measurement.
