//! PLAN-982（EXEC · round 187）：**`auto req`／`auto serve` 的候选腿**——
//! ACE 会话 daemon（970 T-03）＋ legacy wire 客户端复用。
//!
//! 腿 A（daemon 链，真实多进程）：候选 `auto serve --foreground` 起 ACE
//! daemon ⇒ `--new-session` ⇒ 会话内 `let x = 5` ⇒ **另一个 CLI 进程**
//! 同会话求值 `x * 2 + 1` 出 11（**req 持久变量**，EXEC 验收点）⇒ `--list`
//! 列出会话（lang 侧本轮修的 `data` 兼容）⇒ `--delete` 后 `--list` 为空。
//! 腿 B（匿名 one-shot）：候选腿走 ACE 会话宿主临时会话（不再调旧
//! `eval_one_shot`）；控制臂（无候选环境变量）同命令仍出 5——拒绝/分岔
//! 源于候选选择，不是命令本身坏。

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn run_cli(dir: &PathBuf, args: &[&str], candidate: bool) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.args(args).current_dir(dir);
    if candidate {
        cmd.env("AUTO_ACE_CANDIDATE", "1");
    }
    let out = cmd.output().expect("spawn auto");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// Kill the serve child even when the test panics mid-flow（不留孤儿
/// daemon 持有管道——上一版在断言失败时泄漏过 serve 进程）。
struct ServeGuard(std::process::Child);
impl Drop for ServeGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn p982_exec_req_persistent_vars_over_the_ace_daemon() {
    let dir = std::env::temp_dir().join(format!(
        "p982-req-daemon-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    let pipe = format!(
        "p982req-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    );

    // 候选 serve（前台子进程 = 管道监听主循环；ServeGuard 兜底 kill）。
    let serve = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args(["serve", "--foreground", "--pipe-name", &pipe])
        .env("AUTO_ACE_CANDIDATE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn serve");
    let _serve_guard = ServeGuard(serve);
    // 监听就绪窗（客户端自带 10×200ms 连接重试，这里只垫首拍）。
    std::thread::sleep(std::time::Duration::from_millis(400));

    // (1) 新会话：打印会话 id。
    let (ok, log) = run_cli(&dir, &["req", "-p", &pipe, "--new-session"], true);
    eprintln!("p982 req new-session: ok={ok} log={log:?}");
    assert!(ok, "new-session exits zero: {log}");
    let session = log.trim().to_string();
    assert!(
        session.starts_with("ses_"),
        "a session id is printed: {log}"
    );

    // (2) 会话内绑定变量（语句求值：无输出值，零副作用失败）。
    let (ok2, log2) = run_cli(
        &dir,
        &["req", "-p", &pipe, "-s", &session, "let x = 5"],
        true,
    );
    eprintln!("p982 req bind: ok={ok2} log={log2:?}");
    assert!(ok2, "the binding eval exits zero: {log2}");

    // (3) 另一个 CLI 进程，同一会话：携带变量求值 ⇒ 11（持久变量验收）。
    let (ok3, log3) = run_cli(
        &dir,
        &["req", "-p", &pipe, "-s", &session, "x * 2 + 1"],
        true,
    );
    eprintln!("p982 req carry: ok={ok3} log={log3:?}");
    assert!(ok3, "the carry eval exits zero: {log3}");
    assert!(
        log3.trim().contains("11"),
        "the carried value is 11: {log3}"
    );

    // (4) list：会话清单进 data（legacy 客户端读得到）。
    let (ok4, log4) = run_cli(&dir, &["req", "-p", &pipe, "--list"], true);
    eprintln!("p982 req list: ok={ok4} log={log4:?}");
    assert!(ok4, "list exits zero: {log4}");
    assert!(
        log4.contains("Sessions:") && log4.contains(&session),
        "the live session is listed: {log4}"
    );

    // (5) delete 后 list 为空。
    let (ok5, _log5) = run_cli(
        &dir,
        &["req", "-p", &pipe, "-s", &session, "--delete"],
        true,
    );
    assert!(ok5, "delete exits zero");
    let (ok6, log6) = run_cli(&dir, &["req", "-p", &pipe, "--list"], true);
    eprintln!("p982 req list-after-delete: ok={ok6} log={log6:?}");
    assert!(ok6, "list after delete exits zero: {log6}");
    assert!(log6.contains("No sessions"), "the session is gone: {log6}");

    drop(_serve_guard);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p982_exec_req_anonymous_one_shot_on_both_legs() {
    let dir = std::env::temp_dir().join(format!(
        "p982-req-shot-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");

    // (1) 候选腿：匿名 one-shot 走 ACE 会话宿主（临时会话，求值即关）。
    let (ok, log) = run_cli(&dir, &["req", "2 + 3"], true);
    eprintln!("p982 req shot candidate: ok={ok} log={log:?}");
    assert!(ok, "the one-shot exits zero: {log}");
    assert!(log.trim() == "5", "the value prints: {log}");

    // (2) 候选腿 --json：{status, value} 形状（与 legacy 面同形）。
    let (ok2, log2) = run_cli(&dir, &["req", "--json", "2 + 3"], true);
    eprintln!("p982 req shot json: ok={ok2} log={log2:?}");
    assert!(ok2, "the json one-shot exits zero: {log2}");
    assert!(log2.contains("\"status\":\"ok\""), "{log2}");
    assert!(log2.contains("\"value\":\"5\""), "{log2}");

    // (3) 控制臂：不选候选 ⇒ 同命令走 legacy `eval_one_shot`，同样出 5
    //（证明分岔源于候选选择，不是命令本身坏）。
    let (ok3, log3) = run_cli(&dir, &["req", "2 + 3"], false);
    eprintln!("p982 req shot legacy control: ok={ok3} log={log3:?}");
    assert!(ok3, "the legacy control exits zero: {log3}");
    assert!(log3.trim() == "5", "the legacy leg still prints 5: {log3}");
    let _ = std::fs::remove_dir_all(&dir);
}
