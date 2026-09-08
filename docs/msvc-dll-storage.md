# Microsoft DLL storage

The Clang MSVC profile checks `__declspec(dllimport)` and
`__declspec(dllexport)` on C object and function declarations. These attributes
do not change C type identity and do not provide a DLL name.

## Declarations and uses

Both attributes require external linkage and reject thread-local storage.
An imported object with no written storage class acquires `extern` storage,
including inside a block. Imported data cannot have an initializer; an imported
function can have an inline body, but cannot have a non-inline definition.
Typedefs, fields, and C tags ignore DLL storage. Parameters ignore the storage
while still checking attribute arity, as Clang does. Ordinary operands undergo
name lookup even on ignored subjects.

`dllexport` takes precedence when both attributes apply. A later file declaration
without `dllimport` removes an earlier import, except that inline functions keep
it and an object or function definition changes it into an export. Local extern
redeclarations inherit the visible storage. Attributes written after a real
definition are ignored. Tentative object definitions remain a separate case.
Microsoft C also retains earlier external linkage when a later declaration is
written `static`, in all four supported language modes. This follows Clang's
non-pedantic Microsoft compatibility rules; `-pedantic-errors` rejects that
extension.

A variable cannot acquire DLL storage after it has been used. A function may
acquire an import after a use, but cannot acquire an export. This is Clang's
declaration-use rule: dead branches and unselected generic arms still count;
ordinary `sizeof`, `typeof`, `alignof`, and generic controlling operands do not.
Direct allocation and prefetch builtins follow the same declaration-use rule as
ordinary calls, including prototypes with assembly names.
Variably modified operands and array bounds use the compiler's evaluated-context
rules. Tracking is enabled only for Microsoft inputs containing DLL attributes,
with a separate 8 MiB limit for pending names in unevaluated operands.

An imported data address requires the import table at runtime. It cannot be a C
static address constant:

```c
__declspec(dllimport) int value;
int *address = &value;                 // rejected
int *get_address(void) { return &value; } // accepted
```

The same restriction covers arrays and subobjects. Imported function addresses
can initialize static function pointers. Lexically shadowing a DLL object with
an ordinary local object preserves that local object's normal storage rules.
An external redeclaration behind a parameter, local object, or typedef shadow
inherits the earlier linked declaration's DLL storage.

