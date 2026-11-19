//! PLAN-982（Q-982-2 车道裁定 (a) 第二步收官，round 199 默认切换收尾刀）：
//! **Windows 脚本面默认入口翻转为 ACE sys 车道**——切前对拍面已核（round-198
//! 完整兼容门收据＋本刀 legacy 实证），切换以 legacy 可观察**字节对拍**验收。
//!
//! 对拍面口径（切前实证，见 plan §9 round 199）：
//! - **字节对拍面**＝零参值脚本、print-only 脚本（stdout 块后带换行的
//!   `println!("{}", stdout)` 语义）、成功退出 0／编译失败退出非 0；
//! - **裁定契约面**（语义增量，不做 legacy 字节对拍）＝stdin 喂入
//!   （legacy 不喂——str 形参恒空）、argv 的 stdlib 拼写（legacy 只有 native
//!   `process.args()`）、stderr 再发（legacy 丢弃 eprint；print+eprint 组合
//!   在 legacy 直接 nanbox panic——存量缺口在案）；
//! - **命名边界**＝UI 关键字脚本（默认仍走 legacy 动态 UI 车道；显式候选
//!   具名拒）、`--dump-lowered`（默认保持 legacy 审查面；显式候选具名拒）；
//! - **回退钮**＝`AUTO_ACE_CANDIDATE=0` 显式回 legacy。

#![cfg(all(feature = "ace-candidate", windows))]

use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-defsw-{}-{}-{}",
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

fn run(
    dir: &PathBuf,
    name: &str,
    script: &str,
    envs: &[(&str, &str)],
    args: &[&str],
    stdin: Option<&str>,
) -> Output {
    std::fs::write(dir.join(name), script).expect("write script");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.arg(name)
        .args(args)
        .current_dir(dir)
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn auto");
    match stdin {
        Some(text) => {
            if let Some(pipe) = child.stdin.as_mut() {
                let _ = pipe.write_all(text.as_bytes());
            }
        }
        None => {
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(b"");
            }
        }
    }
    drop(child.stdin.take());
    child.wait_with_output().expect("wait auto")
}

fn stdout_of(out: &Output) -> Vec<u8> {
    out.stdout.clone()
}

const ZERO: &str = "fn main() int { return 42 }\n";
const PRINT_ONLY: &str = "fn main() int { print(42)\n    return 7 }\n";
const BAD: &str = "fn main( int { return 1 }\n";
const USE_ARGS: &str = "use std.process.args\nfn main() int { let a = args()\n    if (a.len() == 3) { return 31 }\n    return 2 }\n";

/// (1) 字节对拍：默认（无 env）与 legacy 回退钮在零参/print-only/失败退出
/// 三形上**逐字节一致**（含 stdout 块后换行语义），且默认不添车道标记行。
#[test]
fn p982_default_switch_byte_parity_with_legacy() {
    let dir = scratch("parity");

    for (name, script) in [("zero.at", ZERO), ("p1.at", PRINT_ONLY)] {
        let default = run(&dir, name, script, &[], &[], None);
        let legacy = run(
            &dir,
            name,
            script,
            &[("AUTO_ACE_CANDIDATE", "0")],
            &[],
            None,
        );
        eprintln!(
            "p982 parity {name}: default={:?} legacy={:?}",
            String::from_utf8_lossy(&default.stdout),
            String::from_utf8_lossy(&legacy.stdout)
        );
        assert!(
            default.status.success() && legacy.status.success(),
            "both legs succeed on {name}"
        );
        assert_eq!(
            stdout_of(&default),
            stdout_of(&legacy),
            "byte parity on {name}: default(sys) must match legacy exactly"
        );
        assert!(
            !String::from_utf8_lossy(&default.stdout).contains("ACE sys lane"),
            "the DEFAULT path adds no lane marker line ({}): {}",
            name,
            String::from_utf8_lossy(&default.stdout)
        );
    }

    // 编译失败：两腿都非零退出（诊断文本不同——legacy 语法码 vs ACE 具名码；
    // 对拍口径=退出码族）。
    let default_bad = run(&dir, "bad.at", BAD, &[], &[], None);
    let legacy_bad = run(
        &dir,
        "bad.at",
        BAD,
        &[("AUTO_ACE_CANDIDATE", "0")],
        &[],
        None,
    );
    assert!(!default_bad.status.success(), "sys lane fails non-zero");
    assert!(!legacy_bad.status.success(), "legacy fails non-zero");
}

