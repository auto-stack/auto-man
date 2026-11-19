//! PLAN-982（982/RUST · round 200，排程令第 1 项）：**非 UI 多模块 A2R 工程
//! 写盘**——`auto rust <入口.at>` 在工程形态（同目录兄弟 `.at` ＞1，legacy
//! `transpile_rust_project` 同一发现语义）下经**包级车道**（packages 前门＋
//! `RustTargetProvider`）整工程一次编译成完整 file set，写入 `rust/src/`；
//! 从**干净目录**冷构建、运行，值 oracle 对拍源语义；失败保旧输出（字节
//! 快照不变）；旧 `RustTrans` 零派发。
//!
//! 单文件打印面（`auto rust <file>` 无兄弟）由 `plan982_ace_rust_b` 持有；
//! Target 管线分支（桌面/`--gen-only` 面）服务 1074 联合验收。Iced 工程另留。

#![cfg(feature = "ace-candidate")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-a2r-multi-{}-{}-{}",
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

const UTIL: &str = "pub fn twice(n int) int { return n * 2 }\n";
const MAIN: &str = "use util.twice\nfn main() int { return twice(21) }\n";

/// 工程形态调用：`auto rust main.at`（cwd＝工程根；`rust/src` 落在 cwd）。
fn gen(dir: &Path, proof: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auto"))
        .args(["rust", "main.at"])
        .current_dir(dir)
        .env("AUTO_ACE_RUST", "ace")
        .env("AUTO_ACE_RUST_PROOF", proof.as_os_str())
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS")
        .output()
        .expect("run auto rust (project form)")
}

/// 相对路径 → 文件字节的快照（"保旧输出"的字节级判据）。
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path
                    .strip_prefix(root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                out.insert(rel, bytes);
            }
        }
    }
    walk(root, root, &mut out);
    out
}

/// (1) 核心腿：写盘 → 干净目录冷构建 → 运行（跨模块语义）→ 零旧派发收据。
#[test]
fn p982_rust_multifile_gen_cold_build_and_run() {
    let dir = scratch("core");
    std::fs::write(dir.join("main.at"), MAIN).expect("main.at");
    std::fs::write(dir.join("util.at"), UTIL).expect("util.at");
    let proof = dir.join("proof.json");

    let out = gen(&dir, &proof);
    eprintln!(
        "p982 multi gen: ok={} out={:?} err={:?}",
        out.status.success(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "the multi-module gen must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("[a2r] project form"),
        "the project form announces itself: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let rust_src = dir.join("rust/src");
    assert!(rust_src.join("main.rs").exists(), "the wrapper is written");
    assert!(
        rust_src.join("generated.rs").exists(),
        "the generated module"
    );
    assert!(rust_src.join("a2x_runtime.rs").exists(), "the runtime");

    // 零旧派发收据：a2r 真实计数、legacy 0。
    let receipt = std::fs::read_to_string(&proof).expect("proof written");
    let receipt: serde_json::Value = serde_json::from_str(&receipt).expect("proof json");
    eprintln!("p982 multi proof: {receipt:#?}");
    assert_eq!(receipt["engine"], "a2r", "the A2R engine served the gen");
    assert!(
        receipt["a2r_dispatches"].as_u64().unwrap_or(0) >= 1,
        "the package dispatch is counted"
    );
    assert_eq!(
        receipt["legacy_dispatches"], 0,
        "zero legacy RustTrans dispatches"
    );

    // 干净目录冷构建：把生成的工程拷进全新目录（证明自包含），rustc 直编。
    let clean = scratch("clean");
    let clean_src = clean.join("rust/src");
    copy_tree(&rust_src, &clean_src);
    let exe = clean.join("app.exe");
    let build = Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(clean_src.join("main.rs"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("rustc");
    assert!(
        build.status.success(),
        "the generated project cold-builds from a clean dir:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    eprintln!("p982 multi run: {stdout:?}");
    assert!(
        stdout.contains("value:42"),
        "the cross-module semantics run (twice(21)): {stdout:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&clean);
}

/// (2) 失败保旧输出：坏 util.at ⇒ 具名拒＋`rust/src` 字节快照不变（无半成品）。
#[test]
fn p982_rust_multifile_failure_keeps_previous_output() {
    let dir = scratch("fail");
    std::fs::write(dir.join("main.at"), MAIN).expect("main.at");
    std::fs::write(dir.join("util.at"), UTIL).expect("util.at");
    let proof = dir.join("proof.json");

    let good = gen(&dir, &proof);
    assert!(good.status.success(), "the good gen must succeed first");
    let before = snapshot(&dir.join("rust/src"));
    assert!(!before.is_empty(), "the first gen wrote a real tree");

    // 坏第二模块：语法错。
    std::fs::write(dir.join("util.at"), "pub fn twice(n int { return 1 }\n").expect("break util");
    let bad = gen(&dir, &proof);
    eprintln!(
        "p982 multi fail: ok={} err={:?}",
        bad.status.success(),
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(
        !bad.status.success(),
        "the broken module must refuse the gen"
    );
    let err = String::from_utf8_lossy(&bad.stderr).to_string();
    assert!(
        err.contains("ace.rust/compile"),
        "the refusal is named: {err}"
    );

    let after = snapshot(&dir.join("rust/src"));
    assert_eq!(
        before, after,
        "a failed gen keeps the previous output byte-for-byte (no half-written tree)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// (3) 单文件面不受工程形态影响：无兄弟 ⇒ 既有打印面（generated.rs 到
/// stdout，不写 rust/src）。
#[test]
fn p982_rust_multifile_single_file_print_face_unchanged() {
    let dir = scratch("single");
    std::fs::write(dir.join("solo.at"), "fn main() int { return 9 }\n").expect("solo.at");
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args(["rust", "solo.at"])
        .current_dir(&dir)
        .env("AUTO_ACE_RUST", "ace")
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS")
        .output()
        .expect("run auto rust (single)");
    eprintln!(
        "p982 single: ok={} out={:?}",
        out.status.success(),
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.status.success());
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("[a2r] project form"),
        "a lone file keeps the print face"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("fn_main"),
        "the print face still prints the lowered program"
    );
    assert!(
        !dir.join("rust/src").exists(),
        "the single-file face writes no project tree"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("dst");
    for entry in std::fs::read_dir(src).expect("read src").flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy");
        }
    }
}
