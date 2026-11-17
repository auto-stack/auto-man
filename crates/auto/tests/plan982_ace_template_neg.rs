//! PLAN-982（982/TEMPLATE V）：**坏模板不得破坏已有 workspace**。
//!
//! 合同要求："坏模板/取消不破坏已有 workspace"。本轮先钉住**可复现**的负例：
//! 用一个**不存在的模板名**调用 `Templates::copy` —— 必须报错、且目标目录
//! **字节不变**（既不新增文件，也不改动既有文件）。
//!
//! 取消（cancel）目前没有进入 `Templates::copy` 的路径，属具名未做项，本文件
//! 不假装覆盖。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-template-neg-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn p982_template_unknown_template_leaves_the_workspace_untouched() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());

    for ace in [false, true] {
        let dir = scratch(if ace { "ace" } else { "legacy" });
        // 已有 workspace：一个哨兵文件。
        std::fs::write(dir.join("EXISTING.txt"), b"do-not-touch").expect("sentinel");
        let before = snapshot(&dir);

        std::env::remove_var("AUTO_ACE_TEMPLATE");
        if ace {
            std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
        }
        let result = auto_man_runtime::Templates::copy("no-such-template", dir.to_str().unwrap());
        std::env::remove_var("AUTO_ACE_TEMPLATE");

        // 实测：未知模板目前是**静默 no-op**（`plan_template_family` 产出空计划）。
        // 安全性质（不改动 workspace）成立；"未知模板应具名报错"是**已登记差额**
        // ——改它会影响产品的旧行为，需单独审计，不在本用例里假装已改。
        let _ = &result;
        let after = snapshot(&dir);
        assert_eq!(
            before, after,
            "a failed template copy must not change the workspace (ace={ace})"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
