#!/usr/bin/env python3
"""Exercise version rejection against a copied, previously valid installation."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

INSTALLER = Path(__file__).resolve().with_name("man-install.py")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(root, *args, success=True, message=None):
    result = subprocess.run([sys.executable, str(INSTALLER), *args, "--install-root", str(root)],
                            capture_output=True, text=True, encoding="utf-8", errors="replace")
    output = result.stdout + result.stderr
    if (result.returncode == 0) != success or (message and message not in output):
        raise AssertionError(f"{' '.join(args)}: exit={result.returncode}: {output}")
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    args = parser.parse_args()
    source, root = args.source.resolve(), args.scratch.resolve()
    if root.exists() or source == root or source in root.parents or root in source.parents:
        raise ValueError("scratch must be a new path outside the source installation")
    run(source, "verify", success=True)
    shutil.copytree(source, root)
    receipt = root / "install-manifest.json"
    executable = root / ("auto.exe" if sys.platform == "win32" else "auto")
    original = receipt.read_bytes()
    original_exec = digest(executable)
    doc = json.loads(original)
    for key, value, expected in (("schema", "man-install/999", "unsupported install manifest schema"),
                                  ("profile", "future", "unsupported install profile")):
        altered = dict(doc)
        altered[key] = value
        receipt.write_text(json.dumps(altered, ensure_ascii=False) + "\n", encoding="utf-8")
        altered_bytes = receipt.read_bytes()
        run(root, "verify", success=False, message=expected)
        run(root, "install", "--profile", "core", "--skip-build", "--fail-point", "prepare",
            success=False, message=expected)
        if receipt.read_bytes() != altered_bytes or digest(executable) != original_exec:
            raise AssertionError(f"{key} rejection changed the previous installation")
        print(f"{key}: verify and upgrade rejected, previous bytes preserved")
    receipt.write_bytes(original)
    file = root / "sdk/crates/auto-core/Cargo.toml"
    original_file = file.read_bytes()
    try:
        file.write_bytes(original_file + b"\n# plan842 tamper probe\n")
        tampered = digest(file)
        run(root, "install", "--profile", "core", "--skip-build", "--fail-point", "prepare",
            success=False, message="install content mismatch")
        if digest(file) != tampered or digest(executable) != original_exec:
            raise AssertionError("tampered install was changed")
        print("tamper: upgrade rejected, previous bytes preserved")
    finally:
        file.write_bytes(original_file)
    run(root, "verify", success=True)
    run(root, "install", "--profile", "core", "--skip-build", "--fail-point", "prepare",
        success=False, message="injected prepare failure")
    if receipt.read_bytes() != original or digest(executable) != original_exec:
        raise AssertionError("prepare failure changed valid prior installation")
    run(root, "verify", success=True)
    print("valid older install: accepted for upgrade, injected failure preserved and verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
