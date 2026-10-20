#!/usr/bin/env python3
"""The JavaScript packages of the web bundle, each with its licence text (#296).

Prints the notices for the production dependencies of web/, from
`pnpm licenses list --prod --json` and each package's own LICENSE file.
"""
import glob
import json
import os
import subprocess

report = json.loads(
    subprocess.run(
        ["pnpm", "--dir", "web", "licenses", "list", "--prod", "--json"],
        capture_output=True, text=True, check=True,
    ).stdout
)
packages = sorted(
    (package for packages in report.values() for package in packages),
    key=lambda package: package["name"],
)
for package in packages:
    print("=" * 80)
    print(f"{package['name']} {', '.join(package['versions'])} ({package['license']})")
    if package.get("homepage"):
        print(package["homepage"])
    print("-" * 80)
    licence = next(
        (name for path in package["paths"] for name in sorted(glob.glob(os.path.join(path, "LICEN[CS]E*")))),
        None,
    )
    if licence:
        with open(licence, encoding="utf-8") as text:
            print(text.read().rstrip())
    else:
        print(f"(no licence file in the package; its declared licence is {package['license']})")
    print()
