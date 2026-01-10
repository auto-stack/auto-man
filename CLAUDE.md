# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

AutoMan (Auto Manager) is a build system and package manager for embedded C/Auto language development. Similar to Cargo for Rust, but specialized for embedded systems with multi-platform build support (CMake, IAR, GHS, Ninja).

**Key Concept**: Auto is an intermediate language that transpiles to C, providing type safety and modern features while seamlessly integrating with existing C codebases.

## Development Commands

### Building AutoMan itself
```bash
# Build the project
cargo build

# Build with release optimizations
cargo build --release

# Run tests
cargo test

# Install locally (installs `am` command)
cargo install --path .
```

### Common workflow for testing changes
```bash
# Build and install locally
cargo install --path .

# Test the installed `am` command
am app hello
cd hello
am build
```

## Architecture

### Core Abstractions

**Automan** (`src/automan.rs`) - Main orchestrator that manages projects, scanning, building, and dependencies

**Builder System** (`src/builder.rs`) - Trait-based architecture supporting:
- `CMakeBuilder` - Windows (Visual Studio) and Linux (GCC)
- `IARBuilder` - IAR Embedded Workbench
- `GHSBuilder` - Green Hills MULTI
- `NinjaBuilder` - Ninja build system

**Target** (`src/target.rs`) - Represents buildable units:
- `App` - Executable applications
- `Lib` - Static libraries
- `Dep` - External dependencies (downloaded from git)
- `Device` - Hardware resource packages
- `Bag` - Code collections (not independently compiled)
- `Test` - Test units

**Port** (`src/port.rs`) - Defines compilation targets (platform + toolchain + builder):
- Windows default: `windows_ninja` (MSVC + Ninja)
- Linux default: `linux_ninja` (GCC + Ninja)
- Custom ports configured in user's `~/.auto/auto-man/am.at`

**Pac** (`src/pac.rs`) - Package representation parsed from `pac.at` configuration files

### Build Process Flow

1. **Scan Phase** (`am scan`):
   - Parse `pac.at` for AutoConfig syntax
   - Download dependencies (git-based) to `deps/`
   - Discover source files (Auto `.at` and C `.c`/`.h`)

2. **Transpile Phase**:
   - Convert Auto `.at` files to C `.h`/`.c` files using `auto-lang` crate
   - Auto files use `use` statements → C `#include` with namespacing

3. **Build Generation**:
   - Generate build configuration (CMakeLists.txt, .ewp for IAR, etc.)
   - Invoke appropriate build system based on Port

4. **Compile**:
   - Build artifacts to `build/` directory
   - Debug builds: `build/Debug/` on Windows

### Asset System

Templates and build configurations are embedded at compile time using `rust-embed`:

**Project Templates** (`assets/templates/`):
- `app` - Auto language application
- `lib` - Auto language library  
- `capp` - C application
- `clib` - C library
- `libdemo` - Demo library with examples

**Builder Templates** (`assets/builders/`):
- `iar/` - IAR project files (.ewp, .eww, .ewd, .ewt)
- `ghs/` - GHS project files (.gpj)

### Configuration System

Two-level configuration:

1. **Project Config** - `pac.at` in project root:
   - Defines targets (app, lib, dep, device)
   - Specifies dependencies and links
   - AutoConfig format (JSON-like, supports scripting)

2. **User Config** - `~/.auto/auto-man/am.at`:
   - Port definitions (platform, builder, toolchain)
   - Index repositories for packages/devices
   - Git repository URLs

Example `pac.at`:
```rust
name: "hello"
version: "0.0.1"

dep("log", version: "0.0.2") {
    at: "source/lib/log"
}

app("hello") {
    link("log") {}
    dir("motor") {
        dir("bldc") {}
    }
}
```

### Index System

**IndexStore** (`src/index.rs`) - Manages package and device registries:
- Downloads index from git repositories
- Caches locally in `~/.auto/auto-man/index/`
- Supports vendor/pac naming to avoid conflicts

### Dependency Management

Git-based dependency resolution:
- Format: `vendor/pac:version` (e.g., `soutek/mymath:0.1.5`)
- Downloaded to `deps/` directory
- Must be explicitly linked to targets via `link()` nodes
- Recursive dependency support

## Key Source Files

- `src/main.rs` - CLI entry point with clap command definitions
- `src/automan.rs` - Core project orchestration logic
- `src/builder.rs` - Builder trait and implementations
- `src/target.rs` - Target discovery and source file scanning
- `src/port.rs` - Port/platform/toolchain definitions
- `src/pac.rs` - Package configuration parsing
- `src/index.rs` - Package/device index management
- `src/git.rs` - Git operations for dependencies
- `src/asset.rs` - Embedded template system
- `src/group.rs` - Target grouping and organization
- `src/dir.rs` - Directory scanning logic
- `src/util.rs` - Utility functions
- `src/up.rs` - Self-update functionality
- `src/pull.rs` - Dependency update operations

## Important Notes

- **Auto Language Integration**: This crate depends on the `auto-lang` workspace crates (`auto-val`, `auto-atom`, `auto-gen`) which are sibling projects in `../auto-lang/`
- **Logging**: Uses `simplelog` with Info level by default
- **Error Handling**: All public functions return `AutoResult<T>` (alias for `Result<T, AutoError>`)
- **Path Handling**: Uses `auto_val::AutoPath` and `auto_val::AutoStr` for cross-platform path operations
- **Interactive Prompts**: Uses `dialoguer` for user selection when multiple options exist
- **Command Aliases**: `b` for build, `r` for run, `o` for open, `i` for info
