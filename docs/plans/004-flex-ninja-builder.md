# 灵活的 Ninja Builder 设计计划

## 1. 概述

### 1.1 目标

设计一个高度可配置、高度灵活的 Ninja Builder，能够：
- 支持多种编译器：MSVC, GCC, Clang, GHS, IAR, Targeting, Hightec
- 通过配置选项设置编译器安装目录
- 映射通用命令（`cc`, `ar`, `link`）到编译器特定命令
- 映射通用标志（`-I`, `-l`, `-D`）到编译器特定格式
- 使用 `am b` 一键编译，无需额外调用其他构建命令
- 提供良好的扩展性，便于添加新的编译器支持

### 1.2 当前实现的限制

在 [ninja.rs](../../crates/auto-man/src/builder/ninja.rs) 中，当前存在以下问题：

1. **硬编码的编译器命令**（154-179行）：
   - 命令模板直接写在代码中
   - 添加新编译器需要修改代码
   - 编译器路径从 `port.tool` 获取，无法灵活配置

2. **平台判断简单**（84-88行）：
   ```rust
   let obj_postfix = match target.port.platform.as_str() {
       "windows" => ".obj",
       "linux" => ".o",
       _ => ".o",
   };
   ```
   只简单判断 windows/linux，不够灵活

3. **标志格式硬编码**：
   - include 路径格式：`-I../{}`
   - defines 格式：`-D{}`（从 Tool::defines() 获取）
   - 不同编译器的标志格式差异未处理

4. **缺少编译器特定配置**：
   - 无法配置编译器安装目录
   - 无法自定义编译器特定参数

### 1.3 参考其他 Builder

- **CMakeBuilder** ([cmake.rs](../../crates/auto-man/src/builder/cmake.rs)): 完全依赖 CMake 的工具链抽象
- **IARBuilder** ([iar.rs](../../crates/auto-man/src/builder/iar.rs)): 使用模板系统生成项目文件，由 IAR IDE 处理

## 2. 架构设计

### 2.1 核心概念

#### 2.1.1 编译器配置（CompilerConfig）

每个编译器类型有一个独立的配置，包含：

```rust
pub struct CompilerConfig {
    pub kind: CompilerKind,
    pub name: String,
    pub executable_patterns: HashMap<String, String>,  // "cc" -> "cl.exe"
    pub flag_mappings: FlagMappings,
    pub location: CompilerLocation,
}

pub enum CompilerKind {
    MSVC,
    GCC,
    Clang,
    GHS,
    IAR,
    Targeting,
    Hightec,
}

pub struct FlagMappings {
    pub include: String,      // "-I{}", "/I{}", ...
    pub define: String,       // "-D{}", "/D{}", ...
    pub library: String,      // "-l{}", "{}.lib", ...
    pub library_path: String, // "-L{}", "/LIBPATH:{}", ...
    pub output: String,       // "-o {}", "/Fo{}", ...
    pub compile_only: String, // "-c", "/C", ...
}

pub enum CompilerLocation {
    EnvPath,              // 从 PATH 环境变量查找
    CustomDir(AutoPath),  // 自定义安装目录
    Executable(AutoPath), // 直接指定可执行文件路径
}
```

#### 2.1.2 命令模板系统

使用模板字符串替代硬编码命令：

```
rule cc
    command = {compiler_path} {compile_only_flag} {includes} {defines} {cflags} -o {output} {input}
```

替换为：

```rust
pub struct CommandTemplate {
    pub compile: String,
    pub assemble: String,
    pub link: String,
    pub archive: String,
}

impl CompilerConfig {
    pub fn get_command_template(&self) -> CommandTemplate {
        match self.kind {
            CompilerKind::MSVC => CommandTemplate {
                compile: format!("{{cc}} {{compile_only}} {{includes}} {{defines}} {{cflags}} /Fo{{output}} {{input}}"),
                assemble: format!("{{as}} {{includes}} {{aflags}} /Fo{{output}} {{input}}"),
                link: format!("{{link}} /OUT:{{output}} {{input}} {{libs}}"),
                archive: format!("{{ar}} /OUT:{{output}} {{input}}"),
            },
            CompilerKind::GCC => CommandTemplate {
                compile: format!("{{cc}} {{compile_only}} {{includes}} {{defines}} {{cflags}} -o{{output}} {{input}}"),
                assemble: format!("{{as}} {{includes}} {{aflags}} -o{{output}} {{input}}"),
                link: format!("{{link}} -o{{output}} {{input}} {{libs}}"),
                archive: format!("{{ar}} rcs {{output}} {{input}}"),
            },
            // ... 其他编译器
        }
    }
}
```

