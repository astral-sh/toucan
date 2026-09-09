#!/usr/bin/env python3
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

binary = Path(os.environ["TOUCAN_BINDGEN_BINARY"])
trace = Path(os.environ["TOUCAN_BINDGEN_TRACE"])
args = sys.argv[1:]
result = subprocess.run([str(binary), *args], check=False)
output = Path(args[args.index("--output") + 1]) if "--output" in args else None
record = {
    "generator_binary": str(binary),
    "generator_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "argv": args,
    "exit_code": result.returncode,
    "output": str(output) if output is not None else None,
    "output_sha256": hashlib.sha256(output.read_bytes()).hexdigest() if output is not None and output.is_file() else None,
}
with trace.open("a") as file:
    file.write(json.dumps(record) + "\n")
sys.exit(result.returncode)
