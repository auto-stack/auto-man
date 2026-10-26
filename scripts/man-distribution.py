#!/usr/bin/env python3
"""PLAN-832 independent clone, relocation, install, and failure-recovery gate."""

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

MAN = Path(__file__).resolve().parent.parent
GROUP = MAN.parent
LANG = GROUP / "auto-lang"
UI = GROUP / "auto-ui"
DOWN = GROUP / "auto-down"
FIXED = {
    "auto-man": "a8373e660f47ce1c0e90722c5240359155b81409",
    "auto-lang": "b69da4dde32d1e7226f836e4086c3ebe03ad7ba1",
    "auto-ui": "6623442189f208e4f806ddffe446c8196775b1d8",
    "auto-down": "c39ac7f9dacdc769fc4a5ad3b98ec896240b28e8",
}
SCRUB = {
    "AUTO_LANG_ROOT", "AUTO_MAN_ROOT", "AUTO_UI_ROOT", "AUTO_DOWN_ROOT",
    "AUTO_UI_TOOL_BIN", "AUTO_RUST_WORKSPACE", "AUTO_LANG_CRATE",
    "AUTO_DELIVERY_SOURCE_ROOT", "AUTO_UI_SOURCE_ROOT", "CARGO_TARGET_DIR",
    "RUSTC_WRAPPER", "TE_CHILD_BIN", "AUTO_TE_CHILD_BIN",
    "AUTO_BLUEPRINTS_ROOT", "CARGO_MANIFEST_DIR",
}


def exe(name: str) -> str:
    return name + (".exe" if os.name == "nt" else "")


def env(extra: dict[str, str] | None = None) -> dict[str, str]:
    result = {k: v for k, v in os.environ.items() if k not in SCRUB}
    if extra:
        result.update(extra)
    return result


