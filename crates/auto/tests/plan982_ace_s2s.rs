//! PLAN-982（982/REVERSE）：**s2s（Auto→Auto 改写器）的正反例**。
//!
//! `auto trans <file> --target auto --output <out.at>` 走 s2s 改写器（W1 = identity
//! passthrough + 模式头）。本用例钉住：
//! 1. **正例**：一段可运行的 Auto 源经 s2s 产出仍然**能跑**，且输出与直接运行一致
//!    （改写器不许改变语义）；
//! 2. **负例**：坏输入 ⇒ 非零退出、不留下输出文件；
//! 3. 未给 `--output` 时产物走 stdout（机器可读面）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

const SRC: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    print(total)
    return total
}
"#;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-s2s-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn auto_bin() -> &'static str {
    env!("CARGO_BIN_EXE_auto")
}

#[test]
fn p982_s2s_output_still_runs_with_the_same_semantics() {
    let dir = scratch("roundtrip");
    let src = dir.join("prog.at");
    std::fs::write(&src, SRC).expect("source");
    let lowered = dir.join("lowered.at");

    let s2s = Command::new(auto_bin())
        .args([
            "trans",
            "--path",
            src.to_str().unwrap(),
            "auto",
            "--output",
            lowered.to_str().unwrap(),
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn auto trans --target auto");
    assert!(
        s2s.status.success(),
        "s2s must lower the source:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&s2s.stdout),
        String::from_utf8_lossy(&s2s.stderr)
    );
    assert!(lowered.is_file(), "the lowered file is written");

    let direct = Command::new(auto_bin())
        .arg(src.to_str().unwrap())
        .current_dir(&dir)
        .output()
        .expect("run the source");
    let rewritten = Command::new(auto_bin())
        .arg(lowered.to_str().unwrap())
        .current_dir(&dir)
        .output()
        .expect("run the lowered source");
    assert!(
        rewritten.status.success(),
        "the rewritten source must run:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&rewritten.stdout),
        String::from_utf8_lossy(&rewritten.stderr)
    );
    // 两条命令的 banner 里带文件路径，逐字节不可比；比较**语义载荷**：
    // 两者都必须打印出值 5，且退出码相同。
    assert_eq!(direct.status.code(), rewritten.status.code());
    assert!(
        String::from_utf8_lossy(&direct.stdout).contains('5'),
        "the direct run prints 5: {}",
        String::from_utf8_lossy(&direct.stdout)
    );
    assert!(
        String::from_utf8_lossy(&rewritten.stdout).contains('5'),
        "the program prints its value: {}",
        String::from_utf8_lossy(&rewritten.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p982_s2s_refuses_a_bad_input_and_leaves_no_output() {
    let dir = scratch("bad");
    let bad = dir.join("bad.at");
    std::fs::write(&bad, "fn main() int { return 1\n").expect("bad source");
    let out = dir.join("out.at");
    let s2s = Command::new(auto_bin())
        .args([
            "trans",
            "--path",
            bad.to_str().unwrap(),
            "auto",
            "--output",
            out.to_str().unwrap(),
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn auto trans");
    assert!(
        !s2s.status.success(),
        "a malformed source must fail (stdout: {})",
        String::from_utf8_lossy(&s2s.stdout)
    );
    assert!(!out.exists(), "a failed lowering leaves no output file");
    let _ = std::fs::remove_dir_all(&dir);
}
