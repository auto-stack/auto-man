//! PLAN-982（982/TEMPLATE I）：**模板脚手架的两条引擎路径**。
//!
//! `Templates::copy` 逐文件物化模板：`pac.at` 是配置文档、其余是文本模板。
//! 本用例证明：
//! 1. 默认（未选 `AUTO_ACE_TEMPLATE`）：行为不变，生成的工程树与文件内容可用；
//! 2. `AUTO_ACE_TEMPLATE=ace`：`pac.at` 由 **ACE 引擎**求值，`name` 注入仍到达
//!    文档（`Pac` 投影 `name == 目录名`），且**文件清单与默认路径一致**（不丢文件）；
//! 3. 两条路径的 `pac.at` 都能被产品 `Pac` 消费（同一消费面）。
//!
//! 引擎选择是进程级环境变量：本文件用一把互斥锁串行化。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-template-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn files_of(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn generate(tag: &str, ace: bool) -> (PathBuf, Vec<String>, String) {
    let dir = scratch(tag);
    let project = dir.join("app");
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    if ace {
        std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    }
    auto_man_runtime::Templates::copy("app", project.to_str().unwrap())
        .unwrap_or_else(|e| panic!("template copy ({ace}) failed: {e}"));
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    let files = files_of(&project);
    let pac = std::fs::read_to_string(project.join("pac.at")).expect("pac.at");
    (project, files, pac)
}

#[test]
fn p982_template_copy_switches_the_pac_evaluator_only() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let (legacy_dir, legacy_files, legacy_pac) = generate("legacy", false);
    let (ace_dir, ace_files, ace_pac) = generate("ace", true);

    // 1. 文件清单一致（ACE 路径不丢文件、不多文件）。
    assert_eq!(
        legacy_files, ace_files,
        "the two engines must materialize the same project tree"
    );
    assert!(!ace_files.is_empty(), "the template writes files");

    // 2. `name` 注入在两份 pac.at 里都到达文档。
    for (label, pac) in [("legacy", &legacy_pac), ("ace", &ace_pac)] {
        assert!(
            pac.contains("app"),
            "{label}: the injected name must appear in the generated pac.at: {pac}"
        );
    }

    // 3. 两份 pac.at 都能被产品 `Pac` 消费（同一消费面）。
    for (label, dir) in [("legacy", &legacy_dir), ("ace", &ace_dir)] {
        let args = auto_val::Obj::new();
        let config = auto_man_runtime::config_engine::parse_file(&dir.join("pac.at"), &args)
            .unwrap_or_else(|e| panic!("{label}: the generated pac.at must load: {e}"));
        let pac = auto_man_runtime::Pac::new(config);
        assert_eq!(
            pac.name.as_str(),
            "app",
            "{label}: the project name comes from the directory"
        );
    }

    let _ = std::fs::remove_dir_all(&legacy_dir);
    let _ = std::fs::remove_dir_all(&ace_dir);
}
