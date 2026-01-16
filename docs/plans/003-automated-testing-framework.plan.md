# Auto-Man 自动化测试框架设计计划

## 需求背景

### 参考案例：auto-lang a2c 转译器测试

auto-lang 在开发 a2c 转译器时设计了一套基于文件的测试用例方案：

**测试用例结构**：
```
test/a2c/
├── 000_hello/
│   ├── hello.at              # Auto 源代码输入
│   ├── hello.expected.c      # 预期的 C 输出
│   └── hello.expected.h      # 预期的 H 输出
├── 001_sqrt/
│   ├── sqrt.at
│   ├── sqrt.expected.c
│   └── sqrt.expected.h
```

**测试执行流程**：
1. 读取目录下的输入文件（`.at`）
2. 调用 a2c 解释器生成 C 和 H 文件内容（内存中，不写磁盘）
3. 与目录中的预期文件进行对比
4. 如果不同，测试失败并将实际生成的内容写入 `.wrong.c` / `.wrong.h` 文件

### Auto-Man 的测试需求

auto-man 作为工程管理软件，其测试需要扩展"文件测试用例"为"文件夹测试用例"：

**输入**：整个 auto-man 工程
- `pac.at` 配置文件
- 工程内的源代码文件

**输出**：根据不同 port 配置生成的构建配置文件
- **IAR**: `.eww`, `.ewp`, `.ewt`, `.ewd`（4个文件）
- **Ninja**: `build.ninja`
- **CMake**: `CMakeLists.txt`
- **GHS**: `.gpj` 文件

**测试方案**：
- 测试用例文件夹包含完整的工程结构
- 预期输出文件加上 `.expected` 后缀标识
- 测试执行器生成实际构建配置，与预期文件对比

---

## 测试用例目录结构

### 根目录组织

```
d:/autostack/auto-man/
└── crates/auto-man/
    └── tests/
        └── integration/              # 集成测试用例（新增）
            ├── 001_simple_app/       # 简单应用程序
            ├── 002_with_lib/         # 包含库的项目
            ├── 003_multi_target/     # 多目标项目
            ├── 004_with_deps/        # 带依赖的项目
            ├── 005_iar_project/      # IAR builder 测试
            └── 006_ninja_project/    # Ninja builder 测试
```

**设计原则**：
- 使用 `integration/` 子目录与现有单元测试区分
- 数字前缀 + 下划线描述的命名格式（NNN_description）
- 与 a2c 测试命名保持一致

### 单个测试用例结构

每个测试用例是一个完整的 auto-man 工程：

```
tests/integration/001_simple_app/
├── pac.at                          # 项目配置（必需）
├── source/                         # 源文件
│   └── main.c                      # C 源文件示例
└── expected/                       # 预期输出（按 builder 组织）
    ├── cmake/
    │   └── CMakeLists.txt          # CMake 预期输出
    ├── iar/
    │   ├── simple_app.eww         # IAR 工作区
    │   ├── simple_app.ewp         # IAR 项目
    │   ├── simple_app.ewt         # IAR 设置
    │   └── simple_app.ewd         # IAR 调试器
    ├── ghs/
    │   └── default.gpj            # GHS 预期输出
    └── ninja/
        └── build.ninja            # Ninja 预期输出
```

**关键设计决策**：

1. **嵌套 `expected/` 结构**：不同 builder 的输出文件数量和类型差异较大（CMake 1个，IAR 4个），使用独立目录组织更清晰
2. **按需包含 builder**：不是所有测试都需要测试所有 builder，缺少 `expected/builder/` 表示跳过该 builder
3. **工程可独立构建**：每个测试用例都是有效的 auto-man 工程，可手动构建验证

---

## 核心实现设计

### 1. 测试执行器架构

**文件位置**：`crates/auto-man/tests/integration_test.rs`

