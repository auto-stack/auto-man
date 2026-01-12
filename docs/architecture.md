# AutoMan Architecture

This document provides a comprehensive overview of the AutoMan architecture, including its core components, data flow, and design decisions.

## Table of Contents

1. [Overview](#overview)
2. [Core Concepts](#core-concepts)
3. [Component Architecture](#component-architecture)
4. [Build Pipeline](#build-pipeline)
5. [Dependency Management](#dependency-management)
6. [Builder System](#builder-system)
7. [Configuration System](#configuration-system)
8. [Data Structures](#data-structures)
9. [Error Handling](#error-handling)
10. [Testing Strategy](#testing-strategy)
11. [Extension Points](#extension-points)

---

## Overview

AutoMan is a build system and package manager for embedded C/Auto language development. It provides:

- **Multi-platform build support** (Windows, Linux, embedded toolchains)
- **Multiple build system backends** (CMake, IAR, GHS, Ninja)
- **Git-based dependency management** with semantic versioning
- **Auto language transpilation** to C
- **Incremental compilation** with build caching
- **Reproducible builds** with dependency lock files

### Design Philosophy

1. **Convention over Configuration** - Sensible defaults with override capability
2. **Composability** - Small, focused components that work together
3. **Type Safety** - Leverage Rust's type system for build configurations
4. **Performance** - Incremental builds and parallel operations
5. **User Experience** - Clear error messages, progress indicators, helpful CLI

---

## Core Concepts

### Auto Language

Auto is an intermediate language that transpiles to C, providing:
- Type safety and modern language features
- Seamless C interoperability
- Namespacing via `use` statements
- `.at` source files that compile to `.h` and `.c` files

### Ports

A **Port** defines a compilation target combining:
- **Platform** (Windows, Linux, Embedded)
- **Toolchain** (MSVC, GCC, ARM CC, etc.)
- **Builder** (CMake, IAR, GHS, Ninja)

Example ports:
- `windows_ninja` - Windows + MSVC + Ninja
- `linux_ninja` - Linux + GCC + Ninja
- `iar_arm` - Embedded + ARM CC + IAR

### Targets

A **Target** is a buildable unit:
- **App** - Executable application
- **Lib** - Static library
- **Dep** - External dependency
- **Device** - Hardware resource package
- **Bag** - Code collection (not independently compiled)
- **Test** - Test unit

### Projects

An AutoMan project is defined by a `pac.at` file in the project root. It contains:
- Project metadata (name, version)
- Dependency declarations
- Target definitions with sources and links

---

## Component Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI Layer                           │
│                      (main.rs + clap)                       │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                   AutoMan Orchestrator                      │
│                     (automan.rs)                            │
│  ┌──────────┬──────────┬──────────┬──────────┬──────────┐  │
│  │  Scan    │Transpile │ Generate │  Build   │   Clean  │  │
│  └──────────┴──────────┴──────────┴──────────┴──────────┘  │
└────────────────────────┬────────────────────────────────────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
         ▼               ▼               ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│   Pac Parser │ │    Target    │ │  Index Store │
│   (pac.rs)   │ │  (target.rs) │ │  (index.rs)  │
└──────────────┘ └──────┬───────┘ └──────────────┘
                       │
        ┌──────────────┼──────────────┐
        │              │              │
        ▼              ▼              ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│   Version    │ │  Lock File   │ │ Build Cache  │
│(version.rs)  │ │  (lock.rs)   │ │  (cache.rs)  │
└──────────────┘ └──────────────┘ └──────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   Builder Trait      │
              │   (builder.rs)       │
              ├──────────┬───────────┤
              ▼          ▼           ▼
          ┌──────┐  ┌──────┐   ┌──────┐
          │CMake │  │ IAR  │   │ GHS  │
          └──────┘  └──────┘   └──────┘
```

---

## Build Pipeline

### 1. Scan Phase

**Entry Point:** `Automan::scan()`

**Purpose:** Discover all targets and dependencies

**Steps:**
1. Parse `pac.at` configuration file
2. Query index stores for dependency metadata
3. Download dependencies from git repositories
4. Create `Target` instances for each target
5. Scan source files (Auto `.at` and C `.c`/`.h`)
6. Resolve target links and dependencies

**Output:** `Vec<Target>` - All targets with their sources

**Key Functions:**
- `Pac::parse()` - Parse pac.at configuration
- `Pac::resolve_deps()` - Download and resolve dependencies
- `Target::from()` - Create target from configuration node
- `Target::scan_sources()` - Discover source files

### 2. Transpile Phase

**Entry Point:** `Target::transpile_auto()`

**Purpose:** Convert Auto source files to C

**Steps:**
1. For each `.at` file in target.autos:
   - Invoke auto-lang transpiler
   - Generate `.h` and `.c` files
   - Update target.srcs with generated files
2. Display progress bar for user feedback

**Output:** C files in build directory

**Key Functions:**
- `auto_lang::trans::c::transpile_c()` - Transpile Auto to C
- `Target::transpile_auto()` - Transpile all Auto sources

### 3. Generate Phase

**Entry Point:** `Builder::generate()`

**Purpose:** Generate build configuration files

**Steps:**
1. Create builder-specific project files:
   - CMake: `CMakeLists.txt`
   - IAR: `.ewp`, `.eww`, `.ewd`, `.ewt`
   - GHS: `.gpj`
   - Ninja: `build.ninja`
2. Configure includes, defines, and compiler flags
3. Set up target dependencies

**Output:** Build system configuration files

**Key Functions:**
- `CMakeBuilder::generate()`
- `IARBuilder::generate()`
- `GHSBuilder::generate()`
- `NinjaBuilder::generate()`

### 4. Build Phase

**Entry Point:** `Builder::build()`

**Purpose:** Compile the project

**Steps:**
1. Invoke build system:
   - CMake: `cmake --build .`
   - IAR: `IarBuild.exe`
   - GHS: `gmake`
   - Ninja: `ninja`
2. Capture compiler output
3. Report build success/failure

**Output:** Compiled binaries in `build/` directory

**Key Functions:**
- `Builder::build()` - Invoke build system
- `Port::get_builder()` - Create appropriate builder

### 5. Cache Integration

**Purpose:** Enable incremental builds

**Mechanism:**
1. Load `.am/cache.json` at build start
2. Check file modification times
3. Skip transpilation of unchanged Auto files
4. Update cache after successful operations
5. Save cache to disk

**Key Functions:**
- `BuildCache::is_dirty()` - Check if file changed
- `BuildCache::mark_processed()` - Update cache entry
- `BuildCache::needs_transpilation()` - Check if transpile needed

---

## Dependency Management

### Declaration Format

Dependencies are declared in `pac.at`:

```rust
dep("log", version: "0.0.2") {
    at: "source/lib/log"
}

app("myapp") {
    link("log") {}
}
```

### Resolution Process

1. **Parse dependency declaration** - Extract name, version, location
2. **Query index store** - Get repository URL and metadata
3. **Download from git** - Clone to `deps/<vendor>/<pac>/`
4. **Check version** - Validate against semantic versioning rules
5. **Resolve recursively** - Process nested dependencies
6. **Update lock file** - Record exact versions and commit SHAs

### Semantic Versioning

Supported version requirements:
- **Exact:** `"1.2.3"` - Must be exactly 1.2.3
- **Caret:** `"^1.2.3"` - Compatible (>=1.2.3 <2.0.0)
- **Tilde:** `"~1.2.3"` - Approximate (>=1.2.3 <1.3.0)
- **Wildcard:** `"1.2.*"` - Any 1.2.x version
- **Comparison:** `">=1.2.3"`, `"<2.0.0"`, etc.

**Implementation:** [version.rs](../crates/auto-man/src/version.rs)

### Lock File

The `pac.lock` file ensures reproducible builds:

```toml
automan_version = "v0.1.2"

[[dependencies]]
name = "log"
version = "0.0.2"
url = "https://github.com/soutek/log"
commit = "abc123def456..."
path = "deps/soutek/log"
```

**Features:**
- Records exact versions used
- Captures git commit SHAs
- Supports JSON and TOML formats
- Generated automatically, committed to VCS

**Implementation:** [lock.rs](../crates/auto-man/src/lock.rs)

---

## Builder System

### Builder Trait

All builders implement the `Builder` trait:

```rust
pub trait Builder {
    fn generate(&mut self, targets: &[Target]) -> AutoResult<()>;
    fn build(&self) -> AutoResult<()>;
    fn clean(&self) -> AutoResult<()>;
}
```

### Builder Implementations

#### CMakeBuilder

**Use Cases:**
- Windows with Visual Studio
- Linux with GCC
- Cross-platform projects

**Generated Files:**
- `CMakeLists.txt` - Main build configuration

**Features:**
- Supports multiple configurations (Debug, Release)
- Handles include directories and defines
- Manages target dependencies

#### IARBuilder

**Use Cases:**
- ARM embedded development
- STM32, NXP, Kinetis targets

**Generated Files:**
- `.ewp` - Project file
- `.eww` - Workspace file
- `.ewd` - Debug configuration
- `.ewt` - Tool configuration

**Features:**
- Embedded templates for different device families
- Supports ARM, AVR, MSP430 toolchains
- Configures linker scripts and memory maps

#### GHSBuilder

**Use Cases:**
- Safety-critical embedded systems
- Automotive, aerospace applications

**Generated Files:**
- `.gpj` - Green Hills project file

**Features:**
- Supports INTEGRITY RTOS
- Optimizations for safety certification
- Multi-core support

#### NinjaBuilder

**Use Cases:**
- Fast incremental builds
- Large projects
- CI/CD pipelines

**Generated Files:**
- `build.ninja` - Ninja build configuration

**Features:**
- Minimal overhead
- Parallel compilation
- Dependency tracking

### Port Configuration

Ports are defined in user config (`~/.auto/auto-man/am.at`):

```rust
port("windows_ninja") {
    platform: "windows"
    builder: "ninja"
    toolchain: "msvc"
}
```

**Port Resolution:**
1. Check command-line `--port` flag
2. Check project `.am/port` file
3. Prompt user if multiple ports available
4. Use default for platform

---

## Configuration System

### Project Configuration

**File:** `pac.at` (project root)

**Structure:**
```rust
name: "myproject"
version: "0.1.0"

// Dependencies
dep("log", version: "^0.1.0") {
    at: "lib/log"
}

// Targets
app("myapp") {
    link("log") {}
    dir("src") {}
    dir("tests") {}
}
```

**Parsing:**
- Uses auto-lang's `AutoConfig` parser
- JSON-like syntax with Rust expressions
- Supports scripting and conditionals

### User Configuration

**File:** `~/.auto/auto-man/am.at`

**Contents:**
- Port definitions
- Index repositories
- Default settings
- Toolchain paths

### Cache Directory

**Location:** `.am/` (project root)

**Contents:**
- `cache.json` - Build cache (modified files)
- `port` - Selected port
- Transpiler outputs

---

## Data Structures

### Target

```rust
pub struct Target {
    pub name: AutoStr,
    pub kind: TargetKind,
    pub version: AutoStr,
    pub from: AutoPath,
    pub at: AutoPath,
    pub autos: Vec<AutoPath>,
    pub srcs: HashSet<AutoStr>,
    pub dirs: Vec<Dir>,
    pub links: Vec<Link>,
}

pub enum TargetKind {
    App,
    Lib,
    Dep,
    Device,
    Bag,
    Test,
}
```

**Purpose:** Represents a buildable unit

**Key Fields:**
- `autos` - Auto source files (`.at`)
- `srcs` - All source files (C + generated)
- `dirs` - Directory structure for includes
- `links` - Dependencies on other targets

### Pac

```rust
pub struct Pac {
    pub name: AutoStr,
    pub version: AutoStr,
    pub config: AutoConfig,
    pub deps: Vec<PacInfo>,
    pub targets: Vec<PacInfo>,
}

pub struct PacInfo {
    pub name: AutoStr,
    pub version: AutoStr,
    pub repo: AutoStr,
    pub kind: TargetKind,
    pub at: AutoStr,
}
```

**Purpose:** Parsed project configuration

### IndexStore

```rust
pub struct IndexStore {
    pacs: HashMap<String, PacEntry>,
    devices: HashMap<String, DeviceEntry>,
}

pub struct PacEntry {
    pub name: String,
    pub vendor: String,
    pub repo: String,
    pub versions: Vec<String>,
}
```

**Purpose:** Package registry cache

**Sources:**
- Git repositories (index.pac)
- Local directories
- Remote URLs

---

## Error Handling

### Error Types

AutoMan uses domain-specific error types for better error messages:

```rust
pub enum AutoManError {
    ConfigError { path: AutoPath, message: String },
    DependencyError { name: AutoStr, version: AutoStr, reason: String },
    BuildError { target: AutoStr, stage: BuildStage, message: String },
    GitError { operation: String, repo: String, details: String },
    TranspileError { file: AutoPath, line: usize, message: String },
}

pub enum BuildStage {
    Scan,
    Transpile,
    Generate,
    Compile,
}
```

**Benefits:**
- Contextual error messages
- Structured error data
- Easier debugging
- Better user experience

### Error Propagation

All public functions return `AutoResult<T>`:

```rust
pub type AutoResult<T> = Result<T, AutoError>;
```

**Convention:**
- Use `?` operator for error propagation
- Add context with `.map_err()` when converting errors
- Provide actionable error messages

---

## Testing Strategy

### Unit Tests

**Location:** `src/*.rs` (in module files)

**Coverage:**
- Version parsing and comparison
- Lock file serialization
- Cache operations
- Target creation
- Dependency resolution

**Example:**
```rust
#[test]
fn test_version_parse() {
    let v = Version::from_str("1.2.3").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
}
```

### Integration Tests

**Location:** `tests/` directory

**Coverage:**
- Full build workflow
- Dependency download
- Multi-target projects
- Transpilation pipeline

**Example:**
```rust
#[test]
fn test_full_build() {
    let project = TestProject::new();
    project.add_target("app", "myapp");
    project.build().unwrap();
    assert!(project.output_exists("myapp.elf"));
}
```

### Test Utilities

**File:** `tests/common.rs`

**Provides:**
- `TestProject` - Temporary project creation
- `TestTemplate` - Template configurations
- `MockGitServer` - Git server simulation
- Temporary directory management

---

## Extension Points

### Adding a New Builder

1. Implement `Builder` trait:

```rust
pub struct MyBuilder {
    path: AutoPath,
}

impl Builder for MyBuilder {
    fn generate(&mut self, targets: &[Target]) -> AutoResult<()> {
        // Generate project files
    }

    fn build(&self) -> AutoResult<()> {
        // Invoke compiler
    }

    fn clean(&self) -> AutoResult<()> {
        // Clean build artifacts
    }
}
```

2. Register in `Port` configuration
3. Add to `BuilderKind` enum
4. Update `Port::get_builder()`

### Adding a New Target Kind

1. Add to `TargetKind` enum:

```rust
pub enum TargetKind {
    App,
    Lib,
    // ... existing kinds ...
    MyNewKind,
}
```

2. Update `TargetKind::from_str()`
3. Implement kind-specific logic in `Target`
4. Add builder support if needed

### Adding Index Sources

1. Implement index retrieval:

```rust
pub fn load_index_from_url(url: &str) -> AutoResult<IndexStore> {
    // Download and parse index
}
```

2. Add to user configuration
3. Support in `IndexStore::load()`

---

## Performance Considerations

### Incremental Builds

**Strategy:**
- Track file modification times
- Skip unchanged files in transpilation
- Use builder's own dependency tracking
- Cache build metadata

**Implementation:** [cache.rs](../crates/auto-man/src/cache.rs)

### Parallel Operations

**Current:** Sequential processing

**Future:** Rayon integration for:
- Parallel source file scanning
- Parallel dependency downloads
- Parallel transpilation

### Caching Strategy

**Build Cache:**
- Location: `.am/cache.json`
- Stores: File modification times, sizes
- Updated: After successful operations
- Checked: Before transpilation

**Index Cache:**
- Location: `~/.auto/auto-man/index/`
- Stores: Package metadata
- Updated: Via `am pull` command
- Checked: During dependency resolution

---

## Security Considerations

### Git Operations

- Clone with `--depth 1` for shallow clones
- Verify commit SHAs from lock file
- Support SSH and HTTPS protocols

### Dependency Validation

- Check version constraints
- Verify git commit matches lock file
- Validate package names and vendors

### Path Handling

- Use `AutoPath` for cross-platform paths
- Sanitize user input
- Validate file locations

---

## Future Enhancements

### Planned Features

1. **Configuration Manager** - Centralized config handling
2. **Error Recovery** - Retry logic for network operations
3. **Parallel Builds** - Rayon integration
4. **Template Migration** - Move to CodeGenerator API
5. **Workspace Support** - Multi-package projects

### Architecture Improvements

1. **Plugin System** - Custom builders and commands
2. **Build Graph** - Dependency visualization
3. **Remote Cache** - Shared build artifacts
4. **Distributed Builds** - CI/CD optimization

---

## References

- **Main Code:** [../crates/auto-man/src/](../crates/auto-man/src/)
- **Tests:** [../tests/](../tests/)
- **Templates:** [../crates/auto-man/assets/](../crates/auto-man/assets/)
- **Plan:** [001-code-quality-improvements.plan.md](001-code-quality-improvements.plan.md)

---

**Last Updated:** January 13, 2026
**AutoMan Version:** v0.1.2
