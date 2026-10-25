# Installation, versions, upgrade, and recovery

This is the current AutoMan delivery entry. The legacy `am` / `auto-man.exe` instructions are historical only.

## Fixed sources

Use matched Git commits; do not mix arbitrary repository HEADs.

- **Core**: clone only `auto-man` and `auto-lang`. No AutoUI or auto-down checkout is required.
- **Full**: clone `auto-man`, `auto-lang`, `auto-ui`, and `auto-down` as siblings.
- `scripts/man-distribution.py` records the fixed dependency revisions used by the reproducible distribution gate. The Man revision is the revision being installed.
- Every installed `install-manifest.json` records the actual repository commits under `sources`, the package/source projection under `source_closure`, and a SHA256 for every installed file.

A fixed source package must preserve `Cargo.lock` and, for Core, `core/Cargo.lock`. Cargo/Rust and Python 3 are required. Full development builds additionally require the matched AutoUI source and its documented build prerequisites; auto-down is a fixed Full-only dependency.

## Core installation

From the `auto-man` clone, with only the matched `auto-lang` sibling present:

```bash
python scripts/man-install.py install --profile core --install-root /path/to/auto-core
python scripts/man-install.py verify --profile core --install-root /path/to/auto-core
/path/to/auto-core/auto --version
```

On Windows, use `auto.exe`. Core uses `core/Cargo.toml` and `core/target`, so it neither loads the Full manifest nor reuses the Full target. The installed SDK can be checked independently:

```bash
cargo check --locked --manifest-path /path/to/auto-core/sdk/Cargo.toml --no-default-features --features core
```

Core contains only Man/lang source identities and licenses. UI commands fail by name rather than acquiring UI implicitly.

## Full installation

Place all four matched clones as siblings, then run from `auto-man`:

```bash
python scripts/man-install.py install --profile full --install-root /path/to/auto-full
python scripts/man-install.py verify --profile full --install-root /path/to/auto-full
```

Full builds `auto` in Man and `auto-ui-tool` in AutoUI, and installs both with the combined SDK and licenses. `auto gen`, `auto build`, and UI run consume the real installed tool. Missing or mismatched tools are errors, not a fallback to Core behavior.

## Upgrade and backup

Run the same install command with the desired new fixed sources and the existing managed install root. Preparation and verification happen in a unique staging directory. Only after verification is the previous installation moved to a sibling named like:

```text
.<install-name>.man-backup-<pid>-<timestamp>
```

The new staging directory then becomes the install root. A failed preparation, verification, or swap does not replace the previous verified installation. Unknown non-empty directories are rejected. Staging whose owner is alive—or whose liveness cannot be determined, including Windows access denial—is preserved and reported.

## Restore a previous installation

Use the backup created by the installer; do not delete or overwrite user files. In PowerShell, first move the current candidate aside, then move the backup back. The backup's owned `.man-backup.marker` is transaction metadata, not installed payload: preserve it outside the restored root before verification so strict extra-file checking remains intact.

```powershell
$root = "D:\apps\auto-full"
$backup = "D:\apps\.auto-full.man-backup-<pid>-<timestamp>"
$stamp = Get-Date -Format yyyyMMddHHmmss
Move-Item $root "$root.man-failed-$stamp"
Move-Item $backup $root
Move-Item "$root/.man-backup.marker" "$root.man-restored-backup-marker-$stamp"
python scripts/man-install.py verify --profile full --install-root $root
```

Use `--profile core` for a Core restoration. Verify before using the restored commands. Do not bypass verification or ignore other extra files. The manifest is the version and content source of truth; an executable's filename alone is not provenance.

## Relocation and troubleshooting

An installed tree can be moved as a unit and checked with `verify`. To rebuild its SDK, use the SDK manifest inside the new location, not the original source target. If `verify` reports missing, extra, or changed files, restore a known-good managed backup or reinstall from fixed sources.

`release.ps1` is retired: it formerly packaged the obsolete `auto-man.exe` and must not be used for current releases. Current reproducible delivery and recovery checks are driven by `scripts/man-distribution.py`; remote push/registry publication is not implied by local installation.
