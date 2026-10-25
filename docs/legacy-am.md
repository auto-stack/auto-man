# AutoMan

A modern build system and package manager for embedded C/Auto development. Similar to Cargo for Rust, AutoMan specializes in embedded systems with multi-platform build support (CMake, IAR, GHS, Ninja).

**Core Philosophy**: Auto is an intermediate language that transpiles to C, providing type safety and modern features while seamlessly integrating with existing C codebases.

## Features

- **Build Management**: Transpile Auto to C, compile C to binaries
- **Multi-Platform Build**: Support for CMake, IAR, GHS, and CCS build systems
- **Package Management**: Organize codebases as `pac` (packages)
- **Dependency Management**: Automatic git-based dependency download and version resolution
- **Device Resources**: Configure and download hardware resources (chip libraries, configurations)
- **Testing Framework**: Built-in testing system with automated test execution

## Installation

```bash
# Build from source
cargo build --release

# Install locally (installs `am` command)
cargo install --path .
```

## Quick Start

### Create a New Project

```bash
# Auto language application
am app hello

# Auto language library
am lib mymath

# C language application
am capp hello

# C language library
am clib mymath
```

### Build and Run

```bash
cd hello
am build    # Build the project (alias: am b)
am run      # Run the executable (alias: am r)
```

## Project Structure

A typical AutoMan project:

```
hello/
├── pac.at          # Project configuration file
├── hello.at        # Auto source file
└── deps/           # Downloaded dependencies
```

The `pac.at` file uses AutoConfig format (similar to JSON) to define project metadata, targets, and dependencies:

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

## Build Targets

AutoMan supports several types of build targets:

| Type | Description | Output |
|------|-------------|--------|
| `app` | Executable application | `.exe` / binary |
| `lib` | Static library | `.lib` / `.a` |
| `dep` | External dependency | Downloaded from git |
| `device` | Hardware resource package | Chip support files |
| `bag` | Code collection | Not compiled independently |
| `test` | Test unit | Test executable |

### Example Configuration

```rust
// Define a library
lib("mymath") {
    at: "libs/mymath"
}

// Define an application with dependencies
app("hello") {
    link("mymath") {}
    link("log") {}

    dir("src") {}
    dir("include") {}
}
```

## Dependency Management

AutoMan uses git-based dependency management. Add dependencies in `pac.at`:

```rust
name: "myapp"
version: "0.1.0"

// Dependency with version
dep("mymath", version: "0.1.5") {}

// Or use latest (master:HEAD)
dep("log") {}

app("myapp") {
    link("mymath") {}
    link("log") {}
}
```

### Dependency Commands

```bash
am deps        # List all available dependencies
am devices     # List all available device resources
am pull        # Update all dependencies and indexes
```

### Using Dependencies in Code

**Auto language:**
```rust
use mymath:add

fn main {
    print(add(1, 2))
}
```

**C language:**
```c
#include "mymath.h"

int main(void) {
    printf("%d\n", mymath_add(1, 2));
    return 0;
}
```

## Ports and Build Systems

AutoMan uses **Ports** to define compilation targets (platform + toolchain + builder). Default ports:

- **Windows**: `windows_ninja` (MSVC + Ninja)
- **Linux**: `linux_ninja` (GCC + Ninja)

### Supported Builders

| Builder | Description | Platform |
|---------|-------------|----------|
| CMake | CMake build system | Windows (VS), Linux (GCC) |
| IAR | IAR Embedded Workbench | ARM, RH850, etc. |
| GHS | Green Hills MULTI | PowerPC, ARM, etc. |
| Ninja | Ninja build system | Fast incremental builds |

### Port Management

```bash
am port       # Show and select current port
```

Custom ports can be configured in `~/.auto/auto-man/am.at`:

```rust
port("iar_arm") {
    platform: "windows"
    builder: "iar"
    tool: "iar"
    at: "build_iar"
}
```

## Compiler Configuration

Starting from v0.1.4, AutoMan supports configuring compilers directly in the project's `pac.at` file:

```rust
port("windows_msvc") {
    platform: "windows"
    builder: "ninja"
    at: "build"

    compiler {
        kind: "MSVC"
        location: Env

        executables {
            cc: "cl.exe"
            ar: "lib.exe"
            link: "link.exe"
            as: "ml64.exe"
        }

        flags {
            include: Prefix("/I")
            define: Prefix("/D")
            library: Postfix(".lib")
            library_path: Prefix("/LIBPATH:")
            output: Both("/Fo", ".obj")
            compile_only: Prefix("/c")
        }

        default_cflags: [
            "/O2",
            "/W3",
            "/MD"
        ]
    }
}
```

