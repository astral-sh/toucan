# C type oracles

These compile-time assertions resolve specific differences found while comparing
Toucan with bindgen 0.72.1. Each fixture must follow an include of its original,
pinned project header. It does not include generated Rust or link a library.

| Fixture | What the C compiler checks |
| --- | --- |
| `sqlite.c` | `sqlite3_vfs.xDlSym` returns `void (*)(void)`. A return callback that repeats the lookup's three parameters is incompatible. `sqlite3_version` has `const char` elements. |
| `zstd.c` | `ZSTD_CONTENTSIZE_UNKNOWN` and `ZSTD_CONTENTSIZE_ERROR` are unsigned 64-bit values `2^64 - 1` and `2^64 - 2`. Representative enumerator expressions have type `int`. |
| `libgit2.c` | `GIT_REBASE_NO_OPERATION` has unsigned `size_t` type and value `SIZE_MAX`. Representative enumerator expressions have type `int`. |

The enum checks concern the C expression's type. A Rust generator can choose to
give constants the enum's compatible integer type; that is a source API choice,
and should remain visible in a comparison even when the values and enum ABI agree.
The sentinel checks also verify width and signedness: comparing only a value after
casting to a wide unsigned integer can hide a signed `-1` or `-2` constant.

All three fixtures passed with GCC 13.3.0 and Clang 18.1.3 on x86_64 Linux on
2026-09-08, using C11 and `-Wall -Wextra -Werror`. This is compile-time evidence;
these fixtures do not exercise runtime FFI calls or establish equivalence for
other declarations.

## Reproduce

Prepare the pinned projects as described in [the corpus README](../README.md).
From the repository root, run the following with the resulting cache path:

```sh
python3 - /path/to/corpus-cache <<'PY'
import json
from pathlib import Path
import shlex
import subprocess
import sys

prepared = json.loads((Path(sys.argv[1]) / "prepared.json").read_text())
for compiler in ("gcc", "clang"):
    for project in prepared["projects"]:
        fixture = Path("corpus/oracles") / (project["name"] + ".c")
        if not fixture.exists():
            continue
        command = [
            compiler, "-std=c11", "-Wall", "-Wextra", "-Werror",
            "-fsyntax-only", "-include", project["header"],
        ]
        for directory in project["include_dirs"]:
            command.extend(["-I", directory])
        command.append(str(fixture))
        print(shlex.join(command), flush=True)
        subprocess.run(command, check=True)
PY
```

The corpus harness can also append a fixture after its header include and before
its generated probe `main`. The fixtures declare no functions and require no
linker inputs. Their 64-bit sentinel expectations cover the current Linux and
macOS corpus targets; a 32-bit target needs a separate size expectation for
`size_t`.
