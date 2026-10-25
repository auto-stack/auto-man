# AutoMan

AutoMan owns the `auto` command, package/build management, language-service adapters, and the I-TOOL client. The current delivery has two profiles:

- **Core**: Auto language and management commands without UI sources or `auto-ui-tool`.
- **Full**: Core plus the fixed AutoUI toolchain and UI assets.

The executable is **`auto`**, not `am`. The old single-crate `cargo install --path .` and `am` instructions are retained only as [legacy documentation](docs/legacy-am.md); they are not current installation commands.

## Install

See [Installation, versions, upgrade, and recovery](docs/installation.md) for fixed-source acquisition and complete instructions. From a matched source group, the managed entry points are:

```bash
# Core: only auto-man and auto-lang sources are required.
python scripts/man-install.py install --profile core --install-root /path/to/auto-core

# Full: matched auto-man, auto-lang, auto-ui, and auto-down sources are required.
python scripts/man-install.py install --profile full --install-root /path/to/auto-full

# Verify installed files and profile identity.
python scripts/man-install.py verify --profile core --install-root /path/to/auto-core
```

The installer builds in staging, verifies file hashes and source identity, then swaps the installation into place. An upgrade preserves the previous managed installation in a named sibling backup. Do not install into an unknown non-empty directory.

## Use

```bash
auto --version
auto script.at
auto new demo
auto build
auto run
```

Core rejects UI-only requests with a named profile/capability error. Full UI generation, build, and run use the real `auto-ui-tool` process; there is no static UI fallback.

## Develop

```bash
# Core has its own workspace and lock; it does not load full/UI manifests.
cargo build --locked --manifest-path core/Cargo.toml -p auto --no-default-features --features core

# Full development build (requires the matched UI source sibling).
cargo build --locked -p auto

# Scoped repository tests.
cargo t
cargo tc
cargo ti
```

The cross-repository CI entry is `auto-lang/scripts/cross-repo-tiers.py`. Before Man integration tests it builds `auto-ui-tools` in AutoUI and `te-child` in AutoLang, then passes their owner paths to the tests. Explicit `AUTO_UI_TOOL_BIN` and `AUTO_TE_CHILD_BIN`/`TE_CHILD_BIN` overrides remain authoritative.

## Source layout

- `crates/auto`: shared CLI source and full package/tests.
- `crates/auto-core` + `core/`: Core package, workspace, lock, and target; no UI path dependencies.
- `crates/auto-man-*`, `crates/auto-package-*`, `crates/auto-toolchain-core`: management services and package facilities.
- `crates/auto-tool-client`: tool discovery and protocol client.
- `scripts/man-install.py`: managed Core/full installation and verification.
- `scripts/man-distribution.py`: isolated clone, installation, relocation, and failure-recovery verification.

## License

This project is part of the AutoStack ecosystem. See [LICENSE](LICENSE).
