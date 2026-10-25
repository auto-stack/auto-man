#!/usr/bin/env python3
"""Build, install, and verify PLAN-832 Man Core/full profiles.

Layout: auto(.exe), optional auto-ui-tool(.exe), sdk/, install-manifest.json.
Preparation happens in a unique marked staging directory. Existing managed
installations are moved to a unique preserved backup before swap; unknown
non-empty directories are never claimed or deleted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

MAN = Path(__file__).resolve().parent.parent
GROUP = MAN.parent
LANG = GROUP / "auto-lang"
UI = GROUP / "auto-ui"
DOWN = GROUP / "auto-down"
MANIFEST = "install-manifest.json"
ROOT_INPUTS = [
    "stdlib/assembly-providers.json",
    "stdlib/auto/image.at",
    "stdlib/auto/image.rs.at",
    "stdlib/auto/image.vm.at",
    "docs/schema/aura.at",
]
SCRUB = {
    "AUTO_LANG_ROOT", "AUTO_MAN_ROOT", "AUTO_UI_ROOT", "AUTO_DOWN_ROOT",
    "AUTO_UI_TOOL_BIN", "AUTO_RUST_WORKSPACE", "AUTO_LANG_CRATE",
    "AUTO_DELIVERY_SOURCE_ROOT", "AUTO_UI_SOURCE_ROOT", "CARGO_TARGET_DIR",
    "RUSTC_WRAPPER",
}


def exe(name: str) -> str:
    return name + (".exe" if os.name == "nt" else "")


def run(cmd: list[str], cwd: Path, timeout: int = 7200) -> None:
    env = {k: v for k, v in os.environ.items() if k not in SCRUB}
    proc = subprocess.run(cmd, cwd=str(cwd), env=env, timeout=timeout)
    if proc.returncode:
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(cmd)}")


def head(root: Path) -> str:
    return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def clean_tree(src: Path, dst: Path) -> None:
    def ignore(_dir, names):
        return {name for name in names if name in {".git", "target", "__pycache__"}}
    shutil.copytree(src, dst, ignore=ignore)


def metadata_package_dirs(profile: str) -> set[Path]:
    cmd = ["cargo", "metadata", "--format-version", "1"]
    if profile == "core":
        cmd += ["--no-default-features", "--features", "core"]
    metadata = json.loads(subprocess.check_output(cmd, cwd=str(MAN), text=True))
    root_id = next(p["id"] for p in metadata["packages"] if p["name"] == "auto")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    packages = {p["id"]: p for p in metadata["packages"]}
    seen = set()

    def visit(package_id: str) -> None:
        if package_id in seen:
            return
        seen.add(package_id)
        for dependency in nodes[package_id]["deps"]:
            visit(dependency["pkg"])

    visit(root_id)
    if profile == "full":
        for name in ("auto", "auto-lang", "auto-man"):
            visit(next(p["id"] for p in packages.values() if p["name"] == name))
    result = set()
    for package_id in seen:
        package = packages[package_id]
        if package.get("source") is None:
            result.add(Path(package["manifest_path"]).parent.resolve())
    return result


def repo_for(path: Path) -> tuple[str, Path]:
    for name, root in (("auto-man", MAN), ("auto-lang", LANG), ("auto-ui", UI), ("auto-down", DOWN)):
        try:
            path.relative_to(root)
            return name, root
        except ValueError:
            pass
    raise RuntimeError(f"unregistered path source: {path}")


def rewrite_manifest(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    text = text.replace("../../../auto-lang/crates/", "../")
    text = text.replace("../../../auto-ui/crates/", "../")
    path.write_text(text, encoding="utf-8", newline="\n")


def project_missing_optional(manifest: Path, installed: set[str]) -> list[str]:
    import re
    doc = tomllib.load(manifest.open("rb"))
    missing = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        for name, spec in doc.get(section, {}).items():
            if isinstance(spec, dict) and spec.get("optional") and "path" in spec and name not in installed:
                missing.add(name)
    if not missing:
        return []
    text = manifest.read_text(encoding="utf-8")
    edits = []
    for name in sorted(missing):
        pattern = re.compile(rf"^\s*{re.escape(name)}\s*=.*(?:\n|$)", re.MULTILINE)
        text, count = pattern.subn("", text)
        edits.append(f"{manifest.parent.name}:{name}:dep={count}")
        text, count = re.subn(rf'^\s*"dep:{re.escape(name)}"\s*,?\s*(?:\n|$)', "", text, flags=re.MULTILINE)
        edits.append(f"{manifest.parent.name}:{name}:feature_lines={count}")
        for prefix in (f'"dep:{name}", ', f', "dep:{name}"', f'"dep:{name}"'):
            text = text.replace(prefix, "")
        text, count = re.subn(rf',?\s*"{re.escape(name)}/[^"]*",?\s*', "", text)
        edits.append(f"{manifest.parent.name}:{name}:feature={count}")
    manifest.write_text(text, encoding="utf-8", newline="\n")
    return edits


def stage_sources(stage: Path, profile: str) -> dict:
    sdk = stage / "sdk"
    sdk.mkdir(parents=True)
    package_dirs = metadata_package_dirs(profile)
    packages = []
    installed = {source.name for source in package_dirs}
    projection_edits = []
    for source in sorted(package_dirs, key=lambda p: p.as_posix()):
        repo, root = repo_for(source)
        rel = source.relative_to(root)
        if repo == "auto-down":
            packages.append({"name": source.name, "source_repo": repo,
                             "source_path": rel.as_posix(), "installed_path": f"auto-down/{rel.as_posix()}"})
            continue
        if rel.parts[0] != "crates":
            raise RuntimeError(f"non-crate package input requires explicit mapping: {source}")
        target = sdk / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        clean_tree(source, target)
        manifest = target / "Cargo.toml"
        if manifest.is_file():
            rewrite_manifest(manifest)
        packages.append({"name": source.name, "source_repo": repo,
                         "source_path": rel.as_posix(), "installed_path": rel.as_posix()})
    for manifest in sdk.glob("crates/*/Cargo.toml"):
        projection_edits.extend(project_missing_optional(manifest, installed))

    for rel in ROOT_INPUTS:
        target = sdk / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(LANG / rel, target)
    clean_tree(LANG / "vendor/autodown-test-parser", sdk / "vendor/autodown-test-parser")
    clean_tree(LANG / "crates/autoui-skill", sdk / "crates/autoui-skill")
    shutil.copy2(MAN / "Cargo.lock", sdk / "Cargo.lock")
    workspace = (MAN / "Cargo.toml").read_text(encoding="utf-8")
    members_start = workspace.index("members = [")
    members_end = workspace.index("]\ndefault-members", members_start) + 1
    member_list = ", ".join(f'"{path}"' for path in sorted({p["installed_path"] for p in packages if p["source_repo"] != "auto-down"}))
    workspace = workspace[:members_start] + f"members = [{member_list}]" + workspace[members_end:]
    workspace = workspace.replace("../auto-lang/crates/patches/", "crates/patches/")
    if profile == "core":
        workspace = workspace.split("[patch.crates-io]", 1)[0].rstrip() + "\n"
    (sdk / "Cargo.toml").write_text(workspace, encoding="utf-8", newline="\n")
    run(["cargo", "generate-lockfile"], sdk)
    if profile == "full":
        clean_tree(DOWN, sdk / "auto-down")
        for rel in ("packages/blueprints", "packages/widgets"):
            clean_tree(LANG / rel, sdk / rel)
    for repo in ("auto-lang", "auto-man", "auto-ui"):
        license_root = {"auto-lang": LANG, "auto-man": MAN, "auto-ui": UI}[repo]
        target = stage / "licenses" / repo
        target.mkdir(parents=True, exist_ok=True)
        shutil.copy2(license_root / "LICENSE", target / "LICENSE")
    return {"sdk_layout": "combined-crates", "packages": packages,
            "projection_edits": projection_edits,
            "root_inputs": ROOT_INPUTS + ["vendor/autodown-test-parser", "crates/autoui-skill"]}


def manifest_files(root: Path) -> dict[str, dict]:
    files = {}
    for path in sorted(root.rglob("*"), key=lambda p: p.as_posix()):
        if path.is_file() and path.name != MANIFEST:
            files[path.relative_to(root).as_posix()] = {
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
            }
    return files


def verify(root: Path, expected_profile: str | None = None) -> dict:
    path = root / MANIFEST
    if not path.is_file():
        raise RuntimeError(f"missing {MANIFEST}: {root}")
    doc = json.loads(path.read_text(encoding="utf-8"))
    profile = doc["profile"]
    if expected_profile and profile != expected_profile:
        raise RuntimeError(f"profile mismatch: {profile} != {expected_profile}")
    actual = manifest_files(root)
    expected = doc["files"]
    if actual != expected:
        missing = sorted(set(expected) - set(actual))
        extra = sorted(set(actual) - set(expected))
        changed = sorted(k for k in set(actual) & set(expected) if actual[k] != expected[k])
        raise RuntimeError(f"install content mismatch: missing={missing}, extra={extra}, changed={changed}")
    names = {Path(p).name for p in actual}
    if profile == "core":
        forbidden = [p for p in actual if p.startswith("sdk/crates/auto-ui")
                     or p.startswith("sdk/crates/auto-lang/") or p.startswith("sdk/crates/shell-pack")
                     or p == exe("auto-ui-tool")]
        if forbidden:
            raise RuntimeError(f"UI source/tool in Core install: {forbidden}")
        if exe("auto") not in names:
            raise RuntimeError("Core auto binary missing")
    else:
        for required in (exe("auto"), exe("auto-ui-tool"), "sdk/crates/auto-ui-tools/Cargo.toml"):
            if required not in actual:
                raise RuntimeError(f"full profile required input missing: {required}")
    return {"profile": profile, "files": len(actual), "sources": doc["sources"]}


def unique_sibling(root: Path, kind: str) -> Path:
    return root.with_name(f".{root.name}.man-{kind}-{os.getpid()}-{time.time_ns()}")


def process_alive(pid: int) -> bool:
    if os.name == "nt":
        import ctypes
        handle = ctypes.windll.kernel32.OpenProcess(0x1000, False, pid)
        if not handle:
            return False
        ctypes.windll.kernel32.CloseHandle(handle)
        return True
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except (PermissionError, OSError):
        return True


def recover_stale_staging(root: Path) -> None:
    for candidate in root.parent.glob(f".{root.name}.man-staging-*"):
        marker = candidate / ".man-staging.marker"
        if not marker.is_file():
            continue
        try:
            owner = json.loads(marker.read_text(encoding="utf-8"))
            pid = int(owner["pid"])
            if owner.get("schema") != "man-staging/1":
                continue
        except (ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
        if not process_alive(pid):
            shutil.rmtree(candidate)


def install(profile: str, install_root: Path, fail_point: str | None, skip_build: bool) -> dict:
    root = install_root.resolve()
    root.parent.mkdir(parents=True, exist_ok=True)
    if root.exists() and any(root.iterdir()) and not (root / MANIFEST).is_file():
        raise RuntimeError(f"refusing non-empty unmanaged install root: {root}")
    recover_stale_staging(root)
    if not skip_build:
        if profile == "core":
            run(["cargo", "build", "--locked", "-p", "auto", "--no-default-features", "--features", "core"], MAN)
        else:
            run(["cargo", "build", "--locked", "-p", "auto"], MAN)
            run(["cargo", "build", "--locked", "-p", "auto-ui-tools"], UI)
    sources = {"auto-man": head(MAN), "auto-lang": head(LANG), "auto-ui": head(UI), "auto-down": head(DOWN)}
    stage = unique_sibling(root, "staging")
    backup = None
    try:
        stage.mkdir()
        (stage / ".man-staging.marker").write_text(
            json.dumps({"schema": "man-staging/1", "pid": os.getpid()}) + "\n", encoding="utf-8")
        if fail_point == "prepare":
            raise RuntimeError("injected prepare failure")
        source_receipt = stage_sources(stage, profile)
        shutil.copy2(MAN / "target/debug" / exe("auto"), stage / exe("auto"))
        if profile == "full":
            shutil.copy2(UI / "target/debug" / exe("auto-ui-tool"), stage / exe("auto-ui-tool"))
        doc = {
            "schema": "man-install/1", "profile": profile, "sources": sources,
            "source_closure": source_receipt,
            "files": manifest_files(stage),
        }
        (stage / MANIFEST).write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        verify(stage, profile)
        if fail_point == "verify":
            raise RuntimeError("injected verify failure")
        if root.exists():
            backup = unique_sibling(root, "backup")
            os.replace(root, backup)
            (backup / ".man-backup.marker").write_text("PLAN-832 preserved previous install\n", encoding="utf-8")
        if fail_point == "swap":
            raise RuntimeError("injected swap failure")
        os.replace(stage, root)
        result = verify(root, profile)
        result.update({"install_root": str(root), "backup_root": str(backup) if backup else None,
                       "manifest": str(root / MANIFEST)})
        return result
    except Exception:
        if backup and backup.exists() and not root.exists():
            os.replace(backup, root)
        if stage.exists() and (stage / ".man-staging.marker").is_file():
            shutil.rmtree(stage)
        raise


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    install_parser = sub.add_parser("install")
    install_parser.add_argument("--profile", choices=("core", "full"), required=True)
    install_parser.add_argument("--install-root", type=Path, required=True)
    install_parser.add_argument("--fail-point", choices=("prepare", "verify", "swap"))
    install_parser.add_argument("--skip-build", action="store_true")
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--install-root", type=Path, required=True)
    verify_parser.add_argument("--profile", choices=("core", "full"))
    args = parser.parse_args(argv)
    try:
        if args.command == "install":
            result = install(args.profile, args.install_root, args.fail_point, args.skip_build)
        else:
            result = verify(args.install_root.resolve(), args.profile)
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
        return 0
    except Exception as exc:
        print(f"man-install: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
