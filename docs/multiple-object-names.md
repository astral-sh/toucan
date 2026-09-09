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

The [saved Linux x86-64 study](../corpus/evidence/multiple-object-names-2026-09-09.json.gz)
compares 49 settings against bindgen 0.72.1/libclang 18.1.3. All 49 sets of Rust
object names, kinds, normalized types, mutability and constant values match. It covers
extern-only declarations, definitions in either order, integers, floating
values, complete/incomplete string arrays, enums, callback pointers, file/name
unions and repeated callback names.

The shared API analyzer keys globals by their C symbol. To compare each Rust
name independently, this study analyzes derived copies with `link_name`
attributes removed; raw outputs remain intact. Actual linkage is checked by 64
Rust executions against GCC/Clang objects, on current Rust and Rust 1.64. The
checks include mutating one extern alias and observing another, reading projected
constants, string bytes, enum values and callback function pointers.

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
