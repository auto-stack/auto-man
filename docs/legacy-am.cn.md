# AutoMan

现代化的嵌入式 C/Auto 开发构建系统和包管理器。类似于 Rust 的 Cargo，AutoMan 专注于嵌入式系统开发，支持多平台构建（CMake、IAR、GHS、Ninja）。

**核心理念**：Auto 是一种中间语言，可转译为 C 语言，在提供类型安全和现代特性的同时，与现有 C 代码库无缝集成。

## 特性

- **构建管理**：Auto 转译为 C，C 编译为二进制程序
- **多平台构建**：支持 CMake、IAR、GHS 和 CCS 等编译系统
- **包管理**：以 `pac`（包）为单位组织代码库
- **依赖管理**：基于 Git 的自动依赖下载和版本解析
- **设备资源**：配置并下载硬件资源（芯片库、配置文件等）
- **测试框架**：内置测试系统，自动运行测试用例

## 安装

```bash
# 从源码构建
cargo build --release

# 本地安装（安装 `am` 命令）
cargo install --path .
```

## 快速开始

### 创建新项目

```bash
# Auto 语言应用程序
am app hello

# Auto 语言库
am lib mymath

# C 语言应用程序
am capp hello

# C 语言库
am clib mymath
```

### 构建和运行

```bash
cd hello
am build    # 构建项目（简写：am b）
am run      # 运行可执行文件（简写：am r）
```

## 项目结构

典型的 AutoMan 项目结构：

```
hello/
├── pac.at          # 项目配置文件
├── hello.at        # Auto 源文件
└── deps/           # 下载的依赖
```

`pac.at` 文件使用 AutoConfig 格式（类似 JSON）定义项目元数据、目标和依赖：

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

## 构建目标

AutoMan 支持多种类型的构建目标：

| 类型 | 描述 | 输出 |
|------|------|------|
| `app` | 可执行应用程序 | `.exe` / 二进制文件 |
| `lib` | 静态库 | `.lib` / `.a` |
| `dep` | 外部依赖 | 从 git 下载 |
| `device` | 硬件资源包 | 芯片支持文件 |
| `bag` | 代码集合 | 不独立编译 |
| `test` | 测试单元 | 测试可执行文件 |

### 配置示例

```rust
// 定义一个库
lib("mymath") {
    at: "libs/mymath"
}

// 定义带依赖的应用程序
app("hello") {
    link("mymath") {}
    link("log") {}

    dir("src") {}
    dir("include") {}
}
```

## 依赖管理

AutoMan 使用基于 Git 的依赖管理。在 `pac.at` 中添加依赖：

```rust
name: "myapp"
version: "0.1.0"

// 指定版本的依赖
dep("mymath", version: "0.1.5") {}

// 或使用最新版本（master:HEAD）
dep("log") {}

app("myapp") {
    link("mymath") {}
    link("log") {}
}
```

### 依赖命令

```bash
am deps        # 列出所有可用的依赖
am devices     # 列出所有可用的设备资源
am pull        # 更新所有依赖和索引
```

### 在代码中使用依赖

**Auto 语言：**
```rust
use mymath:add

fn main {
    print(add(1, 2))
}
```

**C 语言：**
```c
#include "mymath.h"

int main(void) {
    printf("%d\n", mymath_add(1, 2));
    return 0;
}
```

## 移植目标和构建系统

AutoMan 使用**移植目标（Port）**来定义编译目标（平台 + 工具链 + 构建器）。默认端口：

- **Windows**：`windows_ninja`（MSVC + Ninja）
- **Linux**：`linux_ninja`（GCC + Ninja）

### 支持的构建器

| 构建器 | 描述 | 平台 |
|--------|------|------|
| CMake | CMake 构建系统 | Windows (VS)、Linux (GCC) |
| IAR | IAR Embedded Workbench | ARM、RH850 等 |
| GHS | Green Hills MULTI | PowerPC、ARM 等 |
| Ninja | Ninja 构建系统 | 快速增量构建 |

### 端口管理

```bash
am port       # 显示并选择当前端口
```

可以在 `~/.auto/auto-man/am.at` 中配置自定义端口：

```rust
port("iar_arm") {
    platform: "windows"
    builder: "iar"
    tool: "iar"
    at: "build_iar"
}
```

## 编译器配置

从 v0.1.4 开始，AutoMan 支持在项目的 `pac.at` 文件中直接配置编译器：

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

详细的编译器配置说明请参考 [Port 编译器配置指南](docs/port-compiler-config.md)。

