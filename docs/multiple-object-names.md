# Several Rust names for one C object

Stateful `generated_name_override` callbacks can assign different names to
compatible C object redeclarations. The Builder emits every selected name and
uses the first selected occurrence of that name. Functions retain their existing
first-selected-name policy.

For example, `extern const int shared; const int shared=7;` can produce an extern
`shared_0` and a constant `shared_1`. Both file and name allowlists participate in
selection before deduplication. Repeated callback names still produce one Rust
binding. Every extern projection retains the original C linker symbol.

The core's optional `BindingOptions::additional_objects` maps additional Rust
names to checked `ObjectOccurrence` values. It does not clone or change canonical
C declarations. The existing object map handles the ordinary single-name case.
Additional projections validate their profile, declaration identity, type, Rust
name, TLS/weak/DLL linkage restrictions and alignment independently. Their types
join the ordinary bounded dependency collector before the shared name-collision
check. Scalar and string constants reuse the ordinary checked-value formatter.

## Evidence

The [saved Linux x86-64 study](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/multiple-object-names-2026-09-09.json.gz)
compares 49 settings against bindgen 0.72.1/libclang 18.1.3. All 49 sets of Rust
object names, kinds, normalized types, mutability and constant values match. It covers
extern-only declarations, definitions in either order, integers, floating
values, complete/incomplete string arrays, enums, callback pointers, file/name
unions and repeated callback names.

The original study used derived copies with `link_name` attributes removed to
work around an analyzer that kept only one Rust name per C symbol. Its 64 Rust
executions against GCC/Clang objects separately validate actual linkage, on
current Rust and Rust 1.64. Those checks include mutating one extern alias and
observing another, projected constants, string bytes, enum values and callback
function pointers. The original artifacts remain unchanged.

The full export inventories retain two existing differences separately:

- Unrestricted default generation also exposes Toucan's compiler `va_list`
  helper.
- Two selected typedef-spelling cases retain the canonical first alias instead
  of the later written alias. Object type shapes still match. Written-object
  type provenance is separate from additional-name projection.

Thirty-four default/single-name controls are byte-identical to the preceding
name-filter layer. Six new tests cover projection behavior, repeat-name
selection, byte constants, collisions, explicit core roots, invalid metadata and
independent linkage guards. The original name-filter evidence remains unchanged.

Two review regressions additionally reject internal objects without materialized
constants and conflicting DLL import library rules across primary and additional
projections. Additional internal scalar and string constants remain supported.

## Full raw export replay

The [schema-2 replay](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/binding-symbol-exports-schema2-2026-09-09.json.gz)
uses the unmodified generated Rust, including every `link_name` attribute. It
retains 51 foreign global exports per side across all 49 settings. All 49 object
contracts match in public name, kind, type, mutability, and effective Linux ELF
symbol. The previous constant-value and C-linkage checks remain separate;
this replay performs no new C executions. All 101 input artifact/source hashes
are unchanged before and after the replay.

The raw groups preserve a spelling difference: bindgen uses LLVM's no-mangle
marker (`\u{1}shared`), while Toucan uses `shared`. The artifact includes both the
exact raw comparison and a qualified view for this Linux target, where the
default symbol prefix is empty. It does not apply that interpretation to macOS
or Windows. Raw linker-group equality holds in eight settings; the qualified
object comparison matches all 49.

The complete inventory comparison remains false in 13 settings: two retain the
typedef-spelling differences described above, and 11 unrestricted settings
include Toucan's extra compiler `va_list` record. Consequently, 47 of 49 alias
inventories and 36 of 49 full structural inventories match after the Linux
linker interpretation. Those counts include every public export rather than a
single representative per linker symbol.
