//! PLAN-982（EXEC · round 187）：**`auto debug`（非 agent）的候选腿**——
//! 真实 CLI 经 ACE 调试门（`start_ace_debug` + 宿主 ProjectDebugSession，
//! 971 形状）：入口断点（start 即在入口首指令前暂停）＋ continue 到终态。
//!
//! 四腿：干净脚本 ⇒ 入口停 + finished + 值 + 退出 0；坏源 ⇒ 非零 +
//! `ace.debug/project-diagnostic`（失败码）；`--agent` ⇒ 具名拒绝；
//! 目标缺失 ⇒ `ace.debug/target-missing`。

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-dbg-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn run_cli(dir: &PathBuf, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args(args)
        .current_dir(dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .output()
        .expect("spawn auto");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

#[test]
fn p982_exec_debug_runs_through_the_ace_debug_door() {
    let dir = scratch("door");

    // (1) 干净脚本：入口停（生产者源级位置）＋ 终态值 ＋ 退出 0。
    std::fs::write(dir.join("clean.at"), "fn main() int { return 42 }\n").expect("write clean");
    let (ok, log) = run_cli(&dir, &["debug", "clean.at"]);
    eprintln!("p982 debug clean: ok={ok} log={log:?}");
    assert!(ok, "the clean script debug-exits zero: {log}");
    assert!(
        log.contains("ACE debug: entry stop at app::clean::main"),
        "the entry stop names the producer source position: {log}"
    );
    assert!(log.contains("continuing to the terminal state"), "{log}");
    assert!(log.contains("finished"), "{log}");
    assert!(log.contains("value = 42"), "{log}");

    // (2) 坏源：编译拒 ⇒ 非零 + 具名失败码（debug 的失败码纪律）。
    std::fs::write(dir.join("broken.at"), "fn main( { return 1 }\n").expect("write broken");
    let (ok2, log2) = run_cli(&dir, &["debug", "broken.at"]);
    eprintln!("p982 debug broken: ok={ok2} log={log2:?}");
    assert!(!ok2, "a rejected source debug-exits non-zero: {log2}");
    assert!(
        log2.contains("ace.debug/project-diagnostic"),
        "the rejection is named: {log2}"
    );

    // (3) agent 协议面：保持具名拒绝（未迁移）。
    let (ok3, log3) = run_cli(&dir, &["debug", "--agent", "clean.at"]);
    eprintln!("p982 debug agent: ok={ok3} log={log3:?}");
    assert!(!ok3, "the agent face is refused: {log3}");
    assert!(log3.contains("ace.exec/unsupported-command"), "{log3}");

    // (4) 目标缺失：具名失败 + 非零。
    let (ok4, log4) = run_cli(&dir, &["debug", "no-such.at"]);
    eprintln!("p982 debug missing: ok={ok4} log={log4:?}");
    assert!(!ok4, "a missing target is a named failure: {log4}");
    assert!(log4.contains("ace.debug/target-missing"), "{log4}");
    let _ = std::fs::remove_dir_all(&dir);
}