Clang evaluates constant data addresses using the first linked declaration's
import attribute. Adding an import later does not retroactively remove constant
addresses; replacing an initial import with an explicit export does not restore
them. A plain file redeclaration can clear the first declaration's import when
no intervening file declaration has replaced it. This follows Clang's
[canonical lvalue identity](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/AST/APValue.cpp#L42)
and is separate from the final DLL storage used for generated bindings.

## Retained facts

`Declaration::dll_storage_class` records the final file declaration's storage.
`DeclarationSite::dll_storage_class()` records what was effective at that site;
`Entity::dll_storage_class()` records the last retained declaration's storage.
`CheckedCode::dll_storage_source()` exposes written import and export spans.
Inherited and synthesized classes have no written span. Earlier declaration
sites preserve the facts present when they were checked.

These facts describe C declarations, not the DLL storage of every instruction
Clang emitted. For example, adding an import after a tentative definition may
leave the later declaration imported while the emitted object remains a local
program definition. Block declarations can also differ from the visible file
declaration. Retained metadata keeps those distinctions explicit.

`Declaration::is_definition` continues to describe a written, checked body.
Imported inline bodies keep that flag, while binding selection retains their
external function symbol.

Ordinary imported bodies have `FunctionDefinitionKind::InlineOnly`. A GNU
extern-inline replacement keeps each earlier body as `Superseded`; only the
entity's latest body receives the final ownership. An explicit weak binding
instead keeps `WeakInline`: Clang can emit that body when it is referenced.

Clang 18 exposes a lowering discrepancy for weak imported inline definitions.
Its COFF output contains a weak function definition, but its LLVM output combines
`weak` and `dllimport` on a definition, which the LLVM verifier rejects. The
frontend retains the source import and weak-ownership facts separately. These
facts do not establish that emitting the same LLVM attributes is valid.

## Rust bindings

Map the imported C names to their native library names explicitly. With the
builder, patterns use its existing exact-name or trailing `.*` convention:

```rust
let bindings = toucan_bindgen::Builder::default()
    .header("api.h")
    .clang_arg("--target=x86_64-pc-windows-msvc")
    .dll_import_library("api_.*", "api")
    .dll_import_library("api_special", "extension")
    .generate()?;
```

The CLI uses exact names or trailing `*` prefixes:

```console
toucan bindgen api.h --target x86_64-pc-windows-msvc \
  --dll-import-library 'api_*=api' \
  --dll-import-library 'api_special=extension'
```

Library callers can set `BindingOptions::dll_import_libraries`, a map from those
same CLI patterns to library names. Exact names take precedence over prefixes;
the longest matching prefix wins. A later rule for an identical pattern replaces
its value. `*` is an explicit default for all selected imports; there is no
implicit default. Patterns match C names before Rust identifier or linker-symbol
renaming. Selected aliases of one linker symbol must choose the same library.

Generation groups foreign declarations by their calling convention and chosen
library, preserving declaration order. It puts
`#[link(name = "...", kind = "dylib")]` on each actual matching foreign block.
Types, ordinary external declarations, exports, and unselected imports do not
receive that annotation. Empty library names and control characters are rejected;
other characters are escaped as Rust string literals. The caller still supplies
library search paths and distributes the required DLLs.

Selected imported data without a matching rule produces a diagnostic. A linker
argument alone, an empty separate foreign block, or a preceding attribute that
attaches to a type does not produce the required imported-data reference.
Imported functions without a rule retain ordinary external linkage, which can
use an import-library thunk. Supply a rule to request direct DLL import for their
addresses as well as calls. The annotation supplies Rust's import storage and
native library requirement; final symbol resolution still belongs to the linker.

Native Windows SDK and full consumer checks remain release gates. The reusable
[DLL probe](../scripts/verify_windows_dll_imports.py) has an explicit `--native`
mode for Windows. Cross-target linking is recorded separately from execution.
This layer adds no Windows ARM target.

## Evidence

The focused tests compare source acceptance with Clang, final file attributes
with its JSON AST, and external linkage with its LLVM output. Accepted and
rejected cases also compare ordinary analysis with retained analysis. The saved
[evidence](../corpus/evidence/dll-storage-2026-09-08/summary.json) includes the
primary compiler probes and source hashes.

The [inline composition probes](../corpus/evidence/dll-inline-composition-2026-09-08/summary.json)
preserve COFF objects and LLVM verifier results for imported and weak bodies.
The [declaration composition probes](../corpus/evidence/dll-composition-2026-09-08/summary.json)
cover hidden extern declarations, direct builtin calls, and first-declaration
address constants, including the failing inputs that led to the corrections.
The [combined validation](../corpus/evidence/dll-root-integration-2026-09-08/summary.json)
records the workspace, native compiler, retained-graph, unchanged-binding, and
ASan checks after integrating those corrections.

Separate COFF probes link a real C DLL and import library against C, Rust 1.64,
and current Rust consumers. Bindgen 0.72.1 reproduces the same imported-data
linking limitation across data-first, function-first, and typedef-first headers.
Those earlier probes validate compilation and linking; no PE executable was run.
The [scoped-library evidence](../corpus/evidence/dll-libraries-2026-09-08/summary.json)
records generated consumers against two DLLs, ordinary object linkage, all three
header orders, Rust 1.64/current, O0/O3, and the explicit execution status.
The [integrated library checks](../corpus/evidence/dll-libraries-root-integration-2026-09-08/summary.json)
repeat those 12 cross-linked consumers after the declaration corrections and
confirm that the eight existing project and musl binding outputs are unchanged.

Compiler rules were checked against Clang 18's
[declaration redeclaration handling](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDecl.cpp)
and [DLL attribute merging](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDeclAttr.cpp).
