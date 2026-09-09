# Comment provenance

Set `PreprocessorConfig.documentation` to `Some(DocumentationOptions::default())`
to retain documentation comments and physical token locations. The default C
frontend configuration leaves capture disabled. `parse_all_comments` additionally
retains ordinary comments, including their inferred trailing position.

`Preprocessed::documentation()` returns an owned `Documentation` catalog, or
`None` when capture was disabled or no matching comments were found. An empty
subsequent preprocessing run starts without the previous catalog. Capture does
not enable declaration, initializer, or function-body graph retention.

## Physical spelling and expansion locations

Each `DocumentationSource` records one physical source read, its canonical path,
accessed spelling, raw comment groups, and system-header regions. Repeated reads
remain distinct. Comments are collected while the already-open source passes
through ordinary normalization; headers are not reopened for documentation.
The recorded ranges index the original physical text. Delimiters, line splices,
and separators inside adjacent comment groups remain intact.

Output token mappings expose an invocation location and, for macro replacements,
a physical spelling location. `DocumentationOrigin::is_macro()` also distinguishes
configured macro expansions, which have no physical replacement spelling, from
direct tokens. This supports Clang's invocation-first lookup with
a declaration-begin spelling fallback. A full declaration supplied as a macro
argument can therefore carry a comment, while a comment on a name-only argument
does not become a declaration comment. Public `Macro` and `MacroDefinition`
replacement strings retain their existing normalized, unexpanded representation.
A private, bounded side table carries replacement token locations during the run
and is released at completion.

Source IDs belong to the catalog that produced them. Mapping and source lookups
are read-only. Mappings cover token bytes; separator bytes have no documentation
mapping. `RawComment::following_barrier` records the next physical `;`, `{`, `}`,
`#`, or `@`, supporting the pinned Clang attachment check without retaining the
entire source again.

## System-header regions

The initial class comes from the include boundary. Captured regions additionally
follow active `GCC system_header` pragmas, Clang's corresponding pragma, expanded
`_Pragma`, and GNU line-marker flag 3. Main-file system-header pragmas have no
effect. Region state propagates into nested includes and is restored on return.
A plain GNU line marker restores user-header status. Diagnostic `#line` filenames
do not replace physical source identity.

The catalog retains raw comments in both user and system regions. A consumer
chooses whether system comments are eligible; retaining raw provenance does not
silently enable bindgen's system-comment emission option. The independent
`FileOrigins` catalog continues to describe the initial include class.

## Bounds and validation

The documentation catalog has its own conservative retained-data estimate bounded
by `max_source_bytes`. Source, origin, and output mapping counts are each capped at
one million; comment and region counts are capped per source. Source byte offsets
must fit `u32`. These checks run before the corresponding owned allocations.
The private token origin ID fits existing token padding.

Focused tests cover raw groups, escaped markers, physical versus logical lines,
macro locations, region propagation, reset, and budget failures. A parser-backed
prototype matches all 18 saved bindgen 0.72.1/libclang 18 macro-comment attachment
probes. This layer supplies provenance; the adapter's
[documentation emission](documentation-emission.md) adds declaration and field
attachment and Rust `#[doc]` output. Macro constants do not acquire Rust
documentation through either API.

Nine real header routes were run with capture disabled and enabled, with unchanged
preprocessed source, semantic units, and eight requested binding outputs. Inputs
with no matching comments—including both zlib routes and 1/100/1,000 declaration
fixtures—have unchanged allocation counts and bytes. Opt-in cumulative allocation
increases were about 1.5 MB for SQLite, 0.75 MB for zstd, 24.8 MB for libgit2, and
20.0 MB for AWS-LC. These are allocated bytes over the run, not peak retained
memory. Shared-host timing samples do not establish a release performance claim.

The [saved evidence](../corpus/evidence/comment-provenance-2026-09-09/README.md)
includes the prototype comparisons and root integration checks. The preprocessing
fuzz harness now covers 480 distinct policy combinations, including disabled,
documentation-only, and all-comment capture, while preserving its existing
macro-history and redefinition selectors.