```rust
use std::fs;
use std::path::PathBuf;
use auto_man::{Automan, AmConfig};

#[test]
fn test_integration_all_cases() {
    let test_cases = discover_test_cases();

    for case in test_cases {
        println!("\n=== Testing: {} ===", case.name);

        let builders = case.get_supported_builders();

        for builder_name in builders {
            println!("  Testing builder: {}", builder_name);

            if let Err(e) = run_test_case(&case, &builder_name) {
                println!("  ✗ {} builder failed: {}", builder_name, e);
                generate_wrong_files(&case, &builder_name);
                panic!("Test failed for {} with builder {}", case.name, builder_name);
            } else {
                println!("  ✓ {} builder passed", builder_name);
            }
        }
    }
}

struct TestCase {
    name: String,
    path: PathBuf,
}

fn discover_test_cases() -> Vec<TestCase> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_dir = manifest_dir.join("tests/integration");
    let mut cases = Vec::new();

    for entry in fs::read_dir(test_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.is_dir() && path.join("pac.at").exists() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            cases.push(TestCase { name, path });
        }
    }

    cases.sort_by_key(|c| c.name.clone());
    cases
}

fn run_test_case(case: &TestCase, builder_name: &str) -> AutoResult<()> {
    // 1. 复制测试用例到临时目录（隔离）
    let temp_dir = setup_temp_project(&case.path)?;

    // 2. 解析项目并设置 builder
    let mut automan = Automan::parse_pac(&temp_dir, &AmConfig::default())?;
    automan.set_port(builder_name.into())?;

    // 3. 在内存中生成构建配置
    let generated_files = generate_to_memory(&mut automan)?;

    // 4. 加载预期文件
    let expected_files = load_expected_files(&case.path, builder_name)?;

    // 5. 比较生成内容与预期文件
    compare_outputs(&generated_files, &expected_files)?;

    // 6. 清理临时目录
    fs::remove_dir_all(temp_dir)?;

    Ok(())
}
```

### 2. Builder 内存输出支持

**核心创新**：让 builder 在内存中生成输出，而非写入磁盘

#### 2.1 扩展 Builder trait

**文件**：`crates/auto-man/src/builder.rs`

```rust
pub trait Builder {
    fn build(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn setup(&mut self, pac: &mut Pac) -> AutoResult<()>;
    fn finish(&mut self, pac: &Pac) -> AutoResult<()>;
    fn target(&mut self, target: &Target, pac: &Pac) -> AutoResult<()>;
    fn clean(&mut self) -> AutoResult<()>;
    fn run(&mut self, pac: &Pac, args: Vec<String>) -> AutoResult<()>;

    // 新增：启用内存输出模式（测试用）
    fn enable_memory_output(&mut self) -> AutoResult<()>;
    fn get_memory_output(&self) -> HashMap<String, Vec<u8>>;
}
```

#### 2.2 CMakeBuilder 实现（参考实现）

**文件**：`crates/auto-man/src/builder/cmake.rs`

```rust
use std::io::Cursor;
use std::collections::HashMap;

pub struct CMakeBuilder {
    pub cmake_path: String,
    pub out: Box<dyn Write>,
    memory_mode: bool,           // 新增
    memory_output: Vec<u8>,      // 新增
}

impl CMakeBuilder {
    pub fn new(path: &str) -> Self {
        Self {
            cmake_path: path.to_string(),
            out: Box::new(std::io::sink()),
            memory_mode: false,
            memory_output: Vec::new(),
        }
    }

    pub fn enable_memory_output(&mut self) {
        self.memory_mode = true;
        self.memory_output = Vec::new();
        self.out = Box::new(Cursor::new(&mut self.memory_output));
    }

    pub fn get_memory_output(&self) -> HashMap<String, Vec<u8>> {
        let mut map = HashMap::new();
        if self.memory_mode {
            map.insert("CMakeLists.txt".to_string(), self.memory_output.clone());
        }
        map
    }
}

impl Builder for CMakeBuilder {
    // ... 其他方法保持不变 ...

    fn enable_memory_output(&mut self) -> AutoResult<()> {
        self.enable_memory_output();
        Ok(())
    }

    fn get_memory_output(&self) -> HashMap<String, Vec<u8>> {
        self.get_memory_output()
    }
}
```

