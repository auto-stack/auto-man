//! PLAN-982（982/RUST，V 第一步）：`AUTO_ACE_RUST` 候选开关的**真实 CLI** 证据。
//!
//! 断言的是一条**可区分**的边界，而不是"命令没崩"：
//! 1. 选中 A2R 时，`auto trans <file> -t rust` 的输出是 A2R 的降级程序
//!    （带 `AUTO-GENERATED`/`a2x_runtime` 标记），并且**不含**旧转译的形状；
//! 2. 未选中时输出仍是旧转译（**对照臂**，证明差异来自开关本身）；
//! 3. A2R 拒绝（如源里有该 profile 不接受的表达式）时**具名失败**且不回退
//!    旧转译——退出非零、stderr 带 `ace.rust/` 码。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

fn auto_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_auto"))
}

struct Project {
    dir: PathBuf,
    script: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn project(tag: &str, source: &str) -> Project {
    let dir = std::env::temp_dir().join(format!(
        "p982-rust-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project");
    let script = dir.join("lib.at");
    std::fs::write(&script, source).expect("write script");
    Project { dir, script }
}

fn trans(project: &Project, ace_rust: bool) -> std::process::Output {
    let mut cmd = Command::new(auto_exe());
    cmd.args(["rust", project.script.to_str().unwrap()])
        .current_dir(&project.dir)
        .env_remove("AUTO_ACE_RUST")
        .env_remove("AUTO_ACE_CANDIDATE");
    if ace_rust {
        cmd.env("AUTO_ACE_RUST", "ace");
    }
    cmd.output().expect("run auto trans")
}

const SRC: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    return total
}
"#;

#[test]
fn p982_rust_a2r_switch_changes_the_real_output() {
    let project = project("switch", SRC);

    let legacy = trans(&project, false);
    assert!(
        legacy.status.success(),
        "legacy trans must still succeed: {}",
        String::from_utf8_lossy(&legacy.stderr)
    );
    let legacy_out = String::from_utf8_lossy(&legacy.stdout).to_string();
    assert!(
        !legacy_out.contains("a2x_runtime"),
        "the control arm must NOT be A2R output: {legacy_out}"
    );

    let a2r = trans(&project, true);
    assert!(
        a2r.status.success(),
        "A2R trans must succeed on a legal source: {}",
        String::from_utf8_lossy(&a2r.stderr)
    );
    let a2r_out = String::from_utf8_lossy(&a2r.stdout).to_string();
    assert!(
        a2r_out.contains("AUTO-GENERATED") && a2r_out.contains("a2x_runtime"),
        "the candidate arm must be the A2R program: {a2r_out}"
    );
    assert!(
        a2r_out.contains("fn_main"),
        "the lowered entry is visible in the A2R program"
    );
    assert_ne!(
        legacy_out, a2r_out,
        "the two engines must produce different text (the switch is real)"
    );
}

#[test]
fn p982_rust_a2r_refusal_is_named_and_never_falls_back() {
    // 该源在 scalar 前门被拒（表达式语句）——A2R 必须具名失败，不能落回旧转译。
    let source = "fn main() int { add(1, 2) return 0 }\n";
    let project = project("refuse", source);
    let out = trans(&project, true);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success(),
        "an A2R refusal must not exit 0 (stderr: {stderr})"
    );
    assert!(
        stderr.contains("ace.rust/"),
        "the refusal must carry an `ace.rust/` code, got: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("fn main()"),
        "no legacy fallback output may appear on refusal"
    );
}
/// **零 legacy dispatch 的对外见证**：A2R 选中时回执必须写 `legacy_dispatches:0`
/// 且 `a2r_dispatches:1`——这是"没有静默走旧转译"的可核对证据（不是叙述）。
#[test]
fn p982_rust_a2r_proof_receipt_shows_zero_legacy_dispatch() {
    let project = project("proof", SRC);
    let receipt = project.dir.join("proof.json");
    let out = Command::new(auto_exe())
        .args(["rust", project.script.to_str().unwrap()])
        .current_dir(&project.dir)
        .env("AUTO_ACE_RUST", "ace")
        .env("AUTO_ACE_RUST_PROOF", &receipt)
        .env_remove("AUTO_ACE_CANDIDATE")
        .output()
        .expect("run auto rust");
    assert!(
        out.status.success(),
        "A2R run must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&receipt).expect("the proof receipt is written");
    let value: serde_json::Value = serde_json::from_str(&text).expect("receipt is JSON");
    assert_eq!(value["engine"], "a2r");
    assert_eq!(value["a2r_dispatches"], 1);
    assert_eq!(
        value["legacy_dispatches"], 0,
        "the candidate leg must not dispatch the legacy transpiler: {text}"
    );
}

/// 对照臂：未选中 A2R 时同一回执显示 `legacy_dispatches:1`（计数是真的，不是常量）。
#[test]
fn p982_rust_legacy_proof_receipt_counts_the_legacy_dispatch() {
    let project = project("proof-legacy", SRC);
    let receipt = project.dir.join("proof.json");
    let out = Command::new(auto_exe())
        .args(["rust", project.script.to_str().unwrap()])
        .current_dir(&project.dir)
        .env("AUTO_ACE_RUST_PROOF", &receipt)
        .env_remove("AUTO_ACE_RUST")
        .env_remove("AUTO_ACE_CANDIDATE")
        .output()
        .expect("run auto rust");
    assert!(out.status.success(), "legacy run must succeed");
    let text = std::fs::read_to_string(&receipt).expect("receipt");
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(value["engine"], "legacy");
    assert_eq!(value["legacy_dispatches"], 1);
    assert_eq!(value["a2r_dispatches"], 0);
}
