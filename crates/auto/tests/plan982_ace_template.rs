//! PLAN-982（982/TEMPLATE I）：`Templates::copy` 的两条引擎路径。
//!
//! 模板族名取自资产现值（`auto-man-assets` 的 `plan982_template_family` 用例钉住
//! 了 `app` 等五个族确实能被 `plan_template_family` 计划出来）。本用例证明：
//! 1. 默认与 `AUTO_ACE_TEMPLATE=ace` 生成**同一文件清单**；
//! 2. `name` 注入到达生成的 `pac.at`；
//! 3. 两份 `pac.at` 都能被产品 `Pac` 消费。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
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

fn files_of(base: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_of(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn find_pac(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().map(|n| n == "pac.at").unwrap_or(false) {
                return Some(path);
            }
        }
    }
    None
}

fn generate(tag: &str, ace: bool) -> (PathBuf, PathBuf, Vec<String>) {
    let root = scratch(tag);
    let project = root.join("app");
    std::fs::create_dir_all(&project).expect("project dir");
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    if ace {
        std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    }
    auto_man_runtime::Templates::copy("app", project.to_str().unwrap())
        .unwrap_or_else(|e| panic!("template copy ({ace}) failed: {e}"));
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    let mut files = Vec::new();
    files_of(&root, &root, &mut files);
    files.sort();
    let pac = find_pac(&root).expect("the template materializes a pac.at");
    (root, pac, files)
}

#[test]
fn p982_template_copy_switches_the_pac_evaluator_only() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let (legacy_root, legacy_pac, legacy_files) = generate("legacy", false);
    let (ace_root, ace_pac, ace_files) = generate("ace", true);

    assert!(!ace_files.is_empty(), "the template writes files");
    assert_eq!(
        legacy_files, ace_files,
        "the two engines must materialize the same project tree"
    );

    for (label, pac) in [("legacy", &legacy_pac), ("ace", &ace_pac)] {
        let text = std::fs::read_to_string(pac).expect("pac text");
        assert!(
            text.contains("app"),
            "{label}: the injected name must appear in the generated pac.at: {text}"
        );
    }

    for (label, pac) in [("legacy", &legacy_pac), ("ace", &ace_pac)] {
        let args = auto_val::Obj::new();
        let config = auto_man_runtime::config_engine::parse_file(pac, &args)
            .unwrap_or_else(|e| panic!("{label}: the generated pac.at must load: {e}"));
        let pac = auto_man_runtime::Pac::new(config);
        assert!(
            !pac.name.as_str().is_empty(),
            "{label}: the project name is projected from the generated document"
        );
    }

    let _ = std::fs::remove_dir_all(&legacy_root);
    let _ = std::fs::remove_dir_all(&ace_root);
}