#### 2.1.3 配置文件格式

在 `~/.auto/auto-man/am.at` 中添加编译器配置：

```rust
// 编译器路径配置
compiler("msvc") {
    dir: "C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Tools/MSVC"
    cc: "cl.exe"
    ar: "lib.exe"
    link: "link.exe"
    as: "ml64.exe"

    flags {
        include: "/I{}"
        define: "/D{}"
        output: "/Fo{}"
    }
}

compiler("gcc") {
    dir: "C:/mingw64/bin"
    cc: "gcc.exe"
    ar: "ar.exe"
    link: "gcc.exe"
    as: "as.exe"

    flags {
        include: "-I{}"
        define: "-D{}"
        output: "-o{}"
    }
}

// Port 可以引用编译器配置
port("windows_msvc") {
    platform: "windows"
    builder: "ninja"
    toolchain: "msvc"  // 引用 compiler("msvc") 的配置
}
```

### 2.2 模块结构

```
crates/auto-man/src/builder/
├── ninja/
│   ├── mod.rs           # NinjaBuilder 主实现
│   ├── config.rs        # 编译器配置结构定义
│   ├── templates.rs     # 命令模板系统
│   ├── resolver.rs      # 编译器路径解析
│   └── mapper.rs        # 标志映射系统
└── ninja.rs             # 重新导出
```

#### 2.2.1 config.rs - 编译器配置

```rust
use crate::{AutoPath, AutoStr};
use std::collections::HashMap;

pub struct CompilerConfig {
    pub kind: CompilerKind,
    pub name: AutoStr,
    pub executables: HashMap<ExecutableType, AutoStr>,
    pub flags: FlagMappings,
    pub location: CompilerLocation,
    pub default_cflags: Vec<AutoStr>,
}

#[derive(Hash, Eq, PartialEq)]
pub enum ExecutableType {
    Compiler,    // cc
    Archiver,    // ar
    Linker,      // link
    Assembler,   // as
}

pub struct FlagMappings {
    pub include: FlagFormat,
    pub define: FlagFormat,
    pub library: FlagFormat,
    pub library_path: FlagFormat,
    pub output: FlagFormat,
    pub compile_only: FlagFormat,
}

pub enum FlagFormat {
    Prefix(String),           // "-I{}" -> -I/path
    Postfix(String),          // "{}.lib" -> path.lib
    Both(String, String),     // "/Fo{}", ".obj" -> /Fopath.obj
}

pub enum CompilerLocation {
    Env,                          // 从环境变量 PATH 查找
    Dir(AutoPath),                // 指定安装目录
    Executable(ExecutableType, AutoPath),  // 直接指定可执行文件
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompilerKind {
    MSVC,
    GCC,
    Clang,
    GHS,
    IAR,
    Targeting,
    Hightec,
}

impl CompilerKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "msvc" => Some(CompilerKind::MSVC),
            "gcc" | "gnu" => Some(CompilerKind::GCC),
            "clang" | "llvm" => Some(CompilerKind::Clang),
            "ghs" => Some(CompilerKind::GHS),
            "iar" => Some(CompilerKind::IAR),
            "targeting" => Some(CompilerKind::Targeting),
            "hightec" => Some(CompilerKind::Hightec),
            _ => None,
        }
    }
}
```

#### 2.2.2 templates.rs - 命令模板