#### 2.3 IARBuilder 实现（多文件输出）

**文件**：`crates/auto-man/src/builder/iar.rs`

```rust
pub struct IARBuilder {
    pub gen: AutoGen,
    pub app_gens: Vec<AutoGen>,
    pub path: AutoPath,
    memory_mode: bool,
    memory_outputs: HashMap<String, Vec<u8>>,  // 文件名 -> 内容
}

impl IARBuilder {
    pub fn enable_memory_output(&mut self) {
        self.memory_mode = true;
        self.memory_outputs = HashMap::new();
        // Hook AutoGen 输出到内存
    }

    pub fn get_memory_output(&self) -> HashMap<String, Vec<u8>> {
        self.memory_outputs.clone()
    }
}

impl Builder for IARBuilder {
    fn enable_memory_output(&mut self) -> AutoResult<()> {
        self.enable_memory_output();
        Ok(())
    }

    fn get_memory_output(&self) -> HashMap<String, Vec<u8>> {
        self.get_memory_output()
    }
}
```

### 3. 文件对比和错误报告

```rust
fn compare_outputs(
    generated: &HashMap<String, Vec<u8>>,
    expected: &HashMap<String, Vec<u8>>
) -> AutoResult<()> {
    for (filename, expected_content) in expected {
        let generated_content = generated.get(filename)
            .ok_or_else(|| format!("Missing generated file: {}", filename))?;

        if generated_content != expected_content {
            return Err(format!(
                "File {} mismatch:\nExpected: {} bytes\nGenerated: {} bytes",
                filename,
                expected_content.len(),
                generated_content.len()
            ).into());
        }
    }

    // 检查是否有未预期的生成文件
    for filename in generated.keys() {
        if !expected.contains_key(filename) {
            return Err(format!("Unexpected generated file: {}", filename).into());
        }
    }

    Ok(())
}

fn generate_wrong_files(case: &TestCase, builder_name: &str) {
    let wrong_dir = case.path.join("wrong").join(builder_name);
    fs::create_dir_all(&wrong_dir).unwrap();

    // 将生成的文件写入 wrong/ 目录
    // ...
}
```

---

## 实现步骤

### ✅ 阶段 1：基础设施（第1-2周）- **已完成**

**任务 1.1**：创建测试目录结构
- [x] 创建 `tests/integration/` 目录
- [x] 创建 `tests/integration_test.rs` 文件
- [x] 实现 `discover_test_cases()` 函数
- [x] 创建 `tests/integration/README.md` 文档

**任务 1.2**：实现第一个测试用例（001_simple_app）
- [x] 创建 `001_simple_app/pac.at`
- [x] 创建 `001_simple_app/source/main.c`
- [x] 创建 `001_simple_app/expected/cmake/CMakeLists.txt`

**任务 1.3**：基础测试执行器
- [x] 实现 `run_test_case()`（仅 CMake）
- [x] 使用临时目录方式（暂不实现内存输出）
- [x] 实现文件对比逻辑 `compare_outputs()`
- [x] 实现预期文件加载 `load_expected_files()`

**关键文件**：
- [x] `crates/auto-man/tests/integration_test.rs` - 新建
- [x] `crates/auto-man/tests/integration/README.md` - 新建
- [x] `crates/auto-man/tests/integration/001_simple_app/` - 新建

**完成日期**：2025-01-16

**完成摘要**：
- ✅ 测试目录结构创建完成
- ✅ 测试执行器基础功能实现
- ✅ 第一个测试用例（001_simple_app）添加
- ✅ 测试通过验证（`cargo test --test integration_test`）

**测试结果**：
```
running 1 test
test test_integration_all_cases ... ok

test result: ok. 1 passed; 0 failed
```

