# AutoMan Typed Configuration

This directory contains typed Auto language definitions for AutoMan project configurations.

## Purpose

These type definitions provide:
- **Type Safety**: Catch configuration errors at parse time
- **IDE Support**: Enable autocomplete and inline documentation via LSP
- **Documentation**: Inline comments explain each property
- **Validation**: Prevent typos and invalid property names

## Usage

Add this to your `pac.at` file:

```auto
use automan

app("myapp") {
    dir("src") {}
    link("mylib") {}
}
```

## Type Definitions

### Core Types (`types.at`)
- `Version` - Semantic versioning
- `Path` - File system paths
- `Url` - Git repository URLs
- `DepKind` - Dependency origin (index/git/local)
- `BuilderType` - Build system (cmake/iar/ghs/ninja)
- `Platform` - Target platform (windows/linux/macos)

### Target Types (`target.at`)
- `App` - Executable applications
- `Lib` - Static libraries
- `Bag` - Code collections (not independently compiled)
- `Test` - Test units

All inherit from `Target` base type with common properties.

### Dependency Types (`dependency.at`)
- `Dep` - External library dependencies
- `Device` - Hardware support packages

Both inherit from `Dependency` base type.

### Supporting Types
- `Port` - Build platform configuration (`port.at`)
- `Dir` - Directory scanning configuration (`directory.at`)
- `Link` - Target dependency links (`link.at`)
- `Tool` - Compiler/toolchain configuration (`tool.at`)

## Module Structure

```
automan/
├── mod.at          # Module exports
├── types.at        # Common types
├── target.at       # App, Lib, Bag, Test
├── dependency.at   # Dep, Device
├── port.at         # Port configuration
├── directory.at    # Dir type
├── link.at         # Link type
├── tool.at         # Tool type
└── README.md       # This file
```

## Migration from Untyped

### Before (Untyped)
```auto
name: "myproject"
version: "1.0.0"

app("myapp") {
    srcc: ["main.c"]  // Typo! Not caught until runtime
    dir: "src"        // Wrong type!
}
```

### After (Typed)
```auto
use automan

app("myapp") {
    srcs: ["main.c"]  // IDE validates this property exists
    dir("src") {}     // IDE knows correct syntax
}
```

## Examples

### Simple Application
```auto
use automan

app("myapp") {
    dir("src") {
        skips: ["tests"]
    }
    link("mylib") {}
}
```

### Library with Exported Headers
```auto
use automan

lib("mylib") {
    export_dir: "api"
    export_all: true
    dir("src") {}
}
```

### Git Dependency
```auto
use automan

dep("logger", "^1.0.0") {
    at: "deps/logger"
    from: "https://github.com/vendor/logger.git"
}

app("myapp") {
    link("logger") {}
}
```

### Custom Port
```auto
use automan

port("my_custom_port") {
    platform: "linux"
    builder: "ninja"
    tool {
        kind: "gcc"
        options: ["-std=c11", "-Wall"]
    }
    cflags: ["-O2"]
}
```

## IDE Support

When the Auto language LSP is implemented:
- **Autocomplete**: Ctrl+Space shows available properties
- **Hover**: Mouse over shows documentation
- **Validation**: Errors highlighted in real-time
- **Go to Definition**: F12 jumps to type definition

## Backward Compatibility

**Typed configurations are 100% backward compatible:**
- Old untyped configs still work
- You can mix typed and untyped nodes
- Migration is gradual and optional
- No breaking changes

## Contributing

When adding new properties to AutoMan:
1. Update the Rust struct (e.g., `Target` in `target.rs`)
2. Update the corresponding type definition in this folder
3. Add documentation comments
4. Update examples

## Related Documentation

- [Architecture Documentation](../docs/architecture.md)
- [Typed Config Plan](../docs/plans/002-typed-auto-config-types.md)
- [Configuration Guide](../docs/typed-config-guide.md) (TODO)

## Version

Compatible with AutoMan v0.1.2+
