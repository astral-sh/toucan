# Builder peak resident memory at 7db2b850

The frozen Builder benchmark binary used for the [paired generation timings](../builder-callback-final/README.md)
was measured again on Linux with `/usr/bin/time`'s maximum resident set size
(`%M`, KiB). Both engines received the identical pinned Builder requests for
untouched zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1 headers.

| Project | Toucan median peak RSS | bindgen median peak RSS | Median paired ratio |
| --- | ---: | ---: | ---: |
| zlib | 10.76 MiB | 82.83 MiB | 7.68× |
| SQLite | 15.09 MiB | 85.52 MiB | 5.65× |
| zstd | 10.24 MiB | 78.23 MiB | 7.64× |
| libgit2 | 38.36 MiB | 105.26 MiB | 2.75× |

Each project has five process pairs with randomized engine order. Every process
generated bindings once for warmup and three more times; all 40 process results
are in [the evidence](evidence.json). The displayed memory values are medians
of five maxima per engine. Each ratio compares the two maxima within its pair,
then takes the median of five ratios. Generated outputs, Builder configurations,
all recorded header dependencies, the binary, and libclang match the original
capture; binary, libclang, and header hashes were also rechecked after the run.
The [runner](../../../scripts/benchmark_builder_peak_rss.py) reproduces these
checks from the original captured requests.

Peak RSS is the largest resident memory of a whole process over its lifetime,
including libraries, process startup, warmup, and three measured calls. It does
not isolate allocator behavior, measure retained memory after generation, or
represent a full application build. Filesystem caches were warm and other
tenants of the shared Linux host were not controlled. This is a separate memory
experiment, not another speed measurement or a comparison between revisions.