```rust
pub struct CommandTemplates {
    pub compile: String,
    pub assemble: String,
    pub link: String,
    pub archive: String,
}

impl CommandTemplates {
    pub fn new(compiler: &CompilerConfig) -> Self {
        match compiler.kind {
            CompilerKind::MSVC => CommandTemplates {
                compile: "{cc} {compile_only} {includes} {defines} {cflags} /Fo{output} {input}",
                assemble: "{as} {includes} {aflags} /Fo{output} {input}",
                link: "{link} /OUT:{output} {input} {ldflags} {libs}",
                archive: "{ar} /OUT:{output} {input}",
            },
            CompilerKind::GCC => CommandTemplates {
                compile: "{cc} {compile_only} {includes} {defines} {cflags} -o{output} {input}",
                assemble: "{as} {includes} {aflags} -o{output} {input}",
                link: "{link} -o{output} {input} {ldflags} {libs}",
                archive: "{ar} rcs {output} {input}",
            },
            CompilerKind::Clang => CommandTemplates {
                compile: "{cc} {compile_only} {includes} {defines} {cflags} -o{output} {input}",
                assemble: "{as} {includes} {aflags} -o{output} {input}",
                link: "{link} -o{output} {input} {ldflags} {libs}",
                archive: "{ar} rcs {output} {input}",
            },
            CompilerKind::IAR => CommandTemplates {
                compile: "{cc} {includes} {defines} {cflags} --debug -o{output} {input}",
                assemble: "{as} {includes} {aflags} -o{output} {input}",
                link: "{link} -o{output} {input} {ldflags} {libs}",
                archive: "{ar} -o{output} {input}",
            },
            // ... GHS, Targeting, Hightec
        }
    }

    pub fn render(&self, template: &str, vars: &HashMap<&str, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }
}
```

#### 2.2.3 resolver.rs - 编译器路径解析

```rust
use crate::builder::ninja::config::{CompilerConfig, CompilerLocation, ExecutableType};
use crate::AutoPath;
use std::env;

pub struct CompilerResolver;

impl CompilerResolver {
    pub fn resolve_executable(
        config: &CompilerConfig,
        exec_type: ExecutableType,
    ) -> AutoPath {
        match &config.location {
            CompilerLocation::Env => {
                // 从 PATH 环境变量查找
                let exe_name = config.executables.get(&exec_type)
                    .unwrap_or(&Self::default_executable(config.kind, exec_type));
                Self::find_in_path(exe_name)
            }
            CompilerLocation::Dir(dir) => {
                // 从指定目录查找
                let exe_name = config.executables.get(&exec_type)
                    .unwrap_or(&Self::default_executable(config.kind, exec_type));
                dir.join(exe_name.as_str())
            }
            CompilerLocation::Executable(e_type, path) => {
                if *e_type == exec_type {
                    path.clone()
                } else {
                    Self::resolve_executable(config, exec_type)
                }
            }
        }
    }

    fn find_in_path(exe_name: &AutoStr) -> AutoPath {
        if let Ok(path_var) = env::var("PATH") {
            for path in env::split_paths(&path_var) {
                let exe_path = AutoPath::new(path).join(exe_name.as_str());
                if exe_path.exists() {
                    return exe_path;
                }
            }
        }
        // 如果找不到，返回可执行文件名本身（假设在 PATH 中）
        AutoPath::new(exe_name.as_str())
    }

    fn default_executable(kind: CompilerKind, exec_type: ExecutableType) -> AutoStr {
        match (kind, exec_type) {
            (CompilerKind::MSVC, ExecutableType::Compiler) => "cl.exe".into(),
            (CompilerKind::MSVC, ExecutableType::Linker) => "link.exe".into(),
            (CompilerKind::MSVC, ExecutableType::Archiver) => "lib.exe".into(),
            (CompilerKind::GCC, ExecutableType::Compiler) => "gcc.exe".into(),
            (CompilerKind::GCC, ExecutableType::Linker) => "gcc.exe".into(),
            (CompilerKind::GCC, ExecutableType::Archiver) => "ar.exe".into(),
            // ... 其他组合
        }
    }

    pub fn validate_compiler(kind: CompilerKind) -> AutoResult<bool> {
        // 尝试运行编译器 --version 命令验证
        let cc = match kind {
            CompilerKind::MSVC => "cl",
            CompilerKind::GCC => "gcc",
            CompilerKind::Clang => "clang",
            CompilerKind::GHS => "ccarm",
            CompilerKind::IAR => "iccarm",
            CompilerKind::Targeting => "tcpp",
            CompilerKind::Hightec => "gcc",
        };

        let result = std::process::Command::new(cc)
            .arg("--version")
            .output();

        match result {
            Ok(output) => Ok(output.status.success()),
            Err(_) => Ok(false),
        }
    }
}
```

#### 2.2.4 mapper.rs - 标志映射

