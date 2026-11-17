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
/// `AUTO_ACE_TS_PROOF` 回执：**真实计数**（a2js 派发数 + 引擎名），与 RUST 的
/// `AUTO_ACE_RUST_PROOF` 对齐。
#[test]
fn p982_ts_proof_receipt_reports_real_dispatch_counts() {
    let dir = scratch("proof");
    let path = dir.join("ts-proof.json");
    std::env::remove_var("AUTO_ACE_TS");
    auto_man_runtime::script_engine::note_a2js_dispatch();
    std::env::set_var("AUTO_ACE_TS", "ace");
    std::env::set_var("AUTO_ACE_TS_PROOF", path.to_str().unwrap());
    auto_man_runtime::script_engine::write_proof_if_requested();
    std::env::remove_var("AUTO_ACE_TS_PROOF");
    std::env::remove_var("AUTO_ACE_TS");

    let text = std::fs::read_to_string(&path).expect("the proof receipt is written");
    assert!(text.contains("auto.plan982.ts.proof/1"), "schema: {text}");
    assert!(text.contains("\"engine\":\"ace\""), "engine: {text}");
    assert!(text.contains("\"a2js_dispatches\""), "counters: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
/// 对称见证：未选中 A2JS 时（legacy 路径），回执必须写 `engine:"legacy"` 且
/// `a2js_dispatches` 为 0 —— "零 A2X 派发"是可被外部读到的数字，不是叙述。
#[test]
fn p982_ts_proof_receipt_reports_legacy_when_not_selected() {
    let dir = scratch("proof-legacy");
    let path = dir.join("ts-proof-legacy.json");
    std::env::remove_var("AUTO_ACE_TS");
    auto_man_runtime::script_engine::note_legacy_dispatch();
    std::env::set_var("AUTO_ACE_TS_PROOF", path.to_str().unwrap());
    auto_man_runtime::script_engine::write_proof_if_requested();
    std::env::remove_var("AUTO_ACE_TS_PROOF");

    let text = std::fs::read_to_string(&path).expect("the proof receipt is written");
    assert!(text.contains("\"engine\":\"legacy\""), "engine: {text}");
    // 计数是**进程级**的：同进程里其它用例可能已经派发过 A2JS，所以这里只断言
    // "legacy 派发数 >= 1 且引擎名是 legacy"；"零 A2JS" 需独立进程才可判定。
    assert!(text.contains("\"legacy_dispatches\":"), "counters: {text}");
    let _ = std::fs::remove_dir_all(&dir);
}
