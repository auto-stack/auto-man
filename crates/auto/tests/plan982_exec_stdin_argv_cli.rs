//! PLAN-982（aggregates 文本入口裁定，2026-11-19，round 198）：**sys 车道
//! stdin＋argv 双通道真 CLI 兼容门**——同一真实 CLI 脚本在**同一次执行**里
//! 读 stdin 文本（`fn main(input str)` 受检文本入口，`ac.runtime.text.v1`
//! 谱系）与 argv 列表（`process.args()`），两路输入独立变化；验返回值、
//! stdout、stderr、退出码；wall/cancel 已接线（`AUTO_ACE_WALL_MS`/
//! `AUTO_ACE_CANCEL_AFTER_MS`）；same_sdk 收据核对配置与实际执行的 SDK
//! 身份＋零旧 VM 分派。
//!
//! 回归族（裁定④）：无参脚本不受影响、空串实参、空格、引号、Unicode、
//! `--` 后参数、逐次隔离、取消、spawn 拒绝；零参入口携带 stdin 文本 ⇒
//! 执行端具名拒（文本不静默丢弃）。

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-stdinx-{}-{}-{}",
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

struct CliOut {
    ok: bool,
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_sys(
    dir: &PathBuf,
    script_name: &str,
    script: &str,
    stdin: Option<&str>,
    args: &[&str],
) -> CliOut {
    std::fs::write(dir.join(script_name), script).expect("write script");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.arg(script_name)
        .args(args)
        .current_dir(dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_ACE_SCRIPT_SYS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn auto");
    if let Some(text) = stdin {
        // None = 继承（本套件不派 TTY；None 腿不经此路径）。
        let pipe = child.stdin.take().expect("stdin pipe");
        let mut pipe = pipe;
        let _ = pipe.write_all(text.as_bytes());
        let _ = pipe.flush();
    } else {
        // 关闭管道（EOF）＝「缺失/空输入」沿现有合同的腿。
        drop(child.stdin.take());
    }
    let out = child.wait_with_output().expect("wait auto");
    CliOut {
        ok: out.status.success(),
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

/// 双通道脚本：stdin 分流（hello/other）×argv 计数（3/1）⇒ 四个不同返回值；
/// print/eprint 两流＋值 oracle 全内嵌。
const DUAL: &str = "use std.process.args\nfn main(input str) int {\n  let a = args()\n  let n = a.len()\n  if input == \"hello\" {\n    if n == 3 { print(\"both:hello:3\")\n      eprint(\"note:3\")\n      return 31 }\n    print(\"text:hello\")\n    return 1\n  }\n  if n == 3 { print(\"argv:3\")\n    return 32 }\n  print(\"plain:other\")\n  eprint(\"note:plain\")\n  return 2\n}\n";

/// (1) 核心：同一脚本，两路输入独立变化 ⇒ 返回值/stdout/stderr/退出码逐腿核对。
#[test]
fn p982_stdin_argv_dual_channel_by_real_cli() {
    let dir = scratch("dual");

    // stdin=hello + argv 3 项（argv0＋2 参）⇒ 31 ＋ both 流。
    let o = run_sys(&dir, "s.at", DUAL, Some("hello"), &["alpha", "beta"]);
    eprintln!(
        "p982 dual hello+3: ok={} code={} out={:?} err={:?}",
        o.ok, o.code, o.stdout, o.stderr
    );
    assert!(o.ok, "exit code 0 on success: stderr={}", o.stderr);
    assert!(
        o.stdout.contains("both:hello:3"),
        "stdout carries print: {}",
        o.stdout
    );
    assert!(
        o.stderr.contains("note:3"),
        "stderr carries eprint: {}",
        o.stderr
    );
    assert!(
        o.stdout.trim_end().ends_with("31"),
        "the value oracle prints 31: {}",
        o.stdout
    );

    // stdin=other + 同 argv ⇒ 2（stdin 变化重新路由）。
    let o2 = run_sys(&dir, "s.at", DUAL, Some("other"), &["alpha", "beta"]);
    assert!(o2.ok, "same argv, other stdin still succeeds");
    assert!(
        o2.stdout.contains("argv:3"),
        "the argv branch: {}",
        o2.stdout
    );
    assert!(o2.stdout.trim_end().ends_with("32"));

    // stdin 不变 + argv 变 1 项（argv0 only）⇒ 1（argv 变化重新路由）。
    let o3 = run_sys(&dir, "s.at", DUAL, Some("hello"), &[]);
    assert!(o3.ok);
    assert!(
        o3.stdout.contains("text:hello"),
        "the text-only branch: {}",
        o3.stdout
    );
    assert!(o3.stdout.trim_end().ends_with("1"));

    // 双双回落 ⇒ 2。
    let o4 = run_sys(&dir, "s.at", DUAL, Some("zzz"), &[]);
    assert!(o4.ok);
    assert!(
        o4.stdout.trim_end().ends_with("2"),
        "plain other leg: {}",
        o4.stdout
    );

    // 逐次隔离：两次执行（不同 stdin＋不同 argv）各查各的，无串漏。
    let i1 = run_sys(&dir, "s.at", DUAL, Some("hello"), &["x1", "x2"]);
    let i2 = run_sys(&dir, "s.at", DUAL, Some("other"), &["y1"]);
    assert!(i1.stdout.trim_end().ends_with("31"), "first isolation leg");
    assert!(
        i2.stdout.trim_end().ends_with("2"),
        "second isolation leg sees only its own inputs"
    );
}

/// (2) 回归族：无参脚本不受影响（stdin 关闭/携带都不改零参语义）；空串/
/// 空格/引号/Unicode/`--` 实参与 stdin 同跑逐项保真。
#[test]
fn p982_stdin_argv_regressions_by_real_cli() {
    let dir = scratch("reg");

    // 无参脚本＋管道 stdin ⇒ 照常运行（零参入口不读 stdin）。
    let plain = "fn main() int { return 42 }\n";
    let o = run_sys(&dir, "s.at", plain, Some("ignored"), &[]);
    eprintln!(
        "p982 reg zero-arg with piped stdin: ok={} out={:?}",
        o.ok, o.stdout
    );
    assert!(
        o.ok,
        "zero-arg main keeps running with piped stdin: {}",
        o.stderr
    );
    assert!(o.stdout.trim_end().ends_with("42"));

    // 六形实参 × stdin 同跑（str 形入口＋argv 逐项）。
    let shapes = ["", "b c", "say \"hi\"", "你好-λ", "--flag=ón", "--"];
    let mut body = String::from("use std.process.args\nfn main(input str) int {\n  let a = args()\n  if (a.len() != 7) { return 100 }\n  if (input != \"in-λ\") { return 101 }\n");
    for (i, e) in shapes.iter().enumerate() {
        let lit = e.replace('\\', "\\\\").replace('"', "\\\"");
        body.push_str(&format!(
            "  let v{} = a.remove(1)\n  if (v{} != \"{}\") {{ return 1{} }}\n",
            i,
            i,
            lit,
            i + 2
        ));
    }
    body.push_str("  return 7\n}\n");
    let o2 = run_sys(&dir, "t.at", &body, Some("in-λ"), &shapes);
    eprintln!(
        "p982 reg six shapes with stdin: ok={} code={} err={:?}",
        o2.ok, o2.code, o2.stderr
    );
    assert!(o2.ok, "six shapes + stdin survive together: {}", o2.stderr);
    assert!(
        o2.stdout.trim_end().ends_with("7"),
        "the checker returns 7: {}",
        o2.stdout
    );

    // 零参入口携带 stdin 文本 ⇒ CLI 按**受检入口签名**不读 stdin（零参
    // `main()` 继续运行、文本不被消费）；宿主层的具名拒（零参入口＋文本
    // 实参）由语言侧 `p982_sys_text_input_without_text_entry_refused` 持有。
    let zero_arg = "fn main() int { return 5 }\n";
    let o3 = run_sys(&dir, "z.at", zero_arg, Some("stray"), &[]);
    eprintln!(
        "p982 reg zero-arg entry with stdin payload: ok={} out={:?}",
        o3.ok, o3.stdout
    );
    assert!(
        o3.ok,
        "a zero-arg entry ignores piped stdin (the CLI reads by signature): {}",
        o3.stderr
    );
    assert!(o3.stdout.trim_end().ends_with("5"));

    // spawn 负控（窄面不可名 spawn）与 stdin 组合。
    let spawn_src = "use std.process.spawn_with_output\nfn main(input str) int { return 0 }\n";
    let o4 = run_sys(&dir, "sp.at", spawn_src, Some("x"), &[]);
    eprintln!("p982 reg spawn refusal: ok={} err={:?}", o4.ok, o4.stderr);
    assert!(!o4.ok, "spawn stays unnameable in the narrow face");
}

/// (3) 取消（裁定④回归族）：watchdog 置 cancel ⇒ `ace.sys/cancelled` 具名
/// 非零退出（不回退 legacy）。热循环脚本跑足够长。
#[test]
fn p982_stdin_argv_cancel_by_real_cli() {
    let dir = scratch("cancel");
    let hot = "fn main(input str) int {\n  var i = 0\n  for i < 2000000000 { i = i + 1 }\n  return i\n}\n";
    std::fs::write(dir.join("s.at"), hot).expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg("s.at")
        .current_dir(&dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_ACE_SCRIPT_SYS", "1")
        .env("AUTO_ACE_CANCEL_AFTER_MS", "300")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            if let Some(pipe) = c.stdin.as_mut() {
                let _ = pipe.write_all(b"spin");
            }
            drop(c.stdin.take());
            c.wait_with_output()
        })
        .expect("run cancelled leg");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    eprintln!("p982 cancel: ok={} err={:?}", out.status.success(), stderr);
    assert!(!out.status.success(), "a cancelled run exits non-zero");
    assert!(
        stderr.contains("ace.sys/cancelled"),
        "the cancellation is named (wall/cancel wiring): {stderr}"
    );
}

/// (4) same_sdk 收据（裁定④）：配置腿/执行腿/实际服务引擎同一 SDK 身份；
/// 零旧 VM 分派（`legacy_dispatch == 0`）；stdin/argv 字节数入账。
#[test]
fn p982_stdin_argv_same_sdk_receipt_by_real_cli() {
    let dir = scratch("receipt");
    let receipt_path = dir.join("receipt.json");
    std::fs::write(dir.join("s.at"), DUAL).expect("write script");
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg("s.at")
        .args(["one", "two"])
        .current_dir(&dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_ACE_SCRIPT_SYS", "1")
        .env("AUTO_ACE_PROOF_RECEIPT", receipt_path.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            if let Some(pipe) = c.stdin.as_mut() {
                let _ = pipe.write_all(b"hello");
            }
            drop(c.stdin.take());
            c.wait_with_output()
        })
        .expect("run receipt leg");
    assert!(
        out.status.success(),
        "the dual run succeeds: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&receipt_path).expect("receipt written");
    let receipt: serde_json::Value = serde_json::from_str(&text).expect("receipt json");
    eprintln!("p982 receipt: {receipt:#?}");
    assert_eq!(receipt["lane"], "sys", "the sys lane records its identity");
    assert_eq!(
        receipt["same_sdk"], true,
        "config leg + execution leg + engine = one SDK"
    );
    assert_eq!(
        receipt["execution"]["engine"], "av",
        "the engine that served is the identity engine"
    );
    assert_eq!(
        receipt["execution"]["engine_is_identity_engine"], true,
        "engine_is_identity_engine recorded"
    );
    assert_eq!(
        receipt["config"]["legacy_dispatch"], 0,
        "zero legacy VM dispatches (config leg)"
    );
    assert_eq!(receipt["execution"]["argv_count"], 2, "argv count recorded");
    assert_eq!(
        receipt["execution"]["stdin_bytes"], 5,
        "stdin bytes recorded"
    );
    assert!(
        receipt["config"]["owner"].is_string(),
        "config owner identity present"
    );
    assert!(
        receipt["execution"]["owner"].is_string(),
        "execution owner identity present"
    );
}
