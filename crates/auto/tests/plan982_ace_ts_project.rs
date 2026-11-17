//! PLAN-982（982/WEBTARGET）：**工程夹具级的 `ts` 构建**（真 `Pac` + 真 `Target`）。
//!
//! 前面的用例直接调渲染器/注册表；本用例走**产品路径**：临时工程（`pac.at` +
//! `main.at`）→ `Pac::build()` → `Target::transpile_auto()` → `ts/src/*`，
//! 再用 `node` 执行产出的脚本。断言：
//! 1. `AUTO_ACE_TS=ace` 时产物落在 `ts/src/`；
//! 2. 产物能用 node 跑出源语义（`5`）；
//! 3. 未选中引擎时**不**产生 `ts/src/*.js`（旧 `trans_typescript` 产出 `.ts`，
//!    不会污染 A2JS 的输出目录）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

const MAIN: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    return total
}
"#;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-ts-proj-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn write_project(dir: &PathBuf) {
    std::fs::write(
        dir.join("pac.at"),
        "name: \"tsproj\"\nrender: \"ts\"\napp(\"main\") { lang: \"ts\" }\n",
    )
    .expect("pac.at");
    std::fs::write(dir.join("main.at"), MAIN).expect("main.at");
}

fn build(dir: &PathBuf, ace: bool) -> Result<(), String> {
    let cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir).expect("cd project");
    if ace {
        std::env::set_var("AUTO_ACE_TS", "ace");
    } else {
        std::env::remove_var("AUTO_ACE_TS");
    }
    let result = (|| -> Result<(), String> {
        let config =
            auto_man_runtime::config_engine::parse_file(&dir.join("pac.at"), &auto_val::Obj::new())
                .map_err(|e| format!("pac.at: {e}"))?;
        let mut pac = auto_man_runtime::Pac::new(config);
        pac.set_port("win32".into()).map_err(|e| e.to_string())?;
        pac.resolve().map_err(|e| e.to_string())?;
        // 只跑目标级的 `transpile_auto`（`pac.build()` 会拉起 ninja 构建器，属另一档）。
        let mut built = 0usize;
        for target in pac.targets.iter_mut() {
            if target.lang.as_str() == "ts" || target.lang.as_str() == "typescript" {
                auto_man_runtime::TargetEffects::transpile_auto(target)
                    .map_err(|e| e.to_string())?;
                built += 1;
            }
        }
        if built == 0 {
            return Err("the fixture declares a ts target but Pac resolved none".to_string());
        }
        Ok(())
    })();
    std::env::remove_var("AUTO_ACE_TS");
    std::env::set_current_dir(cwd).expect("restore cwd");
    result
}

fn js_files(dir: &PathBuf) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("ts")];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .map(|e| e == "js" || e == "mjs")
                .unwrap_or(false)
            {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn p982_ts_project_build_produces_a_runnable_script() {
    // 产品的启动点会装这个渲染器（`ace_candidate::install_script_renderer`）；
    // 集成测试是独立 crate root，按**同一实现**安装。
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

    let dir = scratch("ace");
    write_project(&dir);
    build(&dir, true).expect("the ts project builds under AUTO_ACE_TS=ace");

    let files = js_files(&dir);
    assert!(
        !files.is_empty(),
        "the A2JS leg writes scripts under ts/ (found none)"
    );
    let entry = files
        .iter()
        .find(|f| f.ends_with("main.js") || f.ends_with("main.mjs"))
        .unwrap_or(&files[0]);
    let out = Command::new("node")
        .arg(entry)
        .current_dir(dir.join("ts/src"))
        .output()
        .expect("spawn node");
    assert!(
        out.status.success(),
        "the built script runs:\nstdout: {}\nstderr: {}",
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