```rust
use crate::builder::ninja::config::{CompilerConfig, FlagFormat};
use crate::AutoStr;
use std::collections::HashMap;

pub struct FlagMapper;

impl FlagMapper {
    pub fn map_include(config: &CompilerConfig, path: &AutoStr) -> String {
        match &config.flags.include {
            FlagFormat::Prefix(template) => {
                template.replace("{}", path.as_str())
            }
            FlagFormat::Postfix(template) => {
                template.replace("{}", path.as_str())
            }
            FlagFormat::Both(prefix, postfix) => {
                format!("{}{}{}", prefix, path.as_str(), postfix)
            }
        }
    }

    pub fn map_define(config: &CompilerConfig, define: &AutoStr) -> String {
        match &config.flags.define {
            FlagFormat::Prefix(template) => {
                template.replace("{}", define.as_str())
            }
            FlagFormat::Postfix(template) => {
                template.replace("{}", define.as_str())
            }
            FlagFormat::Both(prefix, postfix) => {
                format!("{}{}{}", prefix, define.as_str(), postfix)
            }
        }
    }

    pub fn map_library(config: &CompilerConfig, lib: &AutoStr) -> String {
        match &config.flags.library {
            FlagFormat::Prefix(template) => {
                template.replace("{}", lib.as_str())
            }
            FlagFormat::Postfix(template) => {
                template.replace("{}", lib.as_str())
            }
            FlagFormat::Both(prefix, postfix) => {
                format!("{}{}{}", prefix, lib.as_str(), postfix)
            }
        }
    }

    pub fn map_output(config: &CompilerConfig, output: &AutoStr, obj_ext: &str) -> String {
        match &config.flags.output {
            FlagFormat::Prefix(template) => {
                let base = output.as_str().trim_end_matches(obj_ext);
                template.replace("{}", base)
            }
            FlagFormat::Postfix(template) => {
                template.replace("{}", output.as_str())
            }
            FlagFormat::Both(prefix, postfix) => {
                let base = output.as_str().trim_end_matches(obj_ext);
                format!("{}{}{}", prefix, base, postfix)
            }
        }
    }

    pub fn build_includes(config: &CompilerConfig, includes: &[AutoStr]) -> Vec<String> {
        includes.iter()
            .map(|inc| Self::map_include(config, inc))
            .collect()
    }

    pub fn build_defines(config: &CompilerConfig, defines: &[AutoStr]) -> Vec<String> {
        defines.iter()
            .map(|def| Self::map_define(config, def))
            .collect()
    }
}
```

### 2.3 NinjaBuilder 重构

#### 2.3.1 新的 NinjaBuilder 结构

```rust
use crate::builder::ninja::config::CompilerConfig;
use crate::builder::ninja::resolver::CompilerResolver;
use crate::builder::ninja::mapper::FlagMapper;
use crate::builder::ninja::templates::CommandTemplates;

pub struct NinjaBuilder {
    pub gen: AutoGen,
    pub path: AutoPath,
    pub out: Box<dyn Write + 'static>,
    pub compiler: Option<CompilerConfig>,
    memory_mode: bool,
    memory_output: Vec<u8>,
}

impl NinjaBuilder {
    pub fn new(path: AutoPath) -> Self {
        Self {
            gen: Self::new_gen(&path),
            path,
            out: Box::new(std::io::sink()),
            compiler: None,
            memory_mode: false,
            memory_output: Vec::new(),
        }
    }

    // 从 Port 加载编译器配置
    fn load_compiler_config(&mut self, port: &Port) -> AutoResult<()> {
        // 1. 尝试从 Port.tool 获取配置
        let kind = port.tool.kind;

        // 2. 创建编译器配置
        let config = CompilerConfig {
            kind: kind.into(),
            name: port.tool.name.clone(),
            executables: Self::get_executables(&port.tool),
            flags: Self::get_flag_mappings(kind),
            location: Self::get_compiler_location(&port.tool),
            default_cflags: Self::get_default_cflags(kind),
        };

        self.compiler = Some(config);
        Ok(())
    }

    fn get_executables(tool: &Tool) -> HashMap<ExecutableType, AutoStr> {
        let mut map = HashMap::new();
        map.insert(ExecutableType::Compiler, tool.compiler.clone());
        map.insert(ExecutableType::Linker, tool.linker.clone());
        map.insert(ExecutableType::Archiver, tool.archiver.clone());
        map.insert(ExecutableType::Assembler, tool.assembler.clone());
        map
    }

    fn get_flag_mappings(kind: ToolKind) -> FlagMappings {
        match kind {
            ToolKind::Msvc => FlagMappings {
                include: FlagFormat::Prefix("/I".to_string()),
                define: FlagFormat::Prefix("/D".to_string()),
                library: FlagFormat::Postfix(".lib".to_string()),
                library_path: FlagFormat::Prefix("/LIBPATH:".to_string()),
                output: FlagFormat::Both("/Fo", ".obj"),
                compile_only: FlagFormat::Prefix("/c".to_string()),
            },
            ToolKind::Gcc => FlagMappings {
                include: FlagFormat::Prefix("-I".to_string()),
                define: FlagFormat::Prefix("-D".to_string()),
                library: FlagFormat::Prefix("-l".to_string()),
                library_path: FlagFormat::Prefix("-L".to_string()),
                output: FlagFormat::Prefix("-o".to_string()),
                compile_only: FlagFormat::Prefix("-c".to_string()),
            },
            ToolKind::Clang => FlagMappings {
                include: FlagFormat::Prefix("-I".to_string()),
                define: FlagFormat::Prefix("-D".to_string()),
                library: FlagFormat::Prefix("-l".to_string()),
                library_path: FlagFormat::Prefix("-L".to_string()),
                output: FlagFormat::Prefix("-o".to_string()),
                compile_only: FlagFormat::Prefix("-c".to_string()),
            },
            // ... 其他编译器
        }
    }
}
```

