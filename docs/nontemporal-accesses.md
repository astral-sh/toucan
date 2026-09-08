# Non-temporal memory accesses

Clang profiles support `__builtin_nontemporal_load(address)` and
`__builtin_nontemporal_store(value, address)` for integer, enumeration, `_Bool`,
real floating, pointer, and fixed-vector storage. The address selects the memory
access type. The store converts its value as a function argument; equal-sized
vectors use Clang's bit reinterpretation. A load produces an unqualified value,
retaining the pointee's typedef alignment. GCC profiles do not provide these
spellings.

The operations carry a cache hint that a backend may ignore. They do not provide
atomic access, synchronization, or an instruction-set requirement. Clang removes
the pointee's const and volatile qualifiers from the access itself. Evaluating a
volatile address or stored-value expression still performs the corresponding
volatile read. A const-qualified pointer can address mutable storage; this does
not authorize modifying an actually const object. C11 atomic storage is rejected,
while storing a pointer to an atomic object remains an ordinary pointer access.

## Checked code

`Builtin::Nontemporal(NontemporalOperation::Load | Store)` identifies the memory
operation and its hint. Each argument has an ordinary `ExprUse` edge; the original
address qualifiers remain available. `Conversion::VectorReinterpret` preserves
bits across vector element types and never requests numeric lane conversion.
Other stored values use ordinary assignment conversions, including atomic loads
needed to obtain an argument value. The hint does not promise a relative order
between evaluations of the stored value and address.

Pointer-to-VLA load results project their `TypeUse` from the address, retaining
bound identities. Constant and object-size query contexts suppress accesses as
specified by their query metadata. Clang classifies these builtins as effectful
for its syntactic query gate, including non-volatile loads; Toucan does not fold
memory contents into C constants.

## Validation and current boundaries

Native tests use independently compiled C callers, check each argument once,
exercise scalar conversion and vector bit patterns, and place explicit fences
between non-temporal stores and subsequent reads. Separate LLVM probes verify
non-temporal metadata, ordinary pointee accesses, and volatile argument reads.
No Rust vector call ABI or binding representation changes are included.

Complex memory accesses are outside this slice's baseline and require the
complex-aware follow-up. Clang accepts their source types, but some Clang 18
lowering paths crash while others compile. Compiler failures are recorded
separately from constraint diagnostics. The existing vector size policy and
strict pointer-qualification assignment diagnostics still apply. Microsoft
forward-enum accesses receive an explicit unsupported-layout diagnostic until
that enum extension has a complete frontend representation.

Primary behavior follows the pinned
[Clang checker](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaChecking.cpp)
and [Clang lowering](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/CodeGen/CGBuiltin.cpp).
[Evidence](../corpus/evidence/nontemporal-2026-09-08.json) records exact commands,
compiler identities, the phase-specific complex defects, allocation checks, and
unchanged project inputs.