def head(root: Path) -> str:
    return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Gate:
    def __init__(self, scratch: Path):
        self.scratch = scratch
        self.scratch.mkdir(parents=True)
        (self.scratch / ".plan832-scratch").write_text("owned verification scratch\n")
        self.receipt = {
            "schema": "man-distribution/1", "plan": "PLAN-832",
            "expected_sources": FIXED, "actual_sources": {}, "cases": [],
        }
        self.out = self.scratch / "distribution-receipt.json"

    def log(self, case: str, message: str) -> None:
        print(f"[{case}] {message}", flush=True)

    def run(self, case: str, cmd: list[str], cwd: Path, extra: dict[str, str] | None = None,
            timeout: int = 7200, expect: tuple[int, ...] = (0,)) -> subprocess.CompletedProcess:
        self.log(case, "$ " + " ".join(str(c) for c in cmd))
        start = time.monotonic()
        proc = subprocess.run(cmd, cwd=str(cwd), env=env(extra), capture_output=True,
                              text=True, encoding="utf-8", errors="replace", timeout=timeout)
        log = self.scratch / "logs" / f"{case}.log"
        log.parent.mkdir(exist_ok=True)
        log.write_text(proc.stdout + "\n--- stderr ---\n" + proc.stderr, encoding="utf-8")
        if proc.returncode not in expect:
            raise RuntimeError(f"{case}: exit {proc.returncode}, expected {expect}; log={log}")
        self.log(case, f"exit={proc.returncode} ({time.monotonic() - start:.1f}s)")
        return proc

    def case(self, case_id: str, fn) -> None:
        try:
            detail = fn() or "pass"
            self.receipt["cases"].append({"id": case_id, "status": "pass", "detail": detail})
            self.log(case_id, f"PASS {detail}")
            self.write()
        except Exception as exc:
            self.receipt["cases"].append({"id": case_id, "status": "fail", "detail": str(exc)})
            self.write()
            raise

    def write(self) -> None:
        summary = {"pass": sum(c["status"] == "pass" for c in self.receipt["cases"]),
                   "fail": sum(c["status"] == "fail" for c in self.receipt["cases"])}
        self.receipt["summary"] = summary
        self.out.write_text(json.dumps(self.receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scratch", type=Path,
                        default=GROUP.parent / f"m{os.getpid()}")
    args = parser.parse_args(argv)
    FIXED["auto-man"] = head(MAN)
    FIXED["auto-lang"] = head(LANG)
    gate = Gate(args.scratch.resolve())
    try:
        gate.case("SRC-1-clone", lambda: clone(gate))
        gate.case("DST-1-fresh-lock", lambda: fresh_lock(gate))
        gate.case("DST-2-core-locked-build", lambda: core_build(gate))
        gate.case("DST-3-full-install", lambda: full_install(gate))
        gate.case("DST-4-third-party-consumer", lambda: third_party(gate))
        gate.case("DST-5-installed-ui-build", lambda: installed_ui(gate))
        gate.case("DST-6-relocate", lambda: relocate(gate))
        gate.case("DST-7-upgrade-backup", lambda: upgrade_backup(gate))
        gate.case("FLR-1-unmanaged-preserved", lambda: unmanaged(gate))
        gate.case("FLR-2-prepare-failure", lambda: failure(gate, "prepare"))
        gate.case("FLR-3-half-install-recovery", lambda: failure(gate, "swap"))
        gate.case("FLR-4-tamper-detection", lambda: tamper(gate))
        gate.case("FLR-5-missing-tool-no-side-effect", lambda: missing_tool(gate))
        gate.case("FLR-6-recovery-green", lambda: recovery(gate))
    except Exception as exc:
        print(f"man-distribution: {exc}", file=sys.stderr)
        print(f"receipt={gate.out}", file=sys.stderr)
        return 2
    print(f"PASS all {len(gate.receipt['cases'])} cases; receipt={gate.out}")
    return 0


def clone(gate: Gate) -> str:
    group = gate.scratch / "grp-a"
    group.mkdir()
    for name, src in (("auto-man", MAN), ("auto-lang", LANG), ("auto-ui", UI), ("auto-down", DOWN)):
        gate.run("clone", ["git", "clone", "--quiet", "--local", str(src), str(group / name)], gate.scratch, timeout=600)
        actual = head(group / name)
        if actual != FIXED[name]:
            raise RuntimeError(f"{name} clone HEAD {actual} != fixed {FIXED[name]}")
        gate.receipt["actual_sources"][name] = actual
    core_group = gate.scratch / "grp-core"
    core_group.mkdir()
    for name in ("auto-man", "auto-lang"):
        gate.run("clone-core", ["git", "clone", "--quiet", "--local", str(group / name),
                                str(core_group / name)], gate.scratch, timeout=600)
    if (core_group / "auto-ui").exists() or (core_group / "auto-down").exists():
        raise RuntimeError("Core clone must not contain UI/down sources")
    return "four full clones plus isolated Man/lang Core clones match fixed commits"


def fresh_lock(gate: Gate) -> str:
    root = gate.scratch / "grp-a/auto-man"
    (root / "Cargo.lock").unlink()
    gate.run("fresh-lock", ["cargo", "generate-lockfile"], root, timeout=1800)
    core = gate.scratch / "grp-core/auto-man"
    (core / "core/Cargo.lock").unlink()
    gate.run("fresh-core-lock", ["cargo", "generate-lockfile", "--manifest-path", "core/Cargo.toml"],
             core, timeout=1800)
    return "fresh full and Core locks regenerated in their own workspaces"


def core_build(gate: Gate) -> str:
    root = gate.scratch / "grp-core/auto-man"
    gate.run("core-locked-build", ["cargo", "build", "--locked", "--manifest-path", "core/Cargo.toml",
                                  "-p", "auto", "--no-default-features", "--features", "core"], root)
    for edges in ("normal,build", "normal,build,dev"):
        tree = gate.run("core-tree", ["cargo", "tree", "--locked", "--manifest-path", "core/Cargo.toml",
                                      "--edges", edges, "--prefix", "none"], root).stdout
        if any(line.startswith(("auto-ui", "shell-pack", "iced", "auto-lang v", "auto-man v"))
               for line in tree.splitlines()):
            raise RuntimeError(f"forbidden UI/compat package in Core {edges} closure")
    install = gate.scratch / "install-core"
    gate.run("core-install", ["python", "scripts/man-install.py", "install", "--profile", "core",
                              "--install-root", str(install)], root)
    gate.run("core-verify", ["python", "scripts/man-install.py", "verify", "--install-root",
                             str(install), "--profile", "core"], root)
    gate.run("core-sdk-check", ["cargo", "check", "--locked", "--manifest-path",
                                str(install / "sdk/Cargo.toml"), "--no-default-features", "--features", "core"],
             gate.scratch)
    probe = gate.scratch / "core-probe.at"
    probe.write_text('fn main() {\n    print("core-independent-ok")\n}\n', encoding="utf-8")
    result = gate.run("core-installed-run", [str(install / exe("auto")), str(probe)], gate.scratch)
    if "core-independent-ok" not in result.stdout:
        raise RuntimeError("independent Core install failed to execute language script")
    denied = gate.run("core-ui-denied", [str(install / exe("auto")), "ui", "list"], gate.scratch,
                      expect=(1, 2, 3))
    if "ui-extra" not in denied.stdout + denied.stderr and "full profile" not in denied.stdout + denied.stderr:
        raise RuntimeError("Core UI request did not name the missing profile")
    return "no-UI/down Core clone builds, installs, rebuilds SDK and runs language/UI-denial commands"


def full_install(gate: Gate) -> str:
    root = gate.scratch / "grp-a/auto-man"
    install = gate.scratch / "install-a"
    gate.run("full-install", ["python", "scripts/man-install.py", "install", "--profile", "full",
                              "--install-root", str(install)], root)
    gate.run("full-verify", ["python", "scripts/man-install.py", "verify", "--install-root",
                             str(install), "--profile", "full"], root)
    gate.run("full-metadata", ["cargo", "metadata", "--locked", "--manifest-path",
                               str(install / "sdk/Cargo.toml"), "--no-deps", "--format-version", "1"],
             gate.scratch)
    return "full install verified with locked SDK metadata"


def third_party(gate: Gate) -> str:
    group = gate.scratch / "grp-a"
    project = gate.scratch / "third-party"
    project.mkdir()
    (project / "src").mkdir()
    (project / "Cargo.toml").write_text("""[package]
name = "three-repository-external"
version = "0.1.0"
edition = "2021"

[dependencies]
auto-man-core = { path = "../grp-a/auto-man/crates/auto-man-core" }
auto-lang-core = { path = "../grp-a/auto-lang/crates/auto-lang-core" }
auto-tool-protocol = { path = "../grp-a/auto-lang/crates/auto-tool-protocol" }
auto-project-model = { path = "../grp-a/auto-lang/crates/auto-project-model" }
auto-val = { path = "../grp-a/auto-lang/crates/auto-val" }
serde_json = "1"

[workspace]
""", encoding="utf-8")
    source = group / "auto-lang/test/fixtures/modularity/three-repository-consumer/src/lib.rs"
    shutil.copy2(source, project / "src/lib.rs")
    gate.run("third-party-lock", ["cargo", "generate-lockfile"], project, timeout=1800)
    gate.run("third-party-test", ["cargo", "test", "--locked"], project)
    tree = gate.run("third-party-tree", ["cargo", "tree", "--locked", "--edges", "normal,build",
                                         "--prefix", "none"], project).stdout
    lines = {line.removesuffix(" (*)") for line in tree.splitlines() if line.strip()}
    for required in ("auto-man-core", "auto-lang-core", "auto-tool-protocol", "auto-project-model"):
        if sum(line.split()[0] == required for line in lines) != 1:
            raise RuntimeError(f"third-party source identity invalid: {required}")
    if any(line.startswith(("auto-ui", "auto-lang v", "auto-man v")) for line in lines):
        raise RuntimeError("UI or compatibility implementation in third-party Core closure")
    return "external crate executes management + SDK + protocol with one source"


def installed_blueprints(gate: Gate) -> str:
    install = gate.scratch / "install-a"
    spec = install / "sdk/packages/blueprints/form/login/spec.md"
    original = spec.read_bytes()
    marker = b"PLAN832_INSTALLED_SDK_ONLY"
    try:
        spec.write_bytes(original + b"\n" + marker + b"\n")
        shown = gate.run("installed-blueprints", [str(install / exe("auto")), "bp", "show", "form/login"],
                         gate.scratch)
        if marker.decode() not in shown.stdout:
            raise RuntimeError("installed CLI did not read its own SDK blueprint")
    finally:
        spec.write_bytes(original)
    return "installed CLI reads unique marker from its own SDK, not development sources"


def installed_ui(gate: Gate) -> str:
    blueprint_detail = installed_blueprints(gate)
    install = gate.scratch / "install-a"
    project = gate.scratch / "ui-project"
    shutil.copytree(gate.scratch / "grp-a/auto-lang/examples/ui/001-helloworld", project)
    auto = install / exe("auto")
    gate.run("installed-gen", [str(auto), "gen"], project, timeout=300)
    gate.run("installed-build", [str(auto), "build"], project, timeout=600)
    if not (project / "gen/front/vue/dist/index.html").is_file():
        raise RuntimeError("installed Vue dist missing")
    return f"installed full CLI generates and builds real Vue app; {blueprint_detail}"


def relocate_core(gate: Gate) -> str:
    source = gate.scratch / "grp-core"
    for pattern in ("*/target", "*/core/target"):
        for target in source.glob(pattern):
            shutil.rmtree(target)
    shutil.rmtree(gate.scratch / "install-core/sdk/target", ignore_errors=True)
    moved = gate.scratch / "grp-core-b"
    shutil.move(str(source), moved)
    gate.run("relocate-core-independent", ["cargo", "build", "--locked", "--manifest-path", "core/Cargo.toml",
                                           "-p", "auto", "--no-default-features", "--features", "core"],
             moved / "auto-man")
    install = gate.scratch / "install-core-b"
    shutil.move(str(gate.scratch / "install-core"), install)
    gate.run("relocate-core-verify", ["python", "scripts/man-install.py", "verify", "--install-root",
                                      str(install), "--profile", "core"], moved / "auto-man")
    gate.run("relocate-core-metadata", ["cargo", "metadata", "--locked", "--manifest-path",
                                        str(install / "sdk/Cargo.toml"), "--no-deps", "--format-version", "1"],
             gate.scratch)
    result = gate.run("relocate-core-script", [str(install / exe("auto")),
                                                str(gate.scratch / "core-probe.at")], gate.scratch)
    if "core-independent-ok" not in result.stdout:
        raise RuntimeError("relocated Core install failed to execute language script")
    return "Core source and install relocate after deleting every original target"


def relocate(gate: Gate) -> str:
    core_detail = relocate_core(gate)
    source = gate.scratch / "grp-a"
    for target in source.glob("*/target"):
        shutil.rmtree(target)
    moved = gate.scratch / "grp-b"
    shutil.move(str(source), moved)
    gate.run("relocate-core", ["cargo", "build", "--locked", "--manifest-path", "core/Cargo.toml",
                              "-p", "auto", "--no-default-features", "--features", "core"], moved / "auto-man")
    install_b = gate.scratch / "install-b"
    shutil.move(str(gate.scratch / "install-a"), install_b)
    gate.run("relocate-verify", ["python", "scripts/man-install.py", "verify",
                                 "--install-root", str(install_b), "--profile", "full"],
             moved / "auto-man")
    probe = gate.scratch / "relocate-probe.at"
    probe.write_text('fn main() {\n    print("relocated")\n}\n', encoding="utf-8")
    result = gate.run("relocate-script", [str(install_b / exe("auto")), str(probe)], gate.scratch)
    if "relocated" not in result.stdout:
        raise RuntimeError("relocated installed auto failed to execute script")
    gate.run("relocate-metadata", ["cargo", "metadata", "--locked", "--manifest-path",
                                   str(install_b / "sdk/Cargo.toml"), "--no-deps", "--format-version", "1"],
             gate.scratch)
    return "source group and installation relocate without original target/source fallback"


def upgrade_backup(gate: Gate) -> str:
    root = gate.scratch / "grp-b/auto-man"
    install = gate.scratch / "install-b"
    gate.run("upgrade", ["python", "scripts/man-install.py", "install", "--profile", "full",
                         "--install-root", str(install)], root)
    backups = list(gate.scratch.glob(".install-b.man-backup-*"))
    if not backups:
        raise RuntimeError("managed previous-install backup missing")
    return f"upgrade verified; previous install preserved at {backups[-1].name}"


def unmanaged(gate: Gate) -> str:
    root = gate.scratch / "unmanaged"
    root.mkdir()
    marker = root / "user-data.txt"
    marker.write_text("keep me", encoding="utf-8")
    gate.run("unmanaged-reject", ["python", "scripts/man-install.py", "install", "--profile", "core",
                                  "--install-root", str(root), "--skip-build"],
             gate.scratch / "grp-b/auto-man", expect=(2,))
    if marker.read_text(encoding="utf-8") != "keep me":
        raise RuntimeError("unmanaged user file changed")
    return "unknown non-empty directory rejected; user bytes preserved"


def failure(gate: Gate, point: str) -> str:
    install = gate.scratch / "install-b"
    before = sha256(install / "install-manifest.json")
    gate.run(f"failure-{point}", ["python", "scripts/man-install.py", "install", "--profile", "full",
                                  "--install-root", str(install), "--skip-build", "--fail-point", point],
             gate.scratch / "grp-b/auto-man", expect=(2,))
    if sha256(install / "install-manifest.json") != before:
        raise RuntimeError(f"{point} changed the previous verified install")
    leftovers = list(gate.scratch.glob(".install-b.man-staging-*"))
    if leftovers:
        raise RuntimeError(f"owned staging left after {point}: {leftovers}")
    gate.run(f"failure-{point}-verify", ["python", "scripts/man-install.py", "verify",
                                         "--install-root", str(install), "--profile", "full"],
             gate.scratch / "grp-b/auto-man")
    return f"{point} failure is named and previous install remains intact"


def tamper(gate: Gate) -> str:
    install = gate.scratch / "install-b"
    target = install / "sdk/crates/auto-tool-protocol/src/version.rs"
    original = target.read_bytes()
    mutated = bytearray(original)
    mutated[0] ^= 1
    try:
        target.write_bytes(mutated)
        gate.run("tamper-verify", ["python", "scripts/man-install.py", "verify",
                                   "--install-root", str(install), "--profile", "full"],
                 gate.scratch / "grp-b/auto-man", expect=(2,))
    finally:
        target.write_bytes(original)
    gate.run("tamper-restore", ["python", "scripts/man-install.py", "verify",
                                "--install-root", str(install), "--profile", "full"],
             gate.scratch / "grp-b/auto-man")
    return "equal-length content tamper rejected; restored bytes verify green"


def missing_tool(gate: Gate) -> str:
    project = gate.scratch / "negative-ui-project"
    shutil.copytree(gate.scratch / "grp-b/auto-lang/examples/ui/001-helloworld", project)
    missing = gate.scratch / "missing-auto-ui-tool.exe"
    gate.run("missing-tool", [str(gate.scratch / "install-b" / exe("auto")), "gen"], project,
             {"AUTO_UI_TOOL_BIN": str(missing)}, expect=(1, 2))
    if (project / "gen").exists():
        raise RuntimeError("missing tool produced generation side effects")
    return "missing explicit tool rejected before generation side effects"


def owner_liveness(gate: Gate) -> str:
    import contextlib
    import importlib.util
    import io

    spec = importlib.util.spec_from_file_location("man_install_liveness", MAN / "scripts/man-install.py")
    installer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(installer)

    def staging(label: str, pid: int) -> tuple[Path, Path]:
        root = gate.scratch / f"owner-{label}"
        stage = root.with_name(f".{root.name}.man-staging-{label}")
        stage.mkdir()
        (stage / ".man-staging.marker").write_text(json.dumps({"schema": "man-staging/1", "pid": pid}))
        (stage / "user-input.txt").write_text("preserve unless owner is confirmed dead")
        return root, stage

    if os.name == "nt":
        root, protected = staging("protected", 4)
        if installer.process_alive(4) is False:
            raise RuntimeError("protected System process must not be classified as dead")
        installer.recover_stale_staging(root)
        if not protected.exists():
            raise RuntimeError("unqueryable protected owner staging was deleted")
        shutil.rmtree(protected)
    root, live = staging("live", os.getpid())
    installer.recover_stale_staging(root)
    if not live.exists():
        raise RuntimeError("live owner staging was deleted")
    shutil.rmtree(live)
    root, dead = staging("dead", 2147483647)
    installer.recover_stale_staging(root)
    if dead.exists():
        raise RuntimeError("confirmed-dead owner staging was not reclaimed")
    root, unknown = staging("unknown", 12345)
    original = installer.process_alive
    diagnostics = io.StringIO()
    try:
        installer.process_alive = lambda _pid: None
        with contextlib.redirect_stderr(diagnostics):
            installer.recover_stale_staging(root)
    finally:
        installer.process_alive = original
    if not unknown.exists() or "owner liveness unknown" not in diagnostics.getvalue():
        raise RuntimeError("unknown owner must be preserved and named")
    shutil.rmtree(unknown)
    return "protected/unknown/live staging preserved; confirmed-dead staging reclaimed"


def documented_restore(gate: Gate) -> str:
    root = gate.scratch / "install-b"
    source = gate.scratch / "grp-b/auto-man"
    upgraded = gate.run("documented-restore-upgrade",
                        ["python", "scripts/man-install.py", "install", "--profile", "full",
                         "--install-root", str(root), "--skip-build"], source)
    backup = Path(json.loads(upgraded.stdout)["backup_root"])
    failed = root.with_name(f"{root.name}.man-failed-{os.getpid()}")
    os.replace(root, failed)
    os.replace(backup, root)
    marker = root / ".man-backup.marker"
    if not marker.is_file():
        raise RuntimeError("real upgrade backup lacks its transaction marker")
    os.replace(marker, gate.scratch / "restored-backup-marker.txt")
    gate.run("documented-restore-verify", ["python", "scripts/man-install.py", "verify",
                                           "--install-root", str(root), "--profile", "full"], source)
    result = gate.run("documented-restore-command", [str(root / exe("auto")),
                                                      str(gate.scratch / "relocate-probe.at")],
                      gate.scratch)
    if "relocated" not in result.stdout:
        raise RuntimeError("restored backup failed to execute the previous language command")
    return "real backup follows documented marker preservation, verifies and runs"


def recovery(gate: Gate) -> str:
    gate.run("recovery-install", ["python", "scripts/man-install.py", "install", "--profile", "full",
                                  "--install-root", str(gate.scratch / "install-b"), "--skip-build"],
             gate.scratch / "grp-b/auto-man")
    detail = owner_liveness(gate)
    restored = documented_restore(gate)
    return f"post-failure managed install verifies green; {detail}; {restored}"


if __name__ == "__main__":
    sys.exit(main())
