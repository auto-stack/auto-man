//! PLAN-982（982/RUST V）：A2R 产物**冷构建 + 实跑**。
//!
//! V-CFG 的 Rust 对应项要求"未修改的生成 Rust 冷构建运行，与源 expected 一致"。
//! 本文件不碰产品路径（`AUTO_ACE_RUST` 已有独立用例），只断言**驱动产物本身**：
//! 1. 宿主驱动产出的三件套写盘后，`rustc --edition 2021 main.rs` **直接**能编译
//!    （没有隐藏依赖、没有额外 crate）；
//! 2. 运行结果与源语义一致（`add(2, 3)` ⇒ `value:5`）——不是"能编译"就算过；
//! 3. 两次冷构建的产物行为一致（确定性在二进制层面的复核）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

use auto_lang_ace_host::rust_transpile::{
    transpile_scalar_source, transpile_with_profile, TranspileProfile,
};

const SRC: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    return total
}
"#;

fn rustc() -> PathBuf {
    let mut cmd = Command::new("rustc");
    cmd.arg("--version");
    let out = cmd.output();
    match out {
        Ok(o) if o.status.success() => PathBuf::from("rustc"),
        _ => panic!("rustc is required for the A2R cold-build gate and was not found"),
    }
}

fn render_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-a2r-cold-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fileset = transpile_scalar_source(SRC, "p982-cold", "main").expect("rendered");
    for file in &fileset.files {
        std::fs::write(dir.join(&file.name), &file.text).expect("write generated file");
    }
    dir
}

#[test]
fn p982_a2r_fileset_builds_cold_and_runs() {
    let dir = render_dir("build");
    let exe = dir.join(if cfg!(windows) { "probe.exe" } else { "probe" });
    let compile = Command::new(rustc())
        .args(["--edition", "2021", "main.rs", "-o"])
        .arg(&exe)
        .current_dir(&dir)
        .output()
        .expect("run rustc");
    assert!(
        compile.status.success(),
        "the generated file set must compile as-is:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&exe).current_dir(&dir).output().expect("run");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    assert!(
        run.status.success(),
        "the cold-built program must run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        stdout.contains("value:5"),
        "`add(2, 3)` must produce `value:5`, got: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p982_a2r_cold_build_is_reproducible() {
    let mut observed = Vec::new();
    for tag in ["a", "b"] {
        let dir = render_dir(tag);
        let exe = dir.join(if cfg!(windows) { "probe.exe" } else { "probe" });
        let compile = Command::new(rustc())
            .args(["--edition", "2021", "main.rs", "-o"])
            .arg(&exe)
            .current_dir(&dir)
            .output()
            .expect("run rustc");
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&exe).current_dir(&dir).output().expect("run");
        observed.push(String::from_utf8_lossy(&run.stdout).to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(
        observed[0], observed[1],
        "two cold builds of the same source must behave identically"
    );
}
/// 家族覆盖：records 家族的产物同样必须**原样冷构建并运行**（不是只有 scalar
/// 能编译）。`main` 返回常量 7 ⇒ 运行输出 `value:7`。
#[test]
fn p982_a2r_records_family_builds_cold_and_runs() {
    let dir = std::env::temp_dir().join(format!(
        "p982-a2r-records-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let fileset = transpile_with_profile(
        "type point { x int; y int }\n\nfn main() int {\n    return 7\n}\n",
        "p982-records",
        "main",
        TranspileProfile::Records,
    )
    .expect("records render");
    for file in &fileset.files {
        std::fs::write(dir.join(&file.name), &file.text).expect("write");
    }
    let exe = dir.join(if cfg!(windows) { "probe.exe" } else { "probe" });
    let compile = Command::new(rustc())
        .args(["--edition", "2021", "main.rs", "-o"])
        .arg(&exe)
        .current_dir(&dir)
        .output()
        .expect("rustc");
    assert!(
        compile.status.success(),
        "the records file set must compile cold:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&exe).current_dir(&dir).output().expect("run");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    assert!(run.status.success());
    assert!(
        stdout.contains("value:7"),
        "records entry returning 7 must print value:7, got {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
