//! PLAN-982（982/TEMPLATE，第一步）：**模板文档在 ACE 引擎下可求值**。
//!
//! 产品里模板渲染（`Templates::copy`）用的是旧 `AutoInterpreter::eval_template`，
//! 并把 `name` 作为全局注入。TEMPLATE 切片要用 ACE 替掉它，第一步先证明：
//! **同一批模板、同一份注入**（`name`）在 `config_engine` 的 ACE 引擎下能求值，
//! 且注入值真的到达文档（`name == 注入值`）。
//!
//! 语料探针里这 5 份模板是"无注入 ⇒ ACE 具名拒绝"的已知例外；本用例补上
//! **有注入**的一面。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};

use auto_lang_core::ace_config::ConfigEngineChoice;
use auto_val::{Obj, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn templates() -> Vec<PathBuf> {
    let dir = repo_root().join("crates/auto-man-assets/assets/templates");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("templates dir")
        .flatten()
        .map(|e| e.path().join("pac.at"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

#[test]
fn p982_template_documents_evaluate_under_ace_with_the_injected_name() {
    auto_lang_ace_host::install().expect("the reference bridge installs");
    let files = templates();
    assert!(
        files.len() >= 5,
        "the template corpus must keep its five families, got {}",
        files.len()
    );

    for file in &files {
        let source = std::fs::read_to_string(file).expect("template source");
        // 产品注入的全局名（`Templates::copy` 的 `interp.set_global("name", …)`）。
        let mut args = Obj::new();
        args.set("name", Value::from("probe"));
        auto_man_runtime::config_engine::set_choice_for_test(Some(ConfigEngineChoice::AceAv));
        let result = auto_man_runtime::config_engine::parse_code(&source, &args);
        auto_man_runtime::config_engine::set_choice_for_test(None);
        let config = result.unwrap_or_else(|e| {
            panic!(
                "{}: the template must evaluate under ACE: {e}",
                file.display()
            )
        });
        assert_eq!(
            config.root.get_prop_of("name"),
            Value::from("probe"),
            "{}: the injected `name` must reach the document",
            file.display()
        );
    }
}
