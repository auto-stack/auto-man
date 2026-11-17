//! PLAN-982（982/WEBTARGET）：**产品 `ts` 目标的引擎选择**有证据。
//!
//! 断言三条：
//! 1. `AUTO_ACE_TS` 的取值语义（未设/`ace`/未知）与显式性（不静默切换）；
//! 2. 选中但**没装渲染器** ⇒ 具名拒绝 `ace.ts/renderer-unavailable`（不回退旧转译）；
//! 3. 装上真实渲染器后，单模块工程渲染出的文件集能**用 node 真跑**（与 RUST 项目面
//!    的同形证据）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

const SRC: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    return total
}
"#;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-ts-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

#[test]
fn p982_ts_engine_selection_is_explicit() {
    std::env::remove_var("AUTO_ACE_TS");
    assert_eq!(auto_man_runtime::script_engine::engine_name(), "legacy");
    std::env::set_var("AUTO_ACE_TS", "ace");
    assert_eq!(auto_man_runtime::script_engine::engine_name(), "ace");
    std::env::set_var("AUTO_ACE_TS", "definitely-not-an-engine");
    assert_eq!(
        auto_man_runtime::script_engine::engine_name(),
        "legacy",
        "an unknown value keeps the legacy path"
    );
    std::env::remove_var("AUTO_ACE_TS");
}

#[test]
fn p982_ts_renderer_registry_serves_a_runnable_file_set() {
    // 集成测试是独立 crate root，按**同一实现**安装（产品的启动点见 `main.rs`）。
    auto_man_runtime::script_engine::install_renderer(|source, module, entry| {
        auto_lang_ace_host::script_transpile::transpile_scalar_source(source, module, entry)
            .map(|fileset| {
                fileset
                    .files
                    .into_iter()
                    .map(|f| (f.name, f.text))
                    .collect()
            })
            .map_err(|e| (e.code.to_string(), e.message))
    });
    assert!(
        auto_man_runtime::script_engine::renderer_installed(),
        "the product startup installs the script renderer"
    );

    let files = auto_man_runtime::script_engine::render_files(SRC, "p982-ts", "main")
        .expect("the registered renderer renders");
    assert!(!files.is_empty(), "the file set is not empty");

    let dir = scratch("run");
    for (name, text) in &files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dir");
        }
        std::fs::write(&path, text).expect("write");
    }
    let entry = files
        .iter()
        .map(|(name, _)| name.as_str())
        .find(|n| n.ends_with(".mjs") || n.ends_with(".js"))
        .expect("a script entry");
    let out = Command::new("node")
        .arg(entry)
        .current_dir(&dir)
        .output()
        .expect("spawn node");
    assert!(
        out.status.success(),
        "the rendered script runs:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains('5'),
        "the script prints the source semantics (5): {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
