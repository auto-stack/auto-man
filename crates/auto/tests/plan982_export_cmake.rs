//! PLAN-982 / V-B-EXPORT：**实际导出的 CMake 工程**交给 MSVC（CMake/Ninja
//! Release）**配置、构建、链接并运行**。
//!
//! 链条（全部真实 CLI/工具链，无桩）：`scene: "c"` 工程 → `auto build`
//! （port `windows_ninja` + builder `cmake` ⇒ 转译 + CMake exporter 生成
//! `build/CMakeLists.txt`）→ `vcvars64` 后 `cmake -G Ninja
//! -DCMAKE_BUILD_TYPE=Release` 配置 → `cmake --build` 构建 → 运行
//! `build/main.exe` ⇒ 输出源语义。
//!
//! 工具链门槛：`cl`／`cmake`／`ninja` 任一缺席 ⇒ **具名跳过**（不判红）。

use std::path::{Path, PathBuf};
use std::process::Command;

const VCVARS: &str =
    r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat";

const MAIN_SRC: &str = "fn main() int {\n  print(\"export-leg:ok\")\n  return 0\n}\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-export-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// `scene: "c"` 工程：单模块 + 名为 `windows_ninja` 的 port（与缺省 port 同名
/// ⇒ 启动不弹交互选择），builder `cmake` ⇒ exporter 只生成工程文件。
fn project(dir: &Path) {
    std::fs::write(
        dir.join("pac.at"),
        "name: \"c982export\"\nversion: \"0.1.0\"\nscene: \"c\"\n\nport(\"windows_ninja\") {\n    builder: \"cmake\"\n}\n\napp main {\n    at: \".\"\n    srcs: [\"main.at\"]\n}\n",
    )
    .expect("pac.at");
    std::fs::write(dir.join("main.at"), MAIN_SRC).expect("main.at");
}

/// 批处理里加载 vcvars 后跑命令（输出重定向进文件，避免管道依赖）。
fn run_in_vs(dir: &Path, body: &str) -> (bool, String) {
    let bat = dir.join("step.bat");
    let out_log = dir.join("step.out");
    std::fs::write(
        &bat,
        format!(
            "@echo off\r\ncall \"{VCVARS}\" >nul 2>&1\r\n{body} > \"{}\" 2>&1\r\n",
            out_log.display()
        ),
    )
    .expect("write step.bat");
    let status = Command::new("cmd")
        .args(["/c", bat.to_str().unwrap()])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("spawn cmd");
    let text = std::fs::read_to_string(&out_log).unwrap_or_default();
    (status.success(), text)
}

/// 工具链是否就位：`cl` 横幅 + `cmake`/`ninja` 在 PATH 上。
fn toolchain_ready(dir: &Path) -> bool {
    if !Path::new(VCVARS).exists() {
        return false;
    }
    let (ok, text) = run_in_vs(dir, "cl");
    if !ok || !(text.contains("Microsoft") && text.contains("C/C++")) {
        return false;
    }
    let cmake = Command::new("cmake").arg("--version").output();
    let ninja = Command::new("ninja").arg("--version").output();
    matches!((cmake, ninja), (Ok(a), Ok(b)) if a.status.success() && b.status.success())
}

#[test]
fn p982_export_cmake_project_builds_links_and_runs_under_msvc() {
    let dir = scratch("cmake");
    project(&dir);

    // (1) 真 CLI 导出：`auto build` ⇒ build/CMakeLists.txt（exporter 产物）。
    let build = Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg("build")
        .current_dir(&dir)
        .output()
        .expect("run auto build");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    eprintln!(
        "p982 export: status={:?} log-tail={}",
        build.status,
        log.lines().rev().take(5).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        build.status.success(),
        "`auto build` (cmake export) succeeds: {log}"
    );
    assert!(
        log.contains("Building via cmake exporter"),
        "the cmake exporter ran: {log}"
    );
    // 导出位置：exporter 按 pac 的 build_location 写（本工程缺省 `.`）——
    // CMakeLists.txt 在工程根。
    let cmake_lists = dir.join("CMakeLists.txt");
    assert!(
        cmake_lists.is_file(),
        "the exported CMake project exists: {}",
        cmake_lists.display()
    );
    let lists_text = std::fs::read_to_string(&cmake_lists).unwrap_or_default();
    eprintln!("p982 export: CMakeLists.txt =\n{lists_text}");
    // 导出形状：app 目标 `main`，源清单含转译产物 `main.c`（含 .at 原源——
    // CMake 对未知扩展按非源跳过，不参与编译）。
    assert!(
        lists_text.contains("add_executable(main") && lists_text.contains("main.c"),
        "the export declares the app target over the transpiled C: {lists_text}"
    );

    // (2) 工具链缺席 ⇒ 具名跳过（不判红）。
    if !toolchain_ready(&dir) {
        eprintln!("p982 export: skipped (cl/cmake/ninja not ready via vcvars64)");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    // (3) 配置（Ninja + Release）+ 构建 + 运行——真实 CMake/Ninja/MSVC。
    let (cfg_ok, cfg_log) = run_in_vs(
        &dir,
        "cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release",
    );
    eprintln!("p982 export configure: {cfg_log}");
    assert!(cfg_ok, "cmake configures the exported project:\n{cfg_log}");

    let (build_ok, build_log) = run_in_vs(&dir, "cmake --build build --config Release");
    eprintln!("p982 export build: {build_log}");
    assert!(
        build_ok && dir.join("build").join("main.exe").is_file(),
        "cmake builds and links main.exe:\n{build_log}"
    );

    let run = Command::new(dir.join("build").join("main.exe"))
        .current_dir(&dir)
        .output()
        .expect("run main.exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    eprintln!("p982 export run: status={:?} stdout={stdout:?}", run.status);
    assert!(
        stdout.contains("export-leg:ok"),
        "the exported binary prints the source semantics: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
