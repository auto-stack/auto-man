//! PLAN-982（排程令第 2 项，V-B-C/V-B-EXPORT，round 201）：**新版 A2C 的
//! CMake/Ninja 导出**——含**标准库**的多模块工程（`use std.math.square`＋跨
//! 模块 `util.add`，oracle＝square(add(2,3))=25）分别走两条腿：
//!
//! 1. **CMake 导出后构建**：`auto build`（builder `cmake` ⇒ exporter 只生成
//!    `CMakeLists.txt`，不调编译器）⇒ **仓外空构建根** `build/` 里
//!    CMake(Ninja, Release, MSVC) 配置、构建、链接 ⇒ 运行 ⇒ `value:25`；
//! 2. **AutoMan 自有 `build.ninja`**：`auto build`（builder `ninja` ⇒ 内建
//!    ninja builder 生成 `build.ninja` 并**在 MSVC 环境里自行调用 ninja**）⇒
//!    运行产物 ⇒ `value:25`；并钉住 round-201 的源前缀修复（根布局
//!    `cc main.c`，无 `../`——修复前 ninja 报源缺失）。
//!
//! 两腿都证 `CTrans=0`（`AUTO_ACE_C_PROOF`：`engine=a2c`、`a2c_dispatches=1`、
//! `legacy_dispatches=0`）。工具链门槛（DevShell/cl/cmake/ninja 任一缺席）⇒
//! **具名跳过**（不判红）。IAR/GHS 已延期。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::process::Command;

const VSSHELL: &str = r"C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Microsoft.VisualStudio.DevShell.dll";

const UTIL: &str = "pub fn add(a int, b int) int {\n    return a + b\n}\n";
const MAIN: &str = "use std.math.square\nuse util\nfn main() int {\n    var r int = util.add(2, 3)\n    return square(r)\n}\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-a2c-exp-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn project(dir: &Path, builder: &str) {
    std::fs::write(
        dir.join("pac.at"),
        format!(
            "name: \"a2cexp\"\nversion: \"0.1.0\"\nscene: \"c\"\n\nport(\"windows_ninja\") {{\n    builder: \"{builder}\"\n    platform: \"windows\"\n    at: \".\"\n}}\n\napp main {{\n    at: \".\"\n    srcs: [\"main.at\", \"util.at\"]\n}}\n"
        ),
    )
    .expect("pac.at");
    std::fs::write(dir.join("main.at"), MAIN).expect("main.at");
    std::fs::write(dir.join("util.at"), UTIL).expect("util.at");
}