#### 2.3.2 重构 setup() 方法

```rust
impl Builder for NinjaBuilder {
    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()> {
        pac.collect_srcs()?;
        self.reset_ninja_file()?;

        // 加载编译器配置
        self.load_compiler_config(&pac.port)?;
        let compiler = self.compiler.as_ref().unwrap();

        // 解析编译器路径
        let cc_path = CompilerResolver::resolve_executable(compiler, ExecutableType::Compiler);
        let as_path = CompilerResolver::resolve_executable(compiler, ExecutableType::Assembler);
        let link_path = CompilerResolver::resolve_executable(compiler, ExecutableType::Linker);
        let ar_path = CompilerResolver::resolve_executable(compiler, ExecutableType::Archiver);

        // 构建标志
        let includes = Self::build_includes(compiler, pac.all_incs());
        let defines = Self::build_defines(compiler, &pac.defines);
        let cflags = Self::build_cflags(compiler);

        // 获取命令模板
        let templates = CommandTemplates::new(compiler);

        // 渲染命令
        let vars = hashmap!{
            "cc" => cc_path.to_astr().as_str(),
            "as" => as_path.to_astr().as_str(),
            "link" => link_path.to_astr().as_str(),
            "ar" => ar_path.to_astr().as_str(),
            "includes" => &includes.join(" "),
            "defines" => &defines.join(" "),
            "cflags" => &cflags.join(" "),
        };

        let asm_cmd = templates.render(&templates.assemble, &vars);
        let cc_cmd = templates.render(&templates.compile, &vars);
        let link_cmd = templates.render(&templates.link, &vars);
        let lib_cmd = templates.render(&templates.archive, &vars);

        // 写入 build.ninja
        self.out.write(format!(r#"
# ninja build file for {} with {} compiler

# includes
includes = {}

# defines
defines = {}

# cflags
cflags = {}

# rules
rule as
    command = {}
    description = Assembling $in

rule cc
    command = {}
    description = Compiling $in

rule link
    command = {}
    description = Linking $out

rule lib
    command = {}
    description = Building library $out

"#, pac.name, compiler.name, includes.join(" "), defines.join(" "), cflags.join(" "),
    asm_cmd, cc_cmd, link_cmd, lib_cmd).as_bytes())?;

        Ok(())
    }

    fn build_includes(compiler: &CompilerConfig, paths: Vec<AutoStr>) -> Vec<String> {
        paths.iter()
            .map(|p| FlagMapper::map_include(compiler, p))
            .collect()
    }

    fn build_defines(compiler: &CompilerConfig, defines: &[AutoStr]) -> Vec<String> {
        defines.iter()
            .map(|d| FlagMapper::map_define(compiler, d))
            .collect()
    }
}
```

## 3. 配置系统

### 3.1 用户配置文件

在 `~/.auto/auto-man/am.at` 中添加编译器配置支持：

```rust
// 编译器配置节
compilers {
    compiler("msvc") {
        kind: "MSVC"
        location: Env  // 或 Dir("C:/Program Files/...")

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
}

// Port 引用编译器配置
ports {
    port("windows_msvc") {
        platform: "windows"
        builder: "ninja"
        compiler: "msvc"  // 引用 compilers.compiler("msvc")
    }

    port("linux_gcc") {
        platform: "linux"
        builder: "ninja"
        compiler: "gcc"
    }

    port("arm_iar") {
        platform: "arm"
        builder: "ninja"
        compiler: "iar-arm"
    }
}
```