### 阶段 2：内存输出支持（第2-3周）

**任务 2.1**：修改 Builder trait
- [ ] 添加 `enable_memory_output()` 方法
- [ ] 添加 `get_memory_output()` 方法

**任务 2.2**：实现 CMakeBuilder 内存输出
- [ ] 修改 `src/builder/cmake.rs`
- [ ] 在内存模式使用 `Cursor<Vec<u8>>` 替代文件输出
- [ ] 使用 001_simple_app 验证

**任务 2.3**：实现 NinjaBuilder 内存输出
- [ ] 修改 `src/builder/ninja.rs`
- [ ] 遵循 CMake 模式

**关键文件**：
- `crates/auto-man/src/builder.rs` - 修改（trait）
- `crates/auto-man/src/builder/cmake.rs` - 修改
- `crates/auto-man/src/builder/ninja.rs` - 修改

### 阶段 3：多文件 Builder（第3-4周）

**任务 3.1**：实现 IARBuilder 内存输出
- [ ] 修改 `src/builder/iar.rs`
- [ ] Hook AutoGen 输出机制
- [ ] 处理 4 文件输出（.eww, .ewp, .ewt, .ewd）

**任务 3.2**：实现 GHSBuilder 内存输出
- [ ] 修改 `src/builder/ghs.rs`
- [ ] 处理 .gpj 文件生成

**任务 3.3**：更新测试执行器
- [ ] 支持多个预期文件的对比
- [ ] 实现基于目录的对比

**关键文件**：
- `crates/auto-man/src/builder/iar.rs` - 修改
- `crates/auto-man/src/builder/ghs.rs` - 修改
- `crates/auto-man/tests/integration_test.rs` - 修改

### 阶段 4：测试用例扩展（第4-5周）

**任务 4.1**：添加功能测试用例
- [ ] 002_with_lib - 库和链接
- [ ] 003_multi_target - 多个 app 和 lib
- [ ] 004_with_deps - 外部依赖
- [ ] 005_iar_project - IAR 专用测试
- [ ] 006_ninja_project - Ninja 专用测试

**任务 4.2**：添加 builder 特定配置
- [ ] 创建 `expected/iar/`, `expected/ghs/`, `expected/ninja/` 目录
- [ ] 生成并验证预期文件

**关键文件**：
- `tests/integration/002_with_lib/` - 新建
- `tests/integration/003_multi_target/` - 新建
- `tests/integration/004_with_deps/` - 新建

### 阶段 5：高级特性（第5-6周）

**任务 5.1**：依赖测试
- [ ] Mock git 仓库
- [ ] 测试 `dep()` 目标解析
- [ ] 测试递归依赖

**任务 5.2**：错误用例测试
- [ ] 测试无效 pac.at
- [ ] 测试缺失依赖
- [ ] 测试循环依赖

**任务 5.3**：Wrong 文件生成
- [ ] 实现 `.wrong` 文件输出
- [ ] 添加测试用例文档生成

**关键文件**：
- `tests/integration/fixtures/` - 新建（mock git 仓库）
- `tests/integration/007_invalid_config/` - 新建

### 阶段 6：CI/CD 集成（第6周）

**任务 6.1**：GitHub Actions 工作流
- [ ] 创建 `.github/workflows/integration-tests.yml`
- [ ] 在 Windows 和 Linux 上运行测试
- [ ] 测试多个 builder

**任务 6.2**：测试文档
- [ ] 添加 `TESTING.md` 到项目
- [ ] 记录如何添加新测试用例
- [ ] 记录测试用例命名约定

**关键文件**：
- `.github/workflows/integration-tests.yml` - 新建
- `TESTING.md` - 新建

---

## 测试用例示例

### 示例 1：001_simple_app（最简单）

**pac.at**：
```rust
name: "simple_app"
version: "0.0.1"

port("windows_cmake") {
    platform: "windows"
    builder: "cmake"
    toolchain: "msvc"
}

app("simple_app") {
    dir("source") {}
}
```