For detailed compiler configuration instructions, see [Port Compiler Configuration Guide](docs/port-compiler-config.md).

## The Build Process

Running `am build` executes these steps:

1. **Scan Phase**: Parse `pac.at`, download dependencies, discover source files
2. **Transpile Phase**: Convert `.at` files to `.h`/`.c` files using `auto-lang` transpiler
3. **Generate Build Files**: Generate CMakeLists.txt, .ewp (IAR), or other build configurations
4. **Compile**: Invoke the build system (CMake, IAR, GHS, Ninja)

Output: Build artifacts in `build/` directory (e.g., `build/Debug/hello.exe`)

## AutoConfig Syntax

AutoConfig is a configuration language that supports both static declarations and dynamic scripting:

### Static Structure

```rust
// Attributes
name: "myproject"
version: "0.1.0"
features: ["feature1", "feature2"]

// Nested objects
config: {
    optimization: "O2"
    warnings: true
}

// Nodes
app("main") {
    at: "apps/main"
    recurse: true

    dir("src") {
        dir("utils") {}
    }
}
```

### Dynamic Scripting

```rust
// Variables
var target_chip = "RH850"

// Conditional compilation
if port == target_chip {
    device("Renesas-RH850") {}
} else {
    device("Generic-ARM") {}
}
```

## Multiple Targets

A single package can contain multiple build targets:

```rust
name: "workspace"
version: "0.1.0"

app("main") {}
app("cli") {}
lib("utils") {}
lib("math") {}
test("utils_test") {}
```

When running `am run`, you'll be prompted to select which app to execute if multiple exist.

## Auto/C Mixed Projects

Auto and C targets can be mixed in the same project. The build process:

1. Scan all targets for `.at` files (Auto source)
2. Transpile all `.at` → `.h`/`.c`
3. Scan again for `.c`/`.h` files (C source)
4. Compile all C sources

Similar to how TypeScript compiles to JavaScript, Auto compiles to C.

## Testing

AutoMan includes a built-in testing framework (Auto language only):

```bash
am addtest mymath    # Generate test file for mymath
am test              # Run all tests
```

Generated test file (`test_mymath.at`):

```rust
fn test_add {
    assert_eq(5, add(2, 3))
}
```

## Advanced Commands

```bash
# Project Management
am clean       # Clean build artifacts
am open        # Open project in IDE (alias: am o)
am info        # Show package/target info (alias: am i)

# Configuration
am reset       # Reset automan config and index
am install     # Install custom am.at config file

# Updates
am upgrade     # Update AutoMan to latest version
```

## Directory Scanning

By default, AutoMan scans these directories for source files:

- Current directory
- `src/` - Source files
- `inc/` - Include files
- `include/` - Include files

Supported file types:
- Auto: `.at`
- C: `.c`, `.h`
- Assembly: `.s`, `.S`, `.850`

## Index System

AutoMan uses a centralized index system similar to crates.io:

- Packages identified by `vendor/pac` (e.g., `soutek/mymath`)
- Prevents naming conflicts between different vendors
- Downloads from git repositories

Configure index repositories in `~/.auto/auto-man/am.at`:

```rust
index: {
    default: "git@gitee.com:auto-stack/auto-index.git"
    custom: "git@github.com:myorg/auto-index.git"
}
```

## Development

### Building AutoMan

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Project Structure

- `src/automan.rs` - Core orchestration logic
- `src/builder.rs` - Builder trait and implementations
- `src/target.rs` - Target discovery and scanning
- `src/port.rs` - Port/platform/toolchain definitions
- `src/pac.rs` - Package configuration parsing
- `src/index.rs` - Package/device index management
- `src/git.rs` - Git operations for dependencies
- `src/asset.rs` - Embedded template system
- `src/scanner.rs` - Unified directory scanning logic
- `src/file_types.rs` - File type registry
- `src/node_ext.rs` - Node property extensions

## License

This project is part of the AutoStack ecosystem.

## Related Projects

- [auto-lang](../auto-lang/) - The Auto programming language
- [auto-val](../auto-lang/) - Value types for Auto
- [auto-gen](../auto-lang/) - Code generation utilities
