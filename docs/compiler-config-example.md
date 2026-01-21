# 编译器配置示例 (已弃用)

> **注意**: 此文档描述的配置方式已过时。请参考 [Port 编译器配置指南](port-compiler-config.md) 了解最新的配置方式。
>
> 新的配置系统将编译器配置直接放在项目的 `pac.at` 文件中，更加简洁和易于管理。

本文档展示如何在 `~/.auto/auto-man/am.at` 中配置编译器。

## 基本配置格式

在 `am.at` 文件中添加 `compilers` 节点,然后在其中定义各个编译器配置:

```rust
// index bases (保持现有配置)
index: {
    default: "git@gitee.com:auto-stack/auto-index.git"
}

// location to store automan related configurations
am: "${HOME}/.auto/auto-man"

// ===== 新增: 编译器配置 =====
compilers {
    // MSVC 编译器配置
    compiler("msvc") {
        kind: "MSVC"
        location: Env  // 从环境变量 PATH 查找

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

    // GCC 编译器配置
    compiler("gcc") {
        kind: "GCC"
        location: Env  // 从环境变量 PATH 查找

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

    // Clang 编译器配置
    compiler("clang") {
        kind: "Clang"
        location: Env

        executables {
            cc: "clang.exe"
            ar: "ar.exe"
            link: "clang.exe"
            as: "clang.exe"
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

    // IAR ARM 编译器配置
    compiler("iar-arm") {
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

    // GHS 编译器配置
    compiler("ghs") {
        kind: "GHS"
        location: Dir("C:/GHS/arm2022")

        executables {
            cc: "ccarm.exe"
            ar: "cxarm.exe"
            link: "ccarm.exe"
            as: "asarm.exe"
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
            "-O",
            "-g"
        ]
    }
}
```

## Location 配置选项

编译器位置支持三种配置方式:

### 1. Env (从环境变量 PATH 查找)

```rust
compiler("gcc") {
    kind: "GCC"
    location: Env
    // ...
}
```

### 2. Dir (指定安装目录)

```rust
compiler("iar-arm") {
    kind: "IAR"
    location: Dir("D:/soft/IAR_EWARM_9.2/arm/bin")
    // ...
}
```

### 3. Executable (直接指定可执行文件路径)

```rust
compiler("custom-gcc") {
    kind: "GCC"
    location: Executable("Compiler", "C:/custom/gcc/bin/gcc.exe")
    // ...
}
```

## Flag 格式配置

标志格式支持三种类型:

### Prefix (前缀格式)

```rust
include: Prefix("-I")  // 生成: -I/path
```

### Postfix (后缀格式)

```rust
library: Postfix(".lib")  // 生成: mylib.lib
```

### Both (前后缀格式)

```rust
output: Both("/Fo", ".obj")  // 生成: /Fooutput.obj
```

## Port 引用编译器配置

在项目的 `pac.at` 中,Port 可以引用 `am.at` 中定义的编译器:

```rust
// pac.at
port("windows_msvc") {
    platform: "windows"
    builder: "ninja"
    compiler: "msvc"  // 引用 am.at 中定义的 compiler("msvc")
}

port("linux_gcc") {
    platform: "linux"
    builder: "ninja"
    compiler: "gcc"  // 引用 am.at 中定义的 compiler("gcc")
}

port("arm_iar") {
    platform: "arm"
    builder: "ninja"
    compiler: "iar-arm"  // 引用 am.at 中定义的 compiler("iar-arm")
}
```

## 向后兼容

如果不指定 `compiler` 属性,系统将继续使用 `tool` 配置:

```rust
port("windows_ninja") {
    platform: "windows"
    builder: "ninja"

    // 使用旧的 tool 配置 (仍然支持)
    tool: {
        kind: "msvc"
        location: "env"
    }
}
```

## 优先级

编译器配置的优先级如下:

1. **Port.compiler**: 如果 Port 指定了 `compiler` 属性,优先使用 am.at 中对应的编译器配置
2. **Port.tool**: 如果没有指定 `compiler` 或找不到对应的编译器配置,则回退到 Port.tool 配置

## 完整示例

一个完整的 `am.at` 配置文件示例:

```rust
// index bases
index: {
    default: "git@gitee.com:auto-stack/auto-index.git"
}

// location to store automan related configurations
am: "${HOME}/.auto/auto-man"

// 编译器配置
compilers {
    compiler("msvc") {
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

    compiler("gcc") {
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

## 注意事项

1. **路径格式**: Windows 路径使用正斜杠 `/` 或双反斜杠 `\\`,例如 `C:/mingw64/bin` 或 `C:\\mingw64\\bin`
2. **大小写敏感**: 编译器名称 (如 "msvc", "gcc") 在 Port 引用时大小写敏感
3. **可执行文件扩展名**: Windows 平台需要包含 `.exe` 扩展名
4. **配置验证**: 如果引用的编译器配置不存在,系统会发出警告并回退到 Port.tool 配置

## 测试配置

配置完成后,可以通过以下方式测试:

```bash
# 在项目目录中
am build

# 查看生成的 build.ninja 文件
cat build/build.ninja
```

检查生成的编译命令是否使用了正确的编译器和标志。