## 构建流程

运行 `am build` 会执行以下步骤：

1. **扫描阶段**：解析 `pac.at`，下载依赖，发现源文件
2. **转译阶段**：使用 `auto-lang` 转译器将 `.at` 文件转换为 `.h`/`.c` 文件
3. **生成构建文件**：生成 CMakeLists.txt、.ewp（IAR）或其他构建配置
4. **编译**：调用构建系统（CMake、IAR、GHS、Ninja）

输出：构建产物位于 `build/` 目录（例如 `build/Debug/hello.exe`）

## AutoConfig 语法

AutoConfig 是一种配置语言，支持静态声明和动态脚本：

### 静态结构

```rust
// 属性
name: "myproject"
version: "0.1.0"
features: ["feature1", "feature2"]

// 嵌套对象
config: {
    optimization: "O2"
    warnings: true
}

// 节点
app("main") {
    at: "apps/main"
    recurse: true

    dir("src") {
        dir("utils") {}
    }
}
```

### 动态脚本

```rust
// 变量声明
var target_chip = "RH850"

// 条件编译
if port == target_chip {
    device("Renesas-RH850") {}
} else {
    device("Generic-ARM") {}
}
```

## 多目标支持

单个包可以包含多个构建目标：

```rust
name: "workspace"
version: "0.1.0"

app("main") {}
app("cli") {}
lib("utils") {}
lib("math") {}
test("utils_test") {}
```

当运行 `am run` 时，如果存在多个应用程序，系统会提示选择要执行的目标。

## Auto/C 混合项目

Auto 和 C 目标可以在同一项目中混合使用。构建流程：

1. 扫描所有目标的 `.at` 文件（Auto 源文件）
2. 将所有 `.at` 转译为 `.h`/`.c`
3. 再次扫描 `.c`/`.h` 文件（C 源文件）
4. 编译所有 C 源文件

类似于 TypeScript 编译为 JavaScript，Auto 编译为 C。

## 测试

AutoMan 包含内置测试框架（仅支持 Auto 语言）：

```bash
am addtest mymath    # 为 mymath 生成测试文件
am test              # 运行所有测试
```

生成的测试文件（`test_mymath.at`）：

```rust
fn test_add {
    assert_eq(5, add(2, 3))
}
```

## 高级命令

```bash
# 项目管理
am clean       # 清理构建产物
am open        # 在 IDE 中打开项目（简写：am o）
am info        # 显示包/目标信息（简写：am i）

# 配置
am reset       # 重置 automan 配置和索引
am install     # 安装自定义 am.at 配置文件

# 更新
am upgrade     # 更新 AutoMan 到最新版本
```

## 目录扫描

默认情况下，AutoMan 会扫描以下目录中的源文件：

- 当前目录
- `src/` - 源文件目录
- `inc/` - 头文件目录
- `include/` - 头文件目录

支持的文件类型：
- Auto：`.at`
- C：`.c`、`.h`
- 汇编：`.s`、`.S`、`.850`

## 索引系统

AutoMan 使用类似 crates.io 的集中式索引系统：

- 包以 `vendor/pac` 标识（例如 `soutek/mymath`）
- 防止不同供应商之间的命名冲突
- 从 git 仓库下载

可以在 `~/.auto/auto-man/am.at` 中配置索引仓库：

```rust
index: {
    default: "git@gitee.com:auto-stack/auto-index.git"
    custom: "git@github.com:myorg/auto-index.git"
}
```

## 开发

### 构建 AutoMan

```bash
cargo build --release
```

### 运行测试

```bash
cargo test
```

### 项目结构

- `src/automan.rs` - 核心编排逻辑
- `src/builder.rs` - 构建器 trait 和实现
- `src/target.rs` - 目标发现和扫描
- `src/port.rs` - 端口/平台/工具链定义
- `src/pac.rs` - 包配置解析
- `src/index.rs` - 包/设备索引管理
- `src/git.rs` - 依赖的 Git 操作
- `src/asset.rs` - 嵌入式模板系统
- `src/scanner.rs` - 统一的目录扫描逻辑
- `src/file_types.rs` - 文件类型注册表
- `src/node_ext.rs` - Node 属性扩展

## 许可证

本项目是 AutoStack 生态系统的一部分。

## 相关项目

- [auto-lang](../auto-lang/) - Auto 编程语言
- [auto-val](../auto-lang/) - Auto 的值类型
- [auto-gen](../auto-lang/) - 代码生成工具