/// DevShell 内跑一段 PowerShell（工具链环境＝计划在案的 DevShell 配方）。
fn in_vs_shell(dir: &Path, script: &str) -> (bool, String) {
    let full = format!(
        "Import-Module '{VSSHELL}' -ErrorAction Stop; \
         Enter-VsDevShell a35e5bdb -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' -ErrorAction Stop | Out-Null; \
         {script}"
    );
    let out = Command::new("pwsh")
        .args(["-NoProfile", "-Command", &full])
        .current_dir(dir)
        .output()
        .expect("spawn pwsh");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// 工具链就位：DevShell＋cl＋cmake＋ninja。
fn toolchain_ready(dir: &Path) -> bool {
    if !Path::new(VSSHELL).exists() {
        return false;
    }
    let (ok, text) = in_vs_shell(dir, "where.exe cl; cmake --version; ninja --version");
    ok && text.contains("cl.exe") && text.contains("cmake version") && text.contains("1.")
}

/// 断言 A2C fileset＋回执（两腿共用；ninja 腿在 DevShell build 之后调用）。
fn assert_a2c_fileset_and_receipt(dir: &Path, proof: &Path) {
    assert!(
        dir.join("main.c").is_file() && dir.join("generated.c").is_file(),
        "the A2C fileset lands"
    );
    assert!(
        dir.join("a2x_c_rt_runtime.h").is_file(),
        "the A2C runtime header"
    );
    assert!(
        !dir.join("util.c").is_file(),
        "the legacy per-file util.c is NOT emitted"
    );
    let receipt = std::fs::read_to_string(proof).expect("proof written");
    let receipt: serde_json::Value = serde_json::from_str(&receipt).expect("proof json");
    eprintln!("p982 a2c-export proof: {receipt:#?}");
    assert_eq!(receipt["engine"], "a2c", "the engine is named: {receipt}");
    assert_eq!(
        receipt["a2c_dispatches"], 1,
        "one package dispatch serves the whole project: {receipt}"
    );
    assert_eq!(
        receipt["legacy_dispatches"], 0,
        "the legacy CTrans never dispatched: {receipt}"
    );
}

/// (1) CMake 导出后构建：仓外空构建根配置、构建、链接、运行。
#[test]
fn p982_a2c_export_cmake_builds_and_runs_from_clean_root() {
    let dir = scratch("cmake");
    project(&dir, "cmake");
    if !toolchain_ready(&dir) {
        eprintln!("p982 a2c-export cmake: SKIPPED (no DevShell/cl/cmake/ninja)");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    // builder `cmake` ⇒ exporter 只生成工程文件（不调编译器）⇒ 可在普通
    // 环境里跑 `auto build`。
    let proof = dir.join("proof.json");
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg("build")
        .current_dir(&dir)
        .env("AUTO_ACE_C", "1")
        .env("AUTO_ACE_C_PROOF", proof.as_os_str())
        .env_remove("AUTO_ACE_CANDIDATE")
        .output()
        .expect("run auto build");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "`auto build` with AUTO_ACE_C=1 succeeds: {log}"
    );
    assert_a2c_fileset_and_receipt(&dir, &proof);

    let cmake_lists = std::fs::read_to_string(dir.join("CMakeLists.txt")).expect("CMakeLists");
    assert!(
        cmake_lists.contains("main.c") && cmake_lists.contains("generated.c"),
        "the exported closure carries the A2C fileset:\n{cmake_lists}"
    );

    // 仓外空构建根：全新 build/ 里配置→构建→运行（DevShell 提供 MSVC）。
    let (ok, vs_log) = in_vs_shell(
        &dir,
        "mkdir build -Force | Out-Null; cd build; \
         cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_TRY_COMPILE_CONFIGURATION=Release .. ; \
         if ($LASTEXITCODE -ne 0) { exit 1 } \
         cmake --build . ; if ($LASTEXITCODE -ne 0) { exit 1 } \
         ./main.exe",
    );
    eprintln!(
        "p982 a2c-export cmake leg: ok={ok} log-tail={}",
        vs_log.lines().rev().take(5).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        ok,
        "CMake configure+build+run from the clean root:\n{vs_log}"
    );
    assert!(
        vs_log.contains("value:25"),
        "the exported program runs the stdlib+cross-module semantics: {vs_log}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// (2) 自有 build.ninja：`auto build` 在 MSVC 环境里生成并**自行调用 ninja**，
/// 运行产物；并钉住 round-201 源前缀修复（根布局无 `../`）。
#[test]
fn p982_a2c_own_ninja_builds_and_runs() {
    let dir = scratch("ninja");
    project(&dir, "ninja");
    if !toolchain_ready(&dir) {
        eprintln!("p982 a2c-export ninja: SKIPPED (no DevShell/cl/cmake/ninja)");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    // builder `ninja` 在 build 内自行调 ninja（需要 cl）——整腿在 DevShell 里
    // 跑：build → 运行 link 产物 `main.exe`（platform=windows 命名——内联 port 声明不从索引继承 platform，缺它时 link 产物无扩展名、Windows 下不可执行）。
    let exe = env!("CARGO_BIN_EXE_auto").replace('\\', "/");
    let proof = dir.join("proof.json");
    let script = format!(
        "$env:AUTO_ACE_C='1'; $env:AUTO_ACE_C_PROOF='{}'; & '{exe}' build; \
         if ($LASTEXITCODE -ne 0) {{ exit 1 }} \
         & .\\main.exe",
        proof.display()
    );
    let (ok, vs_log) = in_vs_shell(&dir, &script);
    eprintln!(
        "p982 a2c-export ninja leg: ok={ok} log-tail={}",
        vs_log.lines().rev().take(5).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        ok,
        "the own-ninja lane builds and runs inside the MSVC shell:\n{vs_log}"
    );
    assert!(
        vs_log.contains("value:25"),
        "the ninja-built program runs the stdlib+cross-module semantics: {vs_log}"
    );
    assert_a2c_fileset_and_receipt(&dir, &proof);

    // 前缀钉面：根布局（build_location "."）下源引用不带 `../`。
    let ninja_file = std::fs::read_to_string(dir.join("build.ninja")).expect("build.ninja");
    assert!(
        ninja_file.contains("cc main.c"),
        "the root layout references sources without ../ (round-201 fix):\n{ninja_file}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