### 3.2 配置解析

扩展 AutoConfig 语法以支持新的编译器配置节点：

```rust
// src/config.rs
pub fn parse_compiler_configs(root: &Node) -> HashMap<String, CompilerConfig> {
    let mut configs = HashMap::new();

    if let Some(compilers_node) = root.get_kid("compilers") {
        for compiler_node in compilers_node.kids {
            if compiler_node.name() == "compiler" {
                let name = compiler_node.get_prop("name").unwrap();
                let kind = compiler_node.get_prop("kind").unwrap();
                let config = parse_compiler_config(&compiler_node);
                configs.insert(name, config);
            }
        }
    }

    configs
}

fn parse_compiler_config(node: &Node) -> CompilerConfig {
    let kind: CompilerKind = node.get_prop("kind")
        .unwrap()
        .as_str()
        .parse()
        .unwrap();

    let location = parse_location(node.get_kid("location"));

    let executables = parse_executables(node.get_kid("executables"));

    let flags = parse_flags(node.get_kid("flags"));

    let default_cflags = node.get_kid("default_cflags")
        .kids
        .iter()
        .map(|n| n.value())
        .collect();

    CompilerConfig {
        kind,
        name: node.get_prop("name").unwrap(),
        executables,
        flags,
        location,
        default_cflags,
    }
}
```

## 4. 实施步骤

### 4.1 Phase 1: 基础结构（1-2天）

**任务**：
- [ ] 创建 `builder/ninja/` 子模块
- [ ] 实现 `config.rs` - 定义所有配置结构
- [ ] 实现 `templates.rs` - 命令模板系统
- [ ] 实现 `mapper.rs` - 标志映射系统
- [ ] 实现 `resolver.rs` - 编译器路径解析
- [ ] 更新 `mod.rs` 重新导出子模块

**验收标准**：
- 所有新模块编译通过
- 结构定义完整，类型安全

### 4.2 Phase 2: NinjaBuilder 重构（2-3天）

**任务**：
- [ ] 重构 `NinjaBuilder::setup()` 使用新的配置系统
- [ ] 实现 `load_compiler_config()` 从 Port 加载配置
- [ ] 重构命令生成逻辑使用模板系统
- [ ] 实现标志映射应用
- [ ] 保持向后兼容性（支持旧的 Port.tool 配置）

**验收标准**：
- NinjaBuilder 可以使用新配置系统构建
- 现有测试用例通过
- MSVC/GCC/Clang 编译器支持正常工作

### 4.3 Phase 3: 配置文件支持（1-2天）✅ **已完成**

**完成日期**: 2025-01-20

**任务**：
- [x] 扩展 AutoConfig 解析器支持 compiler 节点
- [x] 实现 `CompilerStore::load_from_am_config()`
- [x] 实现配置节到 `CompilerConfig` 的转换
- [x] 添加配置验证（检查编译器是否存在）
- [x] 编写配置示例和文档

**实现的文件**：
- `crates/auto-man/src/builder/ninja/compiler_store.rs` (新文件, 263行)
- `crates/auto-man/src/port.rs` - 添加 `compiler_name` 字段
- `crates/auto-man/src/pac.rs` - 添加 `compiler_store` 字段和 `load_compiler_store()` 方法
- `crates/auto-man/src/automan.rs` - 在 `parse_pac()` 中加载 compiler_store
- `crates/auto-man/src/builder.rs` - 将 ninja 模块设为公开
- `docs/compiler-config-example.md` - 配置示例文档

**验收标准**：
- [x] 可以从 `am.at` 加载编译器配置
- [x] Port 可以引用编译器配置
- [x] 配置验证工作正常（86个单元测试全部通过）

**关键实现细节**：

1. **CompilerStore**: 新模块负责从 `~/.auto/auto-man/am.at` 加载编译器配置
   - 支持 7 种编译器类型: MSVC, GCC, Clang, GHS, IAR, Targeting, Hightec
   - 支持三种 location: Env, Dir, Executable
   - 支持三种 flag 格式: Prefix, Postfix, Both

2. **Port 扩展**: 添加 `compiler_name: Option<AutoStr>` 字段,用于引用编译器配置

3. **优先级系统**:
   - 优先使用 `Port.compiler_name` 引用的编译器配置
   - 如果找不到,回退到 `Port.tool` 配置
   - 确保向后兼容性

