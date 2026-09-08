# Declaration alignment

Object and function alignment is separate from C type layout. For example,
`int x __attribute__((aligned(32)))` gives `x` an explicit alignment of 32 bytes;
`int` still has its target's ordinary alignment. The frontend retains this
information for file declarations, local variables, parameters and record members.

`Declaration::alignment` exposes the GNU attribute, C11 `_Alignas` requirement,
and effective alignment through byte-valued getters. The validating constructor
accepts powers of two up to 2^28. `_Alignas(0)` remains explicit: it has no layout
effect, but Clang checks its presence when comparing declarations.

`TranslationUnit::declaration_alignment` computes an object's alignment without
changing its type. In retained code, `DeclarationSite::alignment` describes the
written declaration and `effective_alignment` includes requirements inherited at
that source position. An entity with a file declaration follows that declaration;
a block declaration can have different visible requirements. Record members retain
the annotations; their final alignment also depends on the containing record's
packing and field-layout rules.

## Compiler rules

The frontend follows the selected compiler profile:

- GNU attributes may lower an individual object's alignment. GNU redeclarations
  preserve the maximum, including earlier implicit natural alignment. Clang
  retains lower explicit alignment when later declarations omit an attribute.
- A block `extern` does not uniformly change a visible file declaration. GNU
  propagates increases; Clang keeps the file declaration's alignment separate.
  If the first linked declaration occurs in a block, Clang preserves its
  alignment across later declarations in that block or another block.
- `_Alignas` cannot reduce natural alignment on complete object types. GNU and
  Clang differ when GNU attributes are combined with `_Alignas`, and Clang defers
  this constraint for incomplete array types.
- Clang requires matching C11 requirements on redeclarations and requires
  `_Alignas` on the definition when a relevant earlier declaration specified it.
- `_Alignas` is rejected on functions, parameters, register objects, bitfields,
  typedefs and type names. GNU-aligned parameters are rejected by the GNU profile
  and retained as parameter storage metadata by Clang; parameter types are unchanged.

Alignment operands are checked without executing them. New VLA bounds and `typeof`
operands within an alignment specifier or GNU alignment argument are retained as
unevaluated. Previously declared VLA bounds keep their original identity and
execution context.

## Rust bindings

Ordinary and increased object alignment preserve the ordinary Rust storage type.
A selected object whose C alignment is smaller than that Rust type's alignment
produces a diagnostic. Emitting an ordinary `extern static` would otherwise promise
alignment the C declaration does not provide. Filtering out that object still
allows the remaining interface to be generated.

The [evidence report](../corpus/evidence/declaration-alignment-2026-09-08.json)
records compiler constraints, native C/Rust access, cross-target record layouts,
and memory measurements on pinned real projects. Expression forms of GNU
`__alignof__` remain a separate extension; this layer retains their prerequisite
object and function facts.

## Resource measurements

The pinned before/after check covers four real header interfaces and seven
compiler-preprocessed source files, with three paired samples in ordinary and
retained modes. All 132 runs accept their input, and the generated header bindings
are byte-for-byte unchanged. Ordinary allocation counts are unchanged; requested
bytes increase by 0.03–0.89%. Retained median allocation counts increase by zero to three, including optional
metadata for annotations already present in those inputs. A few retained header
samples vary by one allocation; the report preserves each sample.

`ExpressionInfo` remains 64 bytes and `Entity` remains 80 bytes on the measured
x86-64 host. A declaration grows by 8 bytes, as does a retained declaration site
and a lexical scope. Alignment values occupy 3 bytes; the optional written and
inherited site payload occupies 6 bytes and is charged to the retention quota.
These are instrumented resource measurements, not a throughput guarantee.

The [integration report](../corpus/evidence/declaration-alignment-integration-2026-09-08.json)
records the same-block Clang correction, focused compiler tests, generated Rust
1.64 storage checks, and the retention-quota regression.
