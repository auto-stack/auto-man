//! PLAN-982（982/TEMPLATE）：**`clib` 模板的"ACE-only"是宽松，不是更强** —— 现状钉住。
//!
//! round 114 的语料证据显示 `templates/clib/pac.at`（`lib ${name} {}`）在 legacy 下
//! 被拒、在 ACE 下被接受。本用例把端到端事实钉清楚：
//! 1. `Templates::copy("clib", …)` 在 `AUTO_ACE_TEMPLATE=ace` 下**确实物化**成功；
//! 2. `name:` 这类**值位**插值被替换（"clibproj"）；
//! 3. **节点位**（`lib ${name} {}` 的节点名/实参）**保留 `${name}` 字面**——
//!    ACE 接受该写法但**不做节点位插值**；
//! 4. 因此生成物**不能再被 `Pac` 读回**（`${name}` 成了未定义引用）——这是
//!    **已知限制**，不是本用例要修的缺陷；把"接受"读成"更强"会误导。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-clib-{tag}-{}-{}",
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
fn p982_clib_template_materializes_under_ace_with_a_node_position_limitation() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let root = scratch("ace");
    let project = root.join("clibproj");
    std::fs::create_dir_all(&project).expect("project dir");

    std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    auto_man_runtime::Templates::copy("clib", project.to_str().unwrap())
        .expect("the clib template materializes under ACE");
    std::env::remove_var("AUTO_ACE_TEMPLATE");

    let pac = project.join("pac.at");
    assert!(pac.is_file(), "pac.at is materialized: {}", pac.display());
    let text = std::fs::read_to_string(&pac).expect("pac text");

    // 1. 值位插值成立。
    assert!(
        text.contains("\"clibproj\""),
        "the injected name reaches value positions: {text}"
    );
    // 2. 节点位保留字面（已知限制）。
    assert!(
        text.contains("${name}"),
        "node-position interpolation is NOT done today (the placeholder stays): {text}"
    );

    // 3. 生成物不能读回——这就是上面那条限制的直接后果，钉住它。
    let args = auto_val::Obj::new();
    let reload = auto_man_runtime::config_engine::parse_file(&pac, &args);
    let error = reload
        .err()
        .expect("the generated document is not re-readable while node positions stay literal");
    let message = error.to_string();
    assert!(
        message.contains("source.undefined") || message.contains("name"),
        "the re-read failure is named: {message}"
    );
    eprintln!("p982: re-reading the generated clib pac.at fails by name — {message}");
    let _ = std::fs::remove_dir_all(&root);
}

/// PLAN-982（用户裁定 ①B 的端到端验收）：**开关打开时**，`clib` 模板的节点位真的
/// 插值——生成的 `pac.at` **可以被读回**（上面那条默认模式的限制在开关下消失）。
#[test]
fn p982_clib_template_node_positions_interpolate_under_the_switch() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let root = scratch("switch");
    let project = root.join("clibproj");
    std::fs::create_dir_all(&project).expect("project dir");

    std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    std::env::set_var("AUTO_ACE_TEMPLATE_NODE_INTERP", "1");
    auto_man_runtime::Templates::copy("clib", project.to_str().unwrap())
        .expect("the clib template materializes under the switch");
    std::env::remove_var("AUTO_ACE_TEMPLATE_NODE_INTERP");
    std::env::remove_var("AUTO_ACE_TEMPLATE");

    let pac = project.join("pac.at");
    let text = std::fs::read_to_string(&pac).expect("pac text");
    eprintln!("p982: switched clib pac.at = {text:?}");
    assert!(
        !text.contains("${name}"),
        "with the switch on, no placeholder survives: {text}"
    );

    let args = auto_val::Obj::new();
    let config = auto_man_runtime::config_engine::parse_file(&pac, &args)
        .expect("the generated document loads under the switch");
    let pac = auto_man_runtime::Pac::new(config);
    assert_eq!(pac.name.as_str(), "clibproj");
    let _ = std::fs::remove_dir_all(&root);
}