/// (2) 默认身份为证：默认（无 env）的收据 `lane=sys`、`same_sdk=true`、
/// 引擎=身份引擎——证明字节兼容的默认腿真的是 ACE sys 车道（不是 legacy）。
#[test]
fn p982_default_switch_is_sys_lane_with_receipt() {
    let dir = scratch("identity");
    let receipt = dir.join("receipt.json");
    let out = run(
        &dir,
        "s.at",
        "use std.process.args\nfn main(input str) int {\n  let a = args()\n  if input == \"hello\" {\n    if (a.len() == 3) { return 31 }\n    return 1\n  }\n  return 2\n}\n",
        &[("AUTO_ACE_PROOF_RECEIPT", receipt.to_str().unwrap())],
        &["alpha", "beta"],
        Some("hello"),
    );
    eprintln!(
        "p982 identity: ok={} out={:?}",
        out.status.success(),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        out.status.success(),
        "the dual-channel script runs under the DEFAULT"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .trim_end()
            .ends_with("31"),
        "stdin+argv both serve under the default: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let text = std::fs::read_to_string(&receipt).expect("receipt written");
    let receipt: serde_json::Value = serde_json::from_str(&text).expect("receipt json");
    eprintln!("p982 identity receipt: {receipt:#?}");
    assert_eq!(receipt["lane"], "sys", "the default entry is the sys lane");
    assert_eq!(receipt["same_sdk"], true, "one SDK across both legs");
    assert_eq!(
        receipt["execution"]["engine"], "av",
        "the identity engine served"
    );
    assert_eq!(
        receipt["config"]["legacy_dispatch"], 0,
        "zero legacy VM dispatches"
    );
    assert_eq!(receipt["execution"]["stdin_bytes"], 5, "stdin recorded");
    assert_eq!(receipt["execution"]["argv_count"], 2, "argv recorded");
}

/// (3) 回退钮：`AUTO_ACE_CANDIDATE=0` 恢复 legacy——零参脚本照跑；`use
/// std.process.args` 拼写（sys 车道 stdlib 面）在 legacy 不可解析 ⇒ 非零——
/// 两形合证回退钮真的回到了 legacy 引擎。
#[test]
fn p982_rollback_knob_restores_legacy() {
    let dir = scratch("rollback");

    let zero = run(
        &dir,
        "zero.at",
        ZERO,
        &[("AUTO_ACE_CANDIDATE", "0")],
        &[],
        None,
    );
    assert!(
        zero.status.success()
            && String::from_utf8_lossy(&zero.stdout)
                .trim_end()
                .ends_with("42"),
        "the rollback knob runs legacy: {}",
        String::from_utf8_lossy(&zero.stderr)
    );

    let stdlib_args = run(
        &dir,
        "sa.at",
        USE_ARGS,
        &[("AUTO_ACE_CANDIDATE", "0")],
        &["a", "b"],
        None,
    );
    eprintln!(
        "p982 rollback use-args: ok={} err={:?}",
        stdlib_args.status.success(),
        String::from_utf8_lossy(&stdlib_args.stderr)
    );
    assert!(
        !stdlib_args.status.success(),
        "legacy cannot parse the stdlib args spelling (the sys lane's face)"
    );
    let err = String::from_utf8_lossy(&stdlib_args.stderr);
    assert!(
        err.contains("Module not found") || err.contains("not found"),
        "the legacy diagnostic names the missing module: {err}"
    );
}

/// (4) 显式形状保持：`AUTO_ACE_CANDIDATE=1`＋`AUTO_ACE_SCRIPT_SYS=1` 仍带
/// 车道标记（显式候选的可观察合同）；`=0` 回 console 候选腿（argv 具名拒）。
#[test]
fn p982_explicit_shapes_preserved() {
    let dir = scratch("explicit");

    let explicit_sys = run(
        &dir,
        "zero.at",
        ZERO,
        &[("AUTO_ACE_CANDIDATE", "1"), ("AUTO_ACE_SCRIPT_SYS", "1")],
        &[],
        None,
    );
    assert!(explicit_sys.status.success());
    assert!(
        String::from_utf8_lossy(&explicit_sys.stdout).contains("ACE sys lane"),
        "the EXPLICIT sys shape keeps its marker line"
    );

    // 旧显式形状：CANDIDATE=1 未设 SYS ⇒ console 候选腿——带参脚本仍具名拒。
    let console_shape = run(
        &dir,
        "zero.at",
        ZERO,
        &[("AUTO_ACE_CANDIDATE", "1"), ("AUTO_ACE_SCRIPT_SYS", "0")],
        &["an-arg"],
        None,
    );
    eprintln!(
        "p982 console shape with argv: ok={} err={:?}",
        console_shape.status.success(),
        String::from_utf8_lossy(&console_shape.stderr)
    );
    assert!(
        !console_shape.status.success(),
        "the console candidate leg still refuses argv by name"
    );
    assert!(
        String::from_utf8_lossy(&console_shape.stderr).contains("ace.args/unsupported"),
        "the named refusal code is preserved"
    );
}

/// (5) 命名边界：默认＋`--dump-lowered` 保持 legacy 审查面（`// lowered`
/// 头＋退出 0）；显式候选＋`--dump-lowered` 仍具名拒（A 片无回退合同）。
#[test]
fn p982_dump_lowered_boundary_by_leg() {
    let dir = scratch("dumplowered");
    std::fs::write(dir.join("d.at"), ZERO).expect("write");

    let mut default = Command::new(env!("CARGO_BIN_EXE_auto"));
    default
        .arg("--dump-lowered")
        .arg("d.at")
        .current_dir(&dir)
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS");
    let out = default.output().expect("default dump-lowered");
    eprintln!(
        "p982 dump default: ok={} out={:?}",
        out.status.success(),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        out.status.success() && String::from_utf8_lossy(&out.stdout).contains("// lowered"),
        "the DEFAULT keeps the legacy inspection surface"
    );

    let mut explicit = Command::new(env!("CARGO_BIN_EXE_auto"));
    explicit
        .arg("--dump-lowered")
        .arg("d.at")
        .current_dir(&dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_ACE_SCRIPT_SYS", "1");
    let out2 = explicit.output().expect("explicit dump-lowered");
    assert!(
        !out2.status.success(),
        "the EXPLICIT candidate still refuses dump-lowered by name"
    );
    assert!(
        String::from_utf8_lossy(&out2.stderr).contains("ace.dump-lowered/unsupported"),
        "the refusal is named: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
}