**source/main.c**：
```c
#include <stdio.h>

int main() {
    printf("Hello from auto-man!\n");
    return 0;
}
```

**expected/cmake/CMakeLists.txt**：
```cmake
cmake_minimum_required(VERSION 3.15)
project(simple_app C)

set(CMAKE_C_STANDARD 11)

add_executable(simple_app
    source/main.c
)
```

### 示例 2：002_with_lib（库和链接）

**pac.at**：
```rust
name: "with_lib"
version: "0.0.1"

port("windows_cmake") {
    platform: "windows"
    builder: "cmake"
    toolchain: "msvc"
}

lib("math") {
    dir("math_lib") {}
}

app("calculator") {
    link("math") {}
    dir("calc_app") {}
}
```

### 示例 3：003_iar_project（IAR 多文件输出）

**pac.at**：
```rust
name: "iar_test"
version: "0.0.1"

port("stm32_iar") {
    platform: "windows"
    builder: "iar"
    toolchain: "iar"
}

device("stm32f407") {
    icf: "stm32f407.icf"
    ddf: "stm32f407.ddf"
    board: "stm32f407.board"
}

app("firmware") {
    link("stm32f407") {}
    dir("src") {}
}
```

**expected/iar/**：
```
├── firmware.eww
├── firmware.ewp
├── firmware.ewt
└── firmware.ewd
```

---

## 技术挑战与解决方案

### 挑战 1：AutoGen 输出重定向

**问题**：`AutoGen` 从 `auto-gen` crate 直接写入磁盘。

**解决方案**：
1. 检查 `AutoGen` 是否支持自定义输出 sink
2. 如果不支持，添加 `set_output()` 方法（与 auto-lang 团队协调）
3. 后备方案：写入临时目录然后读回内存

### 挑战 2：平台特定 Builder

**问题**：IAR 和 GHS 仅支持 Windows，无法在 Linux CI 上测试。

**解决方案**：
```rust
fn get_supported_builders(case: &TestCase) -> Vec<String> {
    let mut builders = vec!["cmake".to_string(), "ninja".to_string()];

    #[cfg(windows)]
    {
        builders.push("iar".to_string());
        builders.push("ghs".to_string());
    }

    // 过滤出测试用例提供的 builder
    builders.retain(|b| case.has_expected_files(b));
    builders
}
```

### 挑战 3：外部依赖

**问题**：带 `dep()` 目标的测试用例需要 git clone。

**解决方案**：
1. **Mock 依赖**：在 `tests/fixtures/` 中预打包测试依赖
2. **本地 file:// 依赖**：使用本地 git 仓库而非远程 URL
3. **无依赖测试**：初始阶段大部分测试避免外部依赖

**推荐**：从无依赖测试开始，后续添加 mock 依赖。

### 挑战 4：测试隔离

**问题**：测试可能互相干扰构建输出。

**解决方案**：
```rust
fn setup_temp_project(original: &PathBuf) -> AutoResult<String> {
    let temp_dir = std::env::temp_dir();
    let unique_name = format!("auto-man-test-{}", uuid::Uuid::new_v4());
    let test_dir = temp_dir.join(unique_name);

    // 复制整个测试用例目录
    fs_extra::dir::copy(original, &test_dir, &Default::default())?;

    Ok(test_dir.to_string_lossy().to_string())
}
```

### 挑战 5：预期文件生成

**问题**：如何初始创建预期文件？

**解决方案**：
```rust
#[test]
#[ignore]  // 仅手动运行以生成预期文件
fn generate_expected_files() {
    let test_cases = discover_test_cases();

    for case in test_cases {
        let temp_dir = setup_temp_project(&case.path)?;
        let mut automan = Automan::new(&temp_dir, AmConfig::default())?;
        automan.build()?;

        // 将生成的文件复制到 expected/
        copy_generated_to_expected(&temp_dir, &case.path)?;
    }
}
```

---

## 关键文件清单

### 新建文件

1. **`crates/auto-man/tests/integration_test.rs`**
   - 主测试执行器
   - 测试发现逻辑
   - 内存输出编排

2. **`crates/auto-man/tests/integration/001_simple_app/pac.at`**
   - 第一个测试用例配置

3. **`crates/auto-man/tests/integration/001_simple_app/expected/cmake/CMakeLists.txt`**
   - 预期 CMake 输出

4. **`TESTING.md`**
   - 测试文档

5. **`.github/workflows/integration-tests.yml`**
   - CI/CD 工作流

### 修改文件

1. **`crates/auto-man/src/builder.rs`**
   - 扩展 Builder trait：`enable_memory_output()`, `get_memory_output()`

2. **`crates/auto-man/src/builder/cmake.rs`**
   - 实现内存输出模式（参考实现）

3. **`crates/auto-man/src/builder/ninja.rs`**
   - 实现内存输出模式

4. **`crates/auto-man/src/builder/iar.rs`**
   - 实现多文件内存输出

5. **`crates/auto-man/src/builder/ghs.rs`**
   - 实现 .gpj 文件内存输出

6. **`crates/auto-man/Cargo.toml`**
   - 添加测试依赖（如需要）：`uuid`, `fs_extra`

---

## 成功指标

### 覆盖率目标

**最低要求**：10 个集成测试用例，覆盖：
- 4 种 builder 类型（CMake, IAR, GHS, Ninja）
- 3 种目标类型（app, lib, dep）
- 2 个依赖场景（无依赖、有依赖）
- 1 个多目标项目

**目标**：20+ 测试用例，覆盖边缘情况：
- 无效配置
- 缺失依赖
- 循环依赖
- 复杂依赖图
- 平台特定功能

### 执行时间

- **目标**：所有测试在 30 秒内完成
- **预估**：每个测试用例约 2-5 秒
- **优化**：并行执行、内存输出

---

## 验证步骤

### 运行所有集成测试

```bash
cargo test --test integration_test
```

### 运行特定测试用例

```bash
cargo test --test integration_test -- 001_simple_app
```

### 运行特定 builder

```bash
cargo test --test integration_test -- cmake
```

### 生成预期文件（首次）

```bash
cargo test --test integration_test -- generate_expected_files -- --ignored
```

---

## 时间线

| 阶段 | 周次 | 产出 |
|------|------|------|
| 阶段 1 | 1-2 | 基础设施、001_simple_app |
| 阶段 2 | 2-3 | Builder trait 扩展、CMake/Ninja 内存输出 |
| 阶段 3 | 3-4 | IAR/GHS 多文件输出 |
| 阶段 4 | 4-5 | 测试用例扩展（002-006） |
| 阶段 5 | 5-6 | 高级特性（依赖、错误用例） |
| 阶段 6 | 6 | CI/CD 集成、文档完善 |

---

## 实施进度追踪

### 2025-01-16

**已完成**：阶段 1 - 基础设施

**创建的文件**：
- [x] `tests/integration_test.rs` - 测试执行器（207行）
- [x] `tests/integration/README.md` - 测试文档
- [x] `tests/integration/001_simple_app/pac.at` - 测试用例配置
- [x] `tests/integration/001_simple_app/source/main.c` - 源文件
- [x] `tests/integration/001_simple_app/expected/cmake/CMakeLists.txt` - 预期输出

**实现的功能**：
- ✅ 自动发现测试用例（`discover_test_cases()`）
- ✅ 支持多 builder 测试（cmake, iar, ghs, ninja）
- ✅ 加载预期输出文件（`load_expected_files()`）
- ✅ 文件内容对比逻辑（`compare_outputs()`）
- ✅ 测试结果统计和报告

**测试结果**：
```
$ cargo test --test integration_test
running 1 test
test test_integration_all_cases ... ok

test result: ok. 1 passed; 0 failed
```

**下一步**：阶段 2 - 实现内存输出支持