4. **配置解析**:
   - 使用 ValueKey 进行节点查找
   - 使用 kids_iter() 遍历子节点
   - 正确处理 Kid::Node 和 Kid::Lazy 类型

**测试结果**:
```
test result: ok. 86 passed; 0 failed; 0 ignored
```

### 4.4 Phase 4: 扩展编译器支持（2-3天）

**任务**：
- [ ] 添加 GHS 编译器配置
- [ ] 添加 IAR 编译器配置（完整）
- [ ] 添加 Targeting 编译器配置
- [ ] 添加 Hightec 编译器配置
- [ ] 为每个编译器编写测试用例

**验收标准**：
- 所有 7 种编译器都有完整配置
- 每个编译器的标志映射正确
- 测试覆盖所有编译器类型

### 4.5 Phase 5: 测试和文档（2-3天）

**任务**：
- [ ] 扩展集成测试框架支持多编译器
- [ ] 为每个编译器添加测试用例
- [ ] 编写用户文档：如何配置编译器
- [ ] 编写开发者文档：如何添加新编译器
- [ ] 更新 CLAUDE.md

**验收标准**：
- 每个编译器至少有一个通过的测试
- 文档完整清晰
- 用户可以按照文档配置自定义编译器

### 4.6 Phase 6: 优化和完善（1-2天）

**任务**：
- [ ] 性能优化（缓存编译器路径查找结果）
- [ ] 错误提示改进（给出配置建议）
- [ ] 添加 `am compiler` 命令列出可用编译器
- [ ] 添加 `am compiler validate` 验证编译器配置
- [ ] 代码审查和重构

**验收标准**：
- 编译器路径查找结果被缓存
- 错误消息友好且有帮助
- 新命令工作正常

## 5. 测试策略

