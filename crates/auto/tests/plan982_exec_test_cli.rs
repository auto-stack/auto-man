//! PLAN-982（EXEC · round 186/189）：**`auto test` 的候选腿**——真实
//! CLI 经 ACE 项目测试门（`run_ace_project_tests` + 宿主 ProjectHost）：
//! 零参 `int` 函数即测试候选（宿主发现合同）；失败 ⇒ 非零退出。
//! round 189 起 `--filter` 一并走 ACE：限定名子串、**执行前筛选**
//!（未选中者不运行）、零匹配＝零执行＋退出 0（legacy 匹配/退出语义）。
//!
//! 五腿：全过 ⇒ 0；一败 ⇒ 非零＋FAILED；filter 选中通过者 ⇒ 0 且未选中
//! 者不运行（反控：腿 2 已证其若运行即失败）；filter 选中失败者 ⇒ 非零；
//! filter 零匹配 ⇒ 0＋具名零匹配行。

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

    // (3) --filter 选中通过者（round 189）：`second_check`（若运行即失败，
    //  腿 2 已证）不被执行 ⇒ 全过、退出 0、输出不含未选中者（反控）。
    let (ok3, log3) = run_cli(&dir, &["test", "--dir", "suite.at", "--filter", "first"]);
    eprintln!("p982 test filter-select: ok={ok3} log3={log3:?}");
    assert!(
        ok3,
        "the filtered run exits zero when the selected tests pass: {log3}"
    );
    assert!(log3.contains("test first_check ... ok"), "{log3}");
    assert!(
        !log3.contains("second_check"),
        "the unselected test never runs: {log3}"
    );
    assert!(log3.contains("tests=1"), "{log3}");

    // (4) --filter 只选中失败者 ⇒ 非零退出（选中集的失败按失败码合同）。
    let (ok4, log4) = run_cli(&dir, &["test", "--dir", "suite.at", "--filter", "second"]);
    eprintln!("p982 test filter-fail: ok={ok4} log4={log4:?}");
    assert!(
        !ok4,
        "the filtered run fails when the selected test fails: {log4}"
    );
    assert!(log4.contains("FAILED"), "{log4}");
    assert!(
        !log4.contains("first_check ..."),
        "the unselected test never runs: {log4}"
    );

    // (5) --filter 零匹配 ⇒ 什么都不执行、退出 0（legacy 退出语义），
    //  且零匹配可见（具名行 + discovered 对照，不静默）。
    let (ok5, log5) = run_cli(&dir, &["test", "--dir", "suite.at", "--filter", "no-such"]);
    eprintln!("p982 test filter-zero: ok={ok5} log5={log5:?}");
    assert!(
        ok5,
        "a zero-match filter exits zero (legacy semantics): {log5}"
    );
    assert!(log5.contains("0 tests matched 'no-such'"), "{log5}");
    assert!(log5.contains("tests=0"), "{log5}");
    assert!(
        !log5.contains("first_check ..."),
        "nothing executes on zero match: {log5}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
