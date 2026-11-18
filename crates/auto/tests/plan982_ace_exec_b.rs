//! PLAN-982（982/EXEC 切片，I/V 第一步）：候选腿的**命令面边界**。
//!
//! 已接通的命令面（截至 round 187）：裸脚本（A 切片）、脚本 stdin／`config`
//! 文档参数（round 185）、无过滤词 `test`（round 186）、非 agent `debug` 与
//! `req`/`serve`（round 187）。EXEC 的
//! "未知目标不能 legacy fallback" 要求：一旦选中候选，**没有 ACE 实现的 run 家族
//! 子命令必须具名拒绝**，而不是静默落回旧执行器。本文件用真实 CLI 证明这一点，
//! 并用"未选中候选"的控制臂证明拒绝来自候选选择本身（不是命令本来就坏）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

fn auto_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_auto"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-exec-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project");
    dir
}

fn run(dir: &PathBuf, args: &[&str], candidate: bool) -> std::process::Output {
    let mut cmd = Command::new(auto_exe());
    cmd.args(args).current_dir(dir);
    cmd.env_remove("AUTO_ACE_CANDIDATE");
    if candidate {
        cmd.env("AUTO_ACE_CANDIDATE", "1");
    }
    cmd.output().expect("run auto")
}

/// 选中候选后，未迁移的 run 家族子命令必须非零退出 +
/// 具名 `ace.exec/unsupported-command`，且**不写任何产物**。
///
/// 边界随 EXEC 推进收窄：round 186 起 `test`（round 189 扩至 `--filter`）、
/// round 187 起非 agent `debug` 与 `req`（匿名 one-shot 走会话宿主、会话化走
/// wire 客户端）接通——它们的正面行为由 `plan982_exec_test_cli`／
/// `plan982_exec_debug_cli`／`plan982_exec_req_cli` 各自证明；本测试钉住
/// **仍未迁移**的集合。
#[test]
fn p982_exec_unsupported_commands_refuse_by_name() {
    for (command, args) in [
        ("run", vec!["run"]),
        ("service", vec!["service", "--server", "vm"]),
        (
            "service-export",
            vec!["service-export", "--delivery-root", "out"],
        ),
        ("test-ui", vec!["test:ui"]),
        ("debug", vec!["debug", "--agent", "no_such_script.at"]),
    ] {
        let dir = temp_dir(command);
        let before: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        let out = run(&dir, &args, true);
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            !out.status.success(),
            "`auto {command}` under the candidate leg must fail, got {:?}",
            out.status
        );
        assert!(
            stderr.contains("ace.exec/unsupported-command"),
            "`auto {command}` must name the refusal, stderr was: {stderr}"
        );
        let after: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(
            before.len(),
            after.len(),
            "`auto {command}` must not write into the project"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// `--ai` 形态：同一拒绝必须是结构化 JSON（`status/code/message`），退出非零。
#[test]
fn p982_exec_unsupported_command_ai_json() {
    let dir = temp_dir("ai");
    let out = run(&dir, &["run", "--ai"], true);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(!out.status.success());
    let value: serde_json::Value =
        serde_json::from_str(stderr.trim()).unwrap_or_else(|e| panic!("not JSON ({e}): {stderr}"));
    assert_eq!(value["status"], "error");
    assert_eq!(value["code"], "ace.exec/unsupported-command");
    assert!(value["message"].as_str().unwrap_or("").contains("run"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// 控制臂：**未**选中候选时同一命令不得出现该拒绝码——证明拒绝来自候选选择，
/// 而不是命令本身缺失（否则本用例就是空证）。
#[test]
fn p982_exec_refusal_comes_from_the_candidate_selection() {
    let dir = temp_dir("control");
    let out = run(&dir, &["req"], false);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("ace.exec/unsupported-command"),
        "without the candidate selection `auto req` must not report the ACE boundary: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