### 5.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_mapper_msvc() {
        let config = CompilerConfig::msvc_default();
        assert_eq!(
            FlagMapper::map_include(&config, &"include".into()),
            "/Iinclude"
        );
        assert_eq!(
            FlagMapper::map_define(&config, &"DEBUG".into()),
            "/DDEBUG"
        );
    }

    #[test]
    fn test_flag_mapper_gcc() {
        let config = CompilerConfig::gcc_default();
        assert_eq!(
            FlagMapper::map_include(&config, &"include".into()),
            "-Iinclude"
        );
        assert_eq!(
            FlagMapper::map_define(&config, &"DEBUG".into()),
            "-DDEBUG"
        );
    }

    #[test]
    fn test_compiler_resolver() {
        let config = CompilerConfig {
            kind: CompilerKind::GCC,
            location: CompilerLocation::Env,
            // ...
        };

        let cc = CompilerResolver::resolve_executable(&config, ExecutableType::Compiler);
        assert!(!cc.to_astr().is_empty());
    }
}
```

### 5.2 集成测试

在 `tests/integration/` 下添加多编译器测试：

```
tests/integration/
├── 001_simple_app/
├── 002_msvc_build/
│   ├── pac.at
│   ├── source/
│   │   └── main.c
│   └── expected/
│       └── ninja/
│           └── build.ninja  # 预期的 MSVC build.ninja
├── 003_gcc_build/
│   ├── pac.at
│   ├── source/
│   │   └── main.c
│   └── expected/
│       └── ninja/
│           └── build.ninja  # 预期的 GCC build.ninja
├── 004_iar_build/
└── 005_multi_compiler/
```

### 5.3 端到端测试

测试真实编译器调用：

```rust
#[test]
#[ignore]  // 需要真实环境，手动运行
fn test_build_with_gcc() {
    let test_dir = setup_test_project("gcc");
    let result = std::process::Command::new("am")
        .args(["build"])
        .current_dir(&test_dir)
        .output()
        .expect("Failed to run am build");

    assert!(result.status.success());

    let exe = test_dir.join("build").join("main");
    assert!(exe.exists());

    let run_result = std::process::Command::new(exe)
        .output()
        .expect("Failed to run executable");

    assert!(run_result.status.success());
}
```

## 6. 向后兼容性

### 6.1 保留旧配置格式

继续支持 `Port.tool` 配置，自动转换为新的 `CompilerConfig`：

```rust
impl From<&Tool> for CompilerConfig {
    fn from(tool: &Tool) -> Self {
        Self {
            kind: tool.kind.into(),
            name: tool.name.clone(),
            executables: Self::tool_to_executables(tool),
            flags: Self::tool_kind_to_flags(tool.kind),
            location: Self::tool_to_location(tool),
            default_cflags: Self::tool_kind_to_cflags(tool.kind),
        }
    }
}
```

### 6.2 渐进式迁移

用户可以选择：
1. 继续使用旧的 `Port.tool` 配置（自动转换）
2. 逐步迁移到新的 `compiler` 配置节点
3. 两种配置可以共存，新的优先

## 7. 优势总结

### 7.1 与现有 Builder 的对比

| 特性 | CMakeBuilder | IARBuilder | GHSBuilder | NinjaBuilder (新) |
|------|-------------|-----------|-----------|------------------|
| 直接编译 | ❌ 需要调用 cmake | ❌ 需要打开 IDE | ❌ 需要调用 GHS | ✅ `am b` 直接编译 |
| 多编译器 | ✅ 通过 CMake | ❌ 仅 IAR | ❌ 仅 GHS | ✅ 原生支持 7+ |
| 编译器配置 | ⚠️ 通过 CMake | ❌ 硬编码 | ❌ 硬编码 | ✅ 灵活配置 |
| 标志映射 | ✅ 通过 CMake | ❌ 固定 | ❌ 固定 | ✅ 可扩展 |
| 自定义参数 | ✅ 通过 CMake | ⚠️ 受限 | ⚠️ 受限 | ✅ 完全控制 |

### 7.2 设计优势

1. **高度可配置**：
   - 编译器安装目录可配置
   - 命令名称可自定义
   - 标志格式可扩展

2. **类型安全**：
   - 使用 Rust 类型系统确保配置正确性
   - 编译期检查编译器类型

3. **易于扩展**：
   - 添加新编译器只需：
     - 实现 `CompilerKind` 枚举值
     - 定义标志映射
     - 定义命令模板
     - 无需修改核心逻辑

4. **测试友好**：
   - 内存输出支持已有（Phase 2 完成）
   - 配置与实现分离，易于 mock

5. **用户体验优秀**：
   - 一键编译 `am b`
   - 配置集中管理
   - 错误提示清晰

## 8. 风险和挑战

### 8.1 潜在风险

1. **编译器路径检测**：
   - 不同操作系统路径格式差异
   - 环境变量 PATH 复杂性
   - **缓解**：提供多级 fallback（Env → 默认路径 → 用户指定）

2. **配置复杂性**：
   - 用户需要了解编译器参数
   - **缓解**：提供合理的默认配置，高级选项可选

3. **向后兼容性**：
   - 旧配置格式需要长期支持
   - **缓解**：自动转换机制，充分测试

4. **测试覆盖**：
   - 需要 7 种编译器的测试环境
   - **缓解**：单元测试覆盖逻辑，集成测试使用内存输出

### 8.2 实施风险

- **总工作量估计**：10-15 天
- **复杂度**：中高（涉及配置系统、编译器知识）
- **依赖**：无外部依赖，纯内部重构
- **风险级别**：低（不影响现有功能，渐进式实施）

## 9. 后续改进

### 9.1 Phase 7+ 可选功能

1. **编译器缓存**：
   - 缓存编译器路径查找结果
   - 缓存编译器版本信息

2. **自动检测**：
   - 自动扫描系统可用编译器
   - `am auto-detect` 命令

3. **编译器特性检测**：
   - 检测编译器支持的特性（C11, C++17 等）
   - 自动调整标志

4. **IDE 集成**：
   - 生成 VS Code tasks.json
   - 生成 CLion configurations

5. **分布式编译**：
   - 支持 ccache
   - 支持 distcc

## 10. 总结

本设计实现了一个灵活、可配置的 Ninja Builder，具备以下特点：

✅ **支持 7+ 种编译器**：MSVC, GCC, Clang, GHS, IAR, Targeting, Hightec
✅ **灵活配置**：编译器路径、命令名称、标志格式均可配置
✅ **一键编译**：`am b` 直接编译，无需额外命令
✅ **类型安全**：Rust 类型系统保证配置正确性
✅ **易于扩展**：添加新编译器无需修改核心逻辑
✅ **向后兼容**：支持旧的配置格式
✅ **测试友好**：内存输出支持，易于测试

实施时间：10-15 天
风险级别：低
优先级：高（显著改善用户体验）

---

**文档版本**: 1.0
**创建日期**: 2025-01-20
**最后更新**: 2025-01-20
