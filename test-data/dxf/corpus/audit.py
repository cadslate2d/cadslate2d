#!/usr/bin/env python3
"""Audits CadSlate2D's export of every corpus file with ezdxf.

For each `.dxf` here: our writer re-exports it (`cargo run -p cad-dxf --example
reexport`), and ezdxf 1.4.4 reads the result in recovery mode, which reports
the errors it found and the fixes it had to make. A clean audit is 0 and 0.
Run from the repository root, with ezdxf installed as `generate.py` says:

    .venv/bin/python3 test-data/dxf/corpus/audit.py

The outcome is recorded by hand in `test-data/dxf/README.md`; this script is
not part of the check, since the check has no Python.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path

from ezdxf import recover

HERE = Path(__file__).resolve().parent


def main() -> None:
    files = sorted(HERE.glob("*.dxf"))
    errors = fixes = 0
    with tempfile.TemporaryDirectory() as scratch:
        for file in files:
            out = Path(scratch) / file.name
            text = subprocess.run(
                ["cargo", "run", "-q", "-p", "cad-dxf", "--example", "reexport", "--", str(file)],
                check=True,
                capture_output=True,
                text=True,
            ).stdout
            out.write_text(text)
            doc, auditor = recover.readfile(str(out))
            errors += len(auditor.errors)
            fixes += len(auditor.fixes)
            status = "clean" if not auditor.errors and not auditor.fixes else "NOT CLEAN"
            print(f"{file.name:32s} {doc.dxfversion} {len(doc.modelspace()):3d} entities  {status}")
            for problem in list(auditor.errors) + list(auditor.fixes):
                print("    ", problem.message)
    print(f"{len(files)} files: {errors} errors, {fixes} fixes")


if __name__ == "__main__":
    main()
