//! PLAN-982（982/C，宿主 A2C 渲染器 · 真 CLI 验收）：跨模块样例经
//! `auto build` 的 **C 目标**走**包级 A2C**（`AUTO_ACE_C=1` 显式选择）：
//!
//! 1. **正例**：`scene: "c"` 工程两模块（`use util` + `util.add(2, 3)`）⇒
//!    同一 fileset（`main.c` + `generated.c`，**没有** legacy 的 `util.c`）；
//!    回执 `a2c_dispatches=1`、`legacy_dispatches=0`；再由 **MSVC 真编译**并
//!    运行，输出 `value:5`（跨模块调用的真实结果）。
//! 2. **记账对照**：同一工程**不选** ACE ⇒ legacy 腿如实计数
//!    （`engine:"legacy"`、`legacy_dispatches≥1`、`a2c_dispatches=0`）。
//! 3. **缺席具名失败**：显式选 ACE 而渲染器缺席（本测试进程未安装）⇒
//!    `ace.c/renderer-unavailable`，**绝不静默回退**旧 `CTrans`。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VCVARS: &str =
    r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat";

const UTIL_SRC: &str = "pub fn add(a int, b int) int {\n  return a + b\n}\n";
const MAIN_SRC: &str = "use util\nfn main() int {\n  var r int = util.add(2, 3)\n  return r\n}\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-c-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// `scene: "c"` 工程：两模块 + 名为 `windows_ninja` 的 port（与缺省 port 同名
/// ⇒ 启动时不弹交互选择），builder `cmake` ⇒ 走 exporter 只生成工程文件，
/// 不在转译后立即调编译器（编译由本测试的 MSVC 腿显式做）。
fn project(dir: &Path) {
    std::fs::write(
        dir.join("pac.at"),
        "name: \"c982\"\nversion: \"0.1.0\"\nscene: \"c\"\n\nport(\"windows_ninja\") {\n    builder: \"cmake\"\n}\n\napp main {\n    at: \".\"\n    srcs: [\"main.at\", \"util.at\"]\n}\n",
    )
    .expect("pac.at");
    std::fs::write(dir.join("util.at"), UTIL_SRC).expect("util.at");
    std::fs::write(dir.join("main.at"), MAIN_SRC).expect("main.at");
}

fn build_cli(dir: &Path, ace: bool, proof: &Path) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.arg("build")
        .current_dir(dir)
        .env("AUTO_ACE_C_PROOF", proof);
    if ace {
        cmd.env("AUTO_ACE_C", "1");
    } else {
        cmd.env_remove("AUTO_ACE_C");
    }
    let out = cmd.output().expect("run auto build");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

fn proof_json(path: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read proof: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse proof {text:?}: {e}"))
}

/// MSVC 是否可用（`cl` 横幅）。
fn cl_available(dir: &Path) -> bool {
    if !Path::new(VCVARS).exists() {
        return false;
    }
    let bat = dir.join("probe.bat");
    let out_log = dir.join("probe.out");
    std::fs::write(
        &bat,
        format!(
            "@echo off\r\ncall \"{VCVARS}\" >nul 2>&1\r\ncl > \"{}\" 2>&1\r\n",
            out_log.display()
        ),
    )
    .expect("write probe.bat");
    let _ = Command::new("cmd")
        .args(["/c", bat.to_str().unwrap()])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let text = std::fs::read_to_string(&out_log).unwrap_or_default();
    text.contains("Microsoft") && text.contains("C/C++")
}

/// 经批处理加载 vcvars 后用 `cl` 编译运行（产物 `prog.exe`）。
fn cl_run(dir: &Path, sources: &[&str]) -> String {
    let bat = dir.join("step.bat");
    let out_log = dir.join("step.out");
    std::fs::write(
        &bat,
        format!(
            "@echo off\r\ncall \"{VCVARS}\" >nul 2>&1\r\ncl /nologo /W0 /Fe:prog.exe {} > \"{}\" 2>&1\r\n",
            sources.join(" "),
            out_log.display()
        ),
    )
    .expect("write step.bat");
    let status = Command::new("cmd")
        .args(["/c", bat.to_str().unwrap()])
        .current_dir(dir)
        .status()
        .expect("spawn cmd");
    let build_log = std::fs::read_to_string(&out_log).unwrap_or_default();
    assert!(
        status.success() && dir.join("prog.exe").is_file(),
        "MSVC compiles the A2C fileset:\n{build_log}"
    );
    let run = Command::new(dir.join("prog.exe"))
        .current_dir(dir)
        .output()
        .expect("run prog.exe");
    String::from_utf8_lossy(&run.stdout).to_string()
}

