# AutoMan Integration Tests

This directory contains folder-based integration tests for AutoMan.

## Test Structure

Each test case is a complete auto-man project:

```
NNN_description/
├── pac.at                          # Project configuration (REQUIRED)
├── source/                         # Source files
│   └── main.c
└── expected/                       # Expected outputs (organized by builder)
    ├── cmake/
    │   └── CMakeLists.txt
    ├── iar/
    │   ├── project.eww
    │   ├── project.ewp
    │   ├── project.ewt
    │   └── project.ewd
    ├── ghs/
    │   └── default.gpj
    └── ninja/
        └── build.ninja
```

## Test Case Naming

- Format: `NNN_brief_description`
- NNN: Sequential number (001, 002, ...)
- Description: snake_case, brief but descriptive

Examples:
- `001_simple_app` - Minimal application
- `002_with_lib` - Application with library
- `003_multi_target` - Multiple targets
- `004_with_deps` - External dependencies

## Running Tests

### Run all integration tests
```bash
cargo test --test integration_test
```

### Run specific test case
```bash
cargo test --test integration_test -- 001_simple_app
```

## Adding New Test Cases

1. Create directory: `tests/integration/NNN_description/`
2. Add `pac.at` configuration
3. Add source files in appropriate directories
4. Create `expected/` directory with builder outputs
5. Run tests to verify

## Current Status

### Phase 1: Foundation ✅
- [x] Test directory structure created
- [x] Test executor implemented
- [x] First test case (001_simple_app) added
- [x] Basic test discovery and validation working

### Next Steps
- [ ] Implement in-memory output generation
- [ ] Add actual build configuration comparison
- [ ] Support for multiple builders (IAR, GHS, Ninja)
- [ ] Generate .wrong files on failure
- [ ] Add more test cases

## Test Cases

| # | Name | Description | Builders | Status |
|---|------|-------------|----------|--------|
| 001 | simple_app | Minimal application | cmake | ✅ Pass |

## Implementation Plan

See `docs/plans/003-automated-testing-framework.plan.md` for the complete implementation plan.
