//! PLAN-982（EXEC · round 186）：**`auto test`（无过滤词）的候选腿**——真实
//! CLI 经 ACE 项目测试门（`run_ace_project_tests` + 宿主 ProjectHost）：
//! 零参 `int` 函数即测试候选（宿主发现合同）；失败 ⇒ 非零退出。
//!
//! 三腿：全过 ⇒ 退出 0；一败 ⇒ 非零 + `FAILED`；带 `--filter` ⇒ 具名拒绝。

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-test-{tag}-{}-{}",
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
fn p982_exec_test_passes_and_fails_by_real_cli() {
    let dir = scratch("mixed");
    std::fs::write(
        dir.join("suite.at"),
        "fn main() int { return 0 }\nfn first_check() int { return 0 }\nfn second_check() int { return 1 }\n",
    )
    .expect("write suite");

    // (1) 全过：测试合同 = 零参 `int` 返回 0（宿主冻结合同）。
    let passing = dir.join("passing.at");
    std::fs::write(
        &passing,
        "fn main() int { return 0 }\nfn good_check() int { return 0 }\n",
    )
    .expect("write passing");
    let (ok, log) = run_cli(&dir, &["test", "--dir", "passing.at"]);
    eprintln!("p982 test passing: ok={ok} log={log:?}");
    assert!(ok, "the passing suite exits zero: {log}");
    assert!(log.contains("test good_check ... ok"), "{log}");
    assert!(log.contains("failures=0"), "{log}");

    // (2) 一败：失败测试 ⇒ 非零退出 + FAILED 行（失败码按合同）。
    let (ok2, log2) = run_cli(&dir, &["test", "--dir", "suite.at"]);
    eprintln!("p982 test mixed: ok={ok2} log={log2:?}");
    assert!(!ok2, "a failing suite exits non-zero: {log2}");
    assert!(log2.contains("FAILED"), "the failure is named: {log2}");
    assert!(
        log2.contains("ace.test/failures"),
        "the exit is named: {log2}"
    );

    // (3) 带 --filter ⇒ 候选腿具名拒绝（过滤面未迁移）。
    let (ok3, log3) = run_cli(&dir, &["test", "--dir", "suite.at", "--filter", "first"]);
    eprintln!("p982 test filter: ok={ok3} log3={log3:?}");
    assert!(!ok3, "the filtered form is refused: {log3}");
    assert!(log3.contains("ace.exec/unsupported-command"), "{log3}");
    let _ = std::fs::remove_dir_all(&dir);
}
