//! PLAN-982（EXEC · 脚本 stdin，真实 CLI）：管道 stdin 交给 **`fn main(input str)`
//! 形脚本**（round-182 受检文本通道 + console 捕获），按内容分流；零参脚本
//! 不受影响（stdin 保持既有"不可读"边界，不静默改变语义）。

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-stdin-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

const TEXT_SCRIPT: &str = "fn main(input str) int {\n  if input == \"hello\" {\n    print(\"got:hello\")\n    return 1\n  }\n  print(\"got:other\")\n  return 2\n}\n";
const PLAIN_SCRIPT: &str = "fn main() int {\n  print(\"plain-leg\")\n  return 0\n}\n";

fn run_with_stdin(dir: &PathBuf, script: &str, stdin: Option<&str>) -> (bool, String) {
    let path = dir.join("script.at");
    std::fs::write(&path, script).expect("write script");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg("script.at")
        .current_dir(dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn auto");
    if let Some(text) = stdin {
        cmd.stdin
            .as_mut()
            .expect("piped stdin")
            .write_all(text.as_bytes())
            .expect("write stdin");
    }
    let out = cmd.wait_with_output().expect("collect");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// (1) `fn main(input str)` + 管道 stdin ⇒ 按内容分流（真实 CLI）。
#[test]
fn p982_exec_script_stdin_routes_by_content() {
    let dir = scratch("text");
    let (ok, log) = run_with_stdin(&dir, TEXT_SCRIPT, Some("hello"));
    eprintln!("p982 stdin cli hello: ok={ok} log={log:?}");
    assert!(ok, "the hello leg exits zero: {log}");
    assert!(log.contains("got:hello"), "content answered: {log}");
    assert!(log.contains("1"), "the returned value prints: {log}");

    let (ok2, log2) = run_with_stdin(&dir, TEXT_SCRIPT, Some("other"));
    eprintln!("p982 stdin cli other: ok={ok2} log={log2:?}");
    assert!(ok2, "the other leg exits zero: {log2}");
    assert!(
        log2.contains("got:other"),
        "the second content answers: {log2}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (2) 零参脚本 + 管道 stdin ⇒ 行为不变（忽略 stdin，正常执行）。
#[test]
fn p982_exec_zero_param_script_ignores_piped_stdin() {
    let dir = scratch("plain");
    let (ok, log) = run_with_stdin(&dir, PLAIN_SCRIPT, Some("piped-but-unreadable"));
    eprintln!("p982 stdin cli plain: ok={ok} log={log:?}");
    assert!(ok, "the zero-param script still runs: {log}");
    assert!(log.contains("plain-leg"), "its own output prints: {log}");
    let _ = std::fs::remove_dir_all(&dir);
}
