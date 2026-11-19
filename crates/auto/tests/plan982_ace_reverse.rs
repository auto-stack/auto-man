//! PLAN-982（982/REVERSE，第一步）：**r2a 的"Rust→Auto→真实运行"链路**。
//!
//! 合同 V："原支持 Rust 输入 → Auto 真实运行；非法近邻错误/exit"。本用例用仓库
//! 既有的 `test/r2a/01_basics/001_hello.rs`（受支持的输入）走**产品 CLI**：
//! `auto r2a <in.rs> --output <out.at>` → `auto <out.at>`，断言打印 `hello, world!`；
//! 再用一个**语法近邻的坏输入**断言非零退出且不产出可运行文件。
//!
//! 边界：本用例只覆盖"已支持的子集 + 一个坏邻居"，不要求任意互逆。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-reverse-{tag}-{}-{}",
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
fn p982_reverse_r2a_runs_the_generated_auto() {
    let fixture = repo_root().join("../auto-lang/crates/auto-lang/test/r2a/01_basics/001_hello.rs");
    assert!(
        fixture.is_file(),
        "the r2a fixture exists: {}",
        fixture.display()
    );

    let dir = scratch("hello");
    let out_at = dir.join("hello.at");
    let r2a = Command::new(auto_bin())
        .args([
            "r2a",
            fixture.to_str().unwrap(),
            "--output",
            out_at.to_str().unwrap(),
        ])
        .output()
        .expect("spawn auto r2a");
    assert!(
        r2a.status.success(),
        "r2a must convert the supported input:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&r2a.stdout),
        String::from_utf8_lossy(&r2a.stderr)
    );
    let generated = std::fs::read_to_string(&out_at).expect("the generated Auto file");
    assert!(
        generated.contains("fn main()"),
        "generated Auto: {generated}"
    );

    // 生成物必须**真的能跑**（这是 V 的核心，不是文本比对）。
    // PLAN-982 round 199/202：r2a 产物是 **legacy 方言**形（`fn main() {`
    // 无返回注解）——sys 车道入口冻结面要求注解 ⇒ 默认 sys 腿对其具名拒；
    // 本腿钉**显式 legacy 引擎**跑（r2a 语义面＝legacy VM；sys 面对 r2a
    // 形状的承接属后续切片）。
    let run = Command::new(auto_bin())
        .arg(out_at.to_str().unwrap())
        .current_dir(&dir)
        .env("AUTO_ACE_CANDIDATE", "0")
        .output()
        .expect("spawn auto <generated>");
    assert!(
        run.status.success(),
        "the generated Auto must run:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("hello, world!"),
        "the generated program prints the Rust program's output: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p982_reverse_r2a_refuses_a_near_miss_input() {
    let dir = scratch("bad");
    let bad = dir.join("bad.rs");
    // 语法近邻：函数体不闭合 + 类型位置乱写。
    std::fs::write(&bad, "fn main( { let x: = 3; }\n").expect("bad input");
    let out_at = dir.join("bad.at");
    let r2a = Command::new(auto_bin())
        .args([
            "r2a",
            bad.to_str().unwrap(),
            "--output",
            out_at.to_str().unwrap(),
        ])
        .output()
        .expect("spawn auto r2a");
    assert!(
        !r2a.status.success(),
        "an invalid near-neighbour must not succeed (stderr: {})",
        String::from_utf8_lossy(&r2a.stderr)
    );
    assert!(
        !out_at.exists(),
        "a failed conversion must not leave a generated file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
