# Auto-Man Documentation

## Introduction

Auto-Man is a comprehensive package management and build system tool designed for embedded systems development. It streamlines the development workflow by providing project templates, dependency management, device support, and automated build processes. Auto-Man enables developers to easily transpile Auto language code to C, create projects with different templates, and manage dependencies efficiently.

## Key Features

### Project Templates

Auto-Man provides several project templates to quickly start new projects:

- **App Templates**: Create Auto language application projects
- **Library Templates**: Create Auto language library projects
- **C App Templates**: Create C application projects
- **C Library Templates**: Create C library projects

### Dependency Management

Auto-Man simplifies dependency handling with:

- Automatic dependency scanning and resolution
- Recursive dependency downloads
- Centralized dependency repository management
- Dependency updates and version tracking

### Auto-to-C Transpiling

The tool features a sophisticated transpiling system that:

- Converts Auto language code to C
- Generates appropriate header and source files
- Preserves code functionality while optimizing for the target platform

### Device Support

Auto-Man includes a comprehensive device management system:

- Supports various embedded devices
- Manages device-specific configurations
- Provides easy access to device repositories
- Lists available devices and their specifications

### Project Types Support

Auto-Man enables development for different project types:

- **Standard Applications**: Regular embedded applications
- **FreeRTOS Projects**: Projects using the FreeRTOS real-time operating system
- **Mixed Projects**: Projects combining different languages or frameworks

### Build System Integration

The tool integrates with build systems to:

- Generate CMake configuration files
- Handle platform-specific build settings
- Manage build variants (Debug/Release)
- Clean build artifacts

## Usage Examples

### Creating New Projects

Create a new Auto language application:
```bash
automan app <project-name>
```

Create a new C application:
```bash
automan capp <project-name>
```

Create a new Auto language library:
```bash
automan lib <project-name>
```

Create a new C library:
```bash
automan clib <project-name>
```

### Managing Dependencies

List all available dependencies:
```bash
automan deps
```

Update project dependencies:
```bash
automan update
```

Scan for dependencies in the current project:
```bash
automan scan [--port <port-name>]
```

### Device Management

List all available devices:
```bash
automan devices
```

### Building and Running

Build the current project:
```bash
automan build [--dir <directory>] [--port <port-name>]
```

Run the built application:
```bash
automan run
```

Clean the project (remove build artifacts):
```bash
automan clean [--dir <directory>]
```

## Project Structure

A typical Auto-Man project contains:

- `pac.at`: Project configuration file
- Source files (`.at` for Auto language, `.c`/`.h` for C)
- `deps/`: Directory for downloaded dependencies
- `devices/`: Directory for device configurations
- `build/`: Build output directory

## Configuration

Project configuration is managed through the `pac.at` file, which defines:

- Project name and metadata
- Dependencies
- Target platforms
- Build configurations
- Port configurations

## Advanced Usage

### FreeRTOS Projects

Auto-Man provides specialized support for FreeRTOS-based projects, handling the specific requirements and configurations needed for real-time operating system development.

### Mixed Language Projects

For projects that combine Auto language and C code, Auto-Man handles the transpilation and integration seamlessly, allowing developers to leverage both languages' strengths.

### Custom Port Selection

When multiple ports are available, Auto-Man provides an interactive selection process:

```bash
automan build
# Prompts for port selection if multiple ports are available
```

## Getting Updates

Auto-Man is actively developed, with new features and improvements regularly added. To get the latest version:

```bash
# Update your package manager to get the latest Auto-Man version
```

## Troubleshooting

If you encounter issues:

1. Ensure your `pac.at` file is correctly configured
2. Try cleaning the project with `automan clean`
3. Check dependency versions and compatibility
4. Verify device support for your target hardware


## Web consumer and transaction owner

[PLAN-890 integration](web-integration.md) links the canonical Web domain and central actual delivery receipt. Man remains the generic installer/CLI/process owner.
