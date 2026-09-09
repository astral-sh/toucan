# AWS-LC all-bindings differential on Linux x86-64

This compares the generated Rust files from the paired, unchanged AWS-LC
[`all-bindings` build](../aws-lc-all-bindings-7db2b85/README.md), at frozen Toucan
frontend source `7db2b85027108be1515ad76263720e665cba2ab9`. Both exact input
files are preserved in that build's `capture.json.gz`; their SHA-256 digests are
recorded in [summary.json](summary.json). The full parsed API, native Rust layout
observations, record-name correspondence, and differences are in
[report.json.gz](report.json.gz).

The comparator removes a leading LLVM mangling escape from a linker *symbol*
only on Linux ELF targets. [LLVM documents this escape](https://llvm.org/docs/LangRef.html#identifiers):
it suppresses mangling and is absent from the emitted symbol. The report counts
2,617 bindgen function escapes and 60 global escapes; it rejects collisions
after normalization. The [standalone Rust probe](elf-link-name-probe.rs)
compiled to an ELF object whose `nm -u` output references
`toucan_link_name_probe` without the marker. The comparator still compares
each public Rust function or global name and shape inside its canonical
linker-symbol group.

| Check | Shared entries that agree | Remaining difference |
| --- | ---: | --- |
| Public functions and global signatures | 2,618 / 2,618 functions; 60 / 60 globals | None after linker-symbol and record-position pairing |
| Constants and enums | 3,851 / 3,851 constants; one enum, three variants | None in those inventories |
| Type aliases | 403 / 403 shared | Toucan emits 13 additional integer and pointer-difference aliases |
| Record source shapes | 228 / 232 shared | One incomplete tag and three explicit bindgen padding fields |
| Compiled Rust layout | 98 / 98 shared records; 441 / 441 shared fields | Bindgen alone emits an address-byte field for the incomplete tag and three explicit padding fields |

**Full public API equality remains false.** The nested incomplete
`struct CRYPTO_dynlock_value` is publicly named `CRYPTO_dynlock_value` by bindgen
and `CRYPTO_dynlock_CRYPTO_dynlock_value` by Toucan. Record correspondence makes
the function signatures comparable, but downstream Rust code naming that type
directly does not compile against both files. Bindgen emits a synthetic
`_address: u8` field for this incomplete C type; C cannot form an object of the
incomplete type, so copying that fake by-value layout would be unsound. The
three other source-shape differences are explicit private tail-padding storage;
all common compiled offsets and layouts agree. The 13 extra public aliases can
also affect glob imports.

The native probes compile the *generated Rust* files under edition 2021 and
compare values/layouts; the separate paired build ran six C/Rust layouts, 98
generated layout tests per generator, and representative crypto and memory-BIO
calls. This is Linux x86-64 with SSL and FIPS disabled. It does not establish
the entire downstream Rust surface, trait implementations, ABI register
classification, Windows/macOS behavior, or other AWS-LC profiles.

To replay the differential, extract `evidence/toucan-bindings.rs` and
`evidence/reference-bindings.rs` from the linked capture's `files` entries
(`text` contains the exact source), and run
`scripts/compare_bindings.py --toucan-bindings <toucan.rs> --bindgen-bindings <reference.rs> --target x86_64-unknown-linux-gnu --edition 2021 --output <report.json>`.
The comparator builds `tools/binding_compare` if `--analyzer` is omitted.
The linker-symbol probe can be repeated with
`rustc --edition=2021 --crate-type=lib --emit=obj elf-link-name-probe.rs -o /tmp/toucan-elf-link-name.o`
followed by `nm -u /tmp/toucan-elf-link-name.o`.
