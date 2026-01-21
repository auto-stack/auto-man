# Port 编译器配置指南

本文档展示如何在项目的 `pac.at` 文件中为 `port` 节点配置编译器。

## 概述

在 AutoMan 的统一配置系统中，编译器配置直接位于项目的 `pac.at` 文件的 `port` 节点内。这种方式的优势：

- **项目独立性**: 每个项目可以定义自己的编译器配置
- **版本控制**: 编译器配置可以提交到代码仓库，团队成员共享
- **灵活配置**: 不同项目可以使用不同的编译器版本和参数

## Port 编译器配置格式

在 `pac.at` 文件中，port 节点可以包含完整的编译器配置：

```rust
port("my_port") {
    platform: "windows"
    builder: "ninja"

    // 编译器配置 (可选)
    compiler {
        kind: "GCC"
        location: Dir("C:/mingw64/bin")

        executables {
            cc: "gcc.exe"
            ar: "ar.exe"
            link: "gcc.exe"
            as: "as.exe"
        }

        flags {
            include: Prefix("-I")
            define: Prefix("-D")
            library: Prefix("-l")
            library_path: Prefix("-L")
            output: Prefix("-o")
            compile_only: Prefix("-c")
        }

        default_cflags: [
            "-O2",
            "-Wall"
        ]
    }
}
```

## 配置选项说明

### compiler.kind

编译器类型，支持的值：
- `"MSVC"` - Microsoft Visual C++
- `"GCC"` - GNU Compiler Collection
- `"Clang"` - LLVM Clang
- `"IAR"` - IAR Embedded Workbench
- `"GHS"` - Green Hills MULTI
- `"Targeting"` - Targeting Compiler
- `"Hightec"` - Hightec GCC

### compiler.location

编译器位置，支持三种格式：

#### 1. Env (从环境变量 PATH 查找)

```rust
compiler {
    kind: "GCC"
    location: Env
    // ...
}
```

#### 2. Dir (指定安装目录)

```rust
compiler {
    kind: "IAR"
    location: Dir("D:/soft/IAR_EWARM_9.2/arm/bin")
    // ...
}
```

#### 3. Executable (直接指定可执行文件路径)

```rust
compiler {
    kind: "GCC"
    location: Executable("Compiler", "C:/custom/gcc/bin/gcc.exe")
    // ...
}
```

### compiler.executables

定义编译器相关工具的可执行文件名称：

```rust
executables {
    cc: "gcc.exe"      // 编译器
    ar: "ar.exe"       // 归档工具 (静态库)
    link: "gcc.exe"    // 链接器
    as: "as.exe"       // 汇编器
}
```

### compiler.flags

定义编译器标志的格式，支持三种类型：

#### Prefix (前缀格式)

```rust
include: Prefix("-I")  // 生成: -I/path
define: Prefix("-D")   // 生成: -DDEBUG
```

#### Postfix (后缀格式)

```rust
library: Postfix(".lib")  // 生成: mylib.lib
```

#### Both (前后缀格式)

```rust
output: Both("/Fo", ".obj")  // 生成: /Fooutput.obj
```

### compiler.default_cflags

默认的编译标志数组：

```rust
default_cflags: [
    "-O2",
    "-Wall",
    "-fPIC"
]
```

## 完整配置示例

### Windows MSVC 配置

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

### Linux GCC 配置

```rust
port("linux_gcc") {
    platform: "linux"
    builder: "ninja"
    at: "build"

    compiler {
        kind: "GCC"
        location: Env

        executables {
            cc: "gcc"
            ar: "ar"
            link: "gcc"
            as: "as"
        }

        flags {
            include: Prefix("-I")
            define: Prefix("-D")
            library: Prefix("-l")
            library_path: Prefix("-L")
            output: Prefix("-o")
            compile_only: Prefix("-c")
        }

        default_cflags: [
            "-O2",
            "-Wall"
        ]
    }
}
```

### ARM IAR 配置

```rust
port("arm_iar") {
    platform: "arm"
    builder: "ninja"
    at: "build"

    compiler {
        kind: "IAR"
        location: Dir("D:/soft/IAR_EWARM_9.2/arm/bin")

        executables {
            cc: "iccarm.exe"
            ar: "iarchive.exe"
            link: "ilinkarm.exe"
            as: "iasmarm.exe"
        }

        flags {
            include: Prefix("-I")
            define: Prefix("-D")
            library: Prefix("-l")
            library_path: Prefix("-L")
            output: Prefix("-o")
            compile_only: Prefix("--debug")
        }

        default_cflags: [
            "--no_cse",
            "--no_unroll",
            "--no_inline",
            "--no_code_motion",
            "--no_tbaa",
            "--no_clustering",
            "--no_scheduling",
            "--debug",
            "--endian=little",
            "--cpu=Cortex-M4",
            "-e",
            "--fpu=VFPv4_sp"
        ]
    }
}
```

### 简化配置 (使用默认值)

如果省略 `compiler` 节点，系统会使用平台默认配置：

```rust
port("simple") {
    platform: "windows"
    builder: "ninja"
    // 不指定 compiler，使用默认 MSVC 配置
}
```

## 注意事项

1. **路径格式**: Windows 路径使用正斜杠 `/` 或双反斜杠 `\\`，例如 `C:/mingw64/bin` 或 `C:\\mingw64\\bin`
2. **大小写敏感**: 编译器类型 (如 "MSVC", "GCC") 大小写不敏感
3. **可执行文件扩展名**: Windows 平台需要包含 `.exe` 扩展名
4. **配置继承**: 如果未指定 `flags` 或 `executables`，系统会使用 `kind` 对应的默认配置
5. **多 Port 支持**: 一个项目可以定义多个 port，每个使用不同的编译器配置

## 测试配置

配置完成后，可以通过以下方式测试：

```bash
# 在项目目录中
am build

# 查看生成的 build.ninja 文件
cat build/build.ninja
```

检查生成的编译命令是否使用了正确的编译器和标志。

## 配置验证

AutoMan 会在构建时自动验证编译器配置：
- 检查指定的可执行文件是否存在
- 验证编译器是否可以正常执行
- 如果验证失败，会显示错误信息并退出构建

## 向后兼容

如果不指定 `compiler` 节点，系统会使用平台默认配置：
- Windows: MSVC (从环境变量 PATH 查找)
- Linux: GCC (从环境变量 PATH 查找)
- macOS: Clang (从环境变量 PATH 查找)

## 配置最佳实践

1. **团队协作**: 将编译器配置提交到版本控制，确保所有团队成员使用相同的配置
2. **路径灵活性**: 对于团队项目，优先使用 `Env` 类型，让每个开发者配置自己的 PATH
3. **文档化**: 在项目中添加 README 说明如何配置编译器环境
4. **版本锁定**: 使用 `Dir` 类型指定特定版本的编译器，确保构建可重现性
