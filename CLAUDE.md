# CLAUDE.md

Guidance for development in the current AutoMan workspace.

## Current architecture

AutoMan owns the `auto` CLI, management runtime/core, package lock/resolver, toolchain and asset facilities, and the I-TOOL client. AutoLang owns the language SDK, protocol/project models, and owned execution helpers. AutoUI owns UI compiler/runtime/tools. The executable is `auto`; `am` and the old single-crate layout are legacy only.

The CLI source is shared without duplication:

- `crates/auto` is the Full package and integration-test source.
- `crates/auto-core` plus `core/Cargo.toml` is the independent Core workspace. It reuses `crates/auto/src/main.rs`, `build.rs`, and selected tests through Cargo target paths.
- Core has no UI path dependency and uses `core/Cargo.lock` / `core/target`.
- The root workspace and root lock remain the Full/development workspace.

## Build and install

```bash
# Core: only matched Man/lang sources are required.
cargo build --locked --manifest-path core/Cargo.toml -p auto --no-default-features --features core

# Full: matched UI/down source siblings are required.
cargo build --locked -p auto
cargo build --locked -p auto-ui-tools --manifest-path ../auto-ui/Cargo.toml

# Managed installation; do not use cargo install --path . at the virtual root.
python scripts/man-install.py install --profile core --install-root /path/to/auto-core
python scripts/man-install.py install --profile full --install-root /path/to/auto-full
```

Read `docs/installation.md` for fixed sources, upgrade backups, restoration, and relocation. `release.ps1` is retired and intentionally returns an error. Historical `am` material is under `docs/legacy-am.md` and is not a current command reference.

## Tests and CI

```bash
cargo t    # workspace libraries and auto bin
cargo tc   # management Core libraries
cargo ti   # Man integration targets; requires owner tools
```

For a single real generation test, first build `auto-ui-tools` in AutoUI. The test discovers the group sibling by default; `AUTO_UI_TOOL_BIN` is an explicit authoritative override. Lifetime tests similarly use `te-child` from AutoLang and honor `AUTO_TE_CHILD_BIN`; the independent tool-client fixture uses `TE_CHILD_BIN`.

The cross-repository local CI entry is `auto-lang/scripts/cross-repo-tiers.py`. Its `man-ti` bootstrap builds UI and language helpers in their owner repositories and injects the resolved roots/tool paths. Do not reintroduce Man-target lookup or fallback to an arbitrary main checkout.

## Change discipline

Keep Core/free of UI and compatibility-package dependencies. Preserve the staged install transaction, named backups, unknown-directory protection, and the rule that only a confirmed-dead staging owner permits cleanup. Update `docs/installation.md` and both READMEs when commands, profiles, version sources, or recovery behavior change.
