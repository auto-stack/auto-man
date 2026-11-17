//! PLAN-982（982/WEBTARGET）：**多模块 `ts` 工程具名拒绝**。
//!
//! A2JS/A2Py 的视图是单模块（`SemanticView::from_parts` 首行 `single_module`），
//! 因此多模块工程**不能**静默降级成"只翻第一个文件"。本用例钉住这条：
//! 一个 `ts` 目标带两个 `.at` 文件、`AUTO_ACE_TS=ace` ⇒ `transpile_auto` 返回
//! **具名错误** `ace.ts/project-multi-unsupported`，且 `ts/` 下**不产生**脚本。
//!
//! 若未来多模块被支持，本用例应改为"多模块产物可运行"，而不是删掉。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;

const A: &str = "fn main() int { return 1 }\n";
const B: &str = "fn helper() int { return 2 }\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-ts-multi-{tag}-{}-{}",
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
fn p982_ts_multi_module_project_is_refused_by_name() {
    // 产品的启动点会装渲染器；集成测试按同一实现安装。
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

    let dir = scratch("two");
    std::fs::write(
        dir.join("pac.at"),
        "name: \"multits\"\nrender: \"ts\"\napp(\"main\") { lang: \"ts\" }\n",
    )
    .expect("pac.at");
    std::fs::write(dir.join("a.at"), A).expect("a.at");
    std::fs::write(dir.join("b.at"), B).expect("b.at");

    let cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&dir).expect("cd project");
    std::env::set_var("AUTO_ACE_TS", "ace");
    let result = (|| -> Result<(), String> {
        let config =
            auto_man_runtime::config_engine::parse_file(&dir.join("pac.at"), &auto_val::Obj::new())
                .map_err(|e| format!("pac.at: {e}"))?;
        let mut pac = auto_man_runtime::Pac::new(config);
        pac.set_port("win32".into()).map_err(|e| e.to_string())?;
        pac.resolve().map_err(|e| e.to_string())?;
        let mut errors = Vec::new();
        for target in pac.targets.iter_mut() {
            if target.lang.as_str() == "ts" || target.lang.as_str() == "typescript" {
                match auto_man_runtime::TargetEffects::transpile_auto(target) {
                    Ok(()) => errors.push("Ok".to_string()),
                    Err(e) => errors.push(e.to_string()),
                }
            }
        }
        Err(errors.join(" | "))
    })();
    std::env::remove_var("AUTO_ACE_TS");
    std::env::set_current_dir(cwd).expect("restore cwd");

    let message = result.expect_err("the multi-module project must not silently transpile");
    assert!(
        message.contains("ace.ts/project-multi-unsupported"),
        "the refusal is named: {message}"
    );

    // 拒绝路径不得留下脚本。
    let ts_dir = dir.join("ts");
    if ts_dir.is_dir() {
        let mut stack = vec![ts_dir];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    panic!(
                        "a refused multi-module build must not write {}",
                        path.display()
                    );
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
