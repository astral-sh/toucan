# Conformance and validation

Toucan's first supported integrations generate bindings for specific C libraries.
A passing consumer establishes that recorded path. It does not establish complete
C conformance or compatibility with every bindgen build script.

## What each check establishes

| Check | Evidence | Limit |
| --- | --- | --- |
| Accept valid source | The [external C suite](../corpus/conformance/README.md) compares untouched programs with GCC and Clang. Its strict gate requires acceptance of cases both compilers also accept with pedantic C11 flags. | Compiler agreement on a finite positive corpus does not prove all language rules or rejection of invalid source. |
| Exercise native preprocessing | The same audit can preprocess and analyze original source with Toucan, recording its profile, include search, macro configuration, actual header dependencies, and failures separately from the compiler-preprocessed route. | The profiles can select different conditional-header branches. The audit does not require identical predefined macros or byte-identical preprocessed text. |
| Check generated programs | The [Csmith audit](../corpus/conformance/csmith/README.md) checks generated source in both frontend profiles and compares complete declaration output with optional code retention enabled and disabled. | GCC/Clang runtime checks validate the source oracle. Toucan does not generate or execute machine code. |
| Diagnose unsupported or invalid input | [Semantic tests](../crates/toucan_semantic/tests) cover declaration, expression, initializer, and body constraints. Binding tests require explicit errors for selected unsupported Rust ABI representations. | We still need broader independent coverage of programs that require a diagnostic. A valid unsupported ABI and invalid C source are different outcomes. |
| Preserve C values and calling conventions | The [binding corpus](../corpus/README.md) compares constants, layouts, function signatures, and actual C/Rust calls. [Consumer gates](replacement-readiness.md) build the Rust wrappers and applications that consume generated output. | A layout match alone does not establish argument passing, wrapper compatibility, or every library configuration. |
| Bound malformed input | [Parser limits](parser-limits.md), [fuzzing](../fuzz/README.md), and regression tests exercise preprocessing, analysis, retained code, and binding generation. | Bounded campaigns and resource counters do not prove memory safety or a universal runtime/memory bound. |

The acceptance reports preserve exploratory differences alongside the strict
subset. Cases rejected by either reference compiler are not silently counted as
successful Toucan conformance tests. Crashes, timeouts, invalid tool output, and
changed inputs are infrastructure failures, not ordinary language differences.

## Remaining work

For a general frontend release, broaden both positive and negative language
coverage, continue native-preprocessor comparisons across real header environments,
and exercise the declared target/profile combinations. An independent negative
corpus should distinguish required C diagnostics from optional warnings and
unsupported compiler extensions.

For a binding replacement, complete the selected Builder API and Rust ABI
representations needed by adopting projects, then repeat their actual builds,
layouts, calls, and runtime checks on the intended release revision. The
[compatibility matrix](compatibility.md#current-gaps) names unsupported forms;
[replacement readiness](replacement-readiness.md) tracks consumer and target gates.

The [Linux uv/ty opt-in](opt-in-rollout.md) has passed its selected application
gate. Its Git dependency trial and upstream integration work are separate from
the broader language and target coverage needed for a general replacement.