/// 正例：真 CLI + 包级 A2C + MSVC 真编译运行。
#[test]
fn p982_c_cli_builds_and_runs_the_cross_module_sample_via_a2c() {
    let dir = scratch("ace");
    project(&dir);
    let proof = dir.join("c-proof.json");

    let (ok, log) = build_cli(&dir, true, &proof);
    eprintln!(
        "p982 c-cli ace: ok={ok} log-tail={}",
        log.lines().rev().take(6).collect::<Vec<_>>().join(" | ")
    );
    assert!(ok, "`auto build` with AUTO_ACE_C=1 succeeds: {log}");
    assert!(
        log.contains("Native project detected"),
        "the scene-c lane ran: {log}"
    );

    // (1) fileset：包级一次编译的产物，不是 legacy 的逐文件 `util.c`。
    assert!(
        dir.join("main.c").is_file() && dir.join("generated.c").is_file(),
        "the A2C fileset lands: {:?}",
        std::fs::read_dir(&dir).map(|it| {
            it.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
    );
    assert!(
        !dir.join("util.c").is_file(),
        "the legacy per-file util.c is NOT emitted on the ACE lane"
    );

    // (2) 记账：a2c=1、legacy=0（回执落盘，外部可见证）。
    let receipt = proof_json(&proof);
    assert_eq!(receipt["engine"], "a2c", "the engine is named: {receipt}");
    assert_eq!(
        receipt["a2c_dispatches"], 1,
        "one package dispatch serves the whole project: {receipt}"
    );
    assert_eq!(
        receipt["legacy_dispatches"], 0,
        "the legacy CTrans never dispatched: {receipt}"
    );

    // (3) MSVC 真编译 + 运行：跨模块 `util.add(2, 3)` 的结果必须是 5。
    if !cl_available(&dir) {
        eprintln!("p982 c-cli ace: MSVC leg skipped (no cl via vcvars64)");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let stdout = cl_run(&dir, &["main.c", "generated.c"]);
    eprintln!("p982 c-cli ace: prog stdout={stdout:?}");
    assert!(
        stdout.contains("value:5"),
        "the compiled program prints the cross-module result: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 记账对照：同一工程不选 ACE ⇒ legacy 腿如实计数（两条路径各自记账）。
#[test]
fn p982_c_cli_legacy_lane_counts_its_own_dispatches() {
    let dir = scratch("legacy");
    project(&dir);
    let proof = dir.join("c-proof.json");

    let (ok, log) = build_cli(&dir, false, &proof);
    eprintln!(
        "p982 c-cli legacy: ok={ok} log-tail={}",
        log.lines().rev().take(6).collect::<Vec<_>>().join(" | ")
    );
    assert!(ok, "`auto build` on the legacy lane still succeeds: {log}");
    assert!(
        dir.join("util.c").is_file(),
        "the legacy lane emits its per-file util.c"
    );

    let receipt = proof_json(&proof);
    assert_eq!(
        receipt["engine"], "legacy",
        "the engine is named: {receipt}"
    );
    assert_eq!(
        receipt["a2c_dispatches"], 0,
        "no A2C dispatch on the legacy lane: {receipt}"
    );
    assert!(
        receipt["legacy_dispatches"].as_u64().unwrap_or(0) >= 1,
        "every legacy per-file dispatch is counted: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 缺席具名失败：显式选 ACE 而渲染器缺席（本测试进程未安装任何 C 渲染器）
/// ⇒ `ace.c/renderer-unavailable`，绝不静默回退旧 `CTrans`。
#[test]
fn p982_c_explicit_selection_without_a_renderer_is_a_named_refusal() {
    use auto_man_runtime::target::{Target, TargetEffects, TargetKind};

    let dir = scratch("absent");
    project(&dir);
    let main_at = dir.join("main.at").to_string_lossy().to_string();
    let util_at = dir.join("util.at").to_string_lossy().to_string();

    let mut target = Target::new("main", TargetKind::App);
    target.lang = "c".into();
    target.autos = vec![util_at.into(), main_at.into()];

    // 本测试进程从未安装 C 渲染器（安装点在 `auto` CLI 的启动里）。
    assert!(
        !auto_man_runtime::c_engine::renderer_installed(),
        "the test process has no A2C renderer installed"
    );
    #[allow(deprecated)]
    unsafe {
        std::env::set_var("AUTO_ACE_C", "1");
    }
    let refusal = target
        .transpile_auto()
        .expect_err("the refusal is an error");
    #[allow(deprecated)]
    unsafe {
        std::env::remove_var("AUTO_ACE_C");
    }
    let message = refusal.to_string();
    eprintln!("p982 c-cli absent: {message}");
    assert!(
        message.contains("ace.c/renderer-unavailable"),
        "the refusal names the missing renderer: {message}"
    );
    // 零写：拒绝发生在任何文件生成之前。
    assert!(
        !dir.join("main.c").is_file() && !dir.join("util.c").is_file(),
        "the refusal writes nothing"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
