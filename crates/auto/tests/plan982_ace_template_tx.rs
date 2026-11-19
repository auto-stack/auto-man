//! PLAN-982（982/TEMPLATE · round 202，排程令并行项）：**取消通道＋写盘中断
//! 回滚**——事务式写盘段（`materialize_transactional`，`copy`/`copy_user`
//! 共用）的验收：
//!
//! 1. **写盘中断回滚**：最后一个产物位被占（目录阻断 `File::create`）⇒ 具名
//!    `ace.template/write` 失败 ⇒ 已有工作区**逐字节不变**（先写下的既有文件
//!    按原字节还原、本 run 新建文件全无、无关文件原样）——无半成品；
//! 2. **取消通道**：`AUTO_ACE_TEMPLATE_CANCEL_AFTER_MS=0`（立即取消语义）⇒
//!    具名 `ace.template/cancelled` ⇒ 目标目录不变；
//! 3. 正控：同一夹具无阻断/取消 ⇒ 物化成功（非空证）。

#![cfg(feature = "ace-candidate")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-tpl-tx-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// 相对路径 → 文件字节的快照（"已有工作区字节不变"的判据）。
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
                if let Ok(rel) = path.strip_prefix(root) {
                    out.insert(rel.to_string_lossy().replace('\\', "/"), bytes);
                }
            }
        }
    }
    walk(root, root, &mut out);
    out
}

/// `app` 族的产物相对路径清单（清洁目录先跑一次 `copy` 枚举）。
fn app_family_outputs() -> Vec<String> {
    let root = scratch("enum");
    let project = root.join("app");
    std::fs::create_dir_all(&project).expect("project");
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    std::env::remove_var(auto_man_runtime::asset::TEMPLATE_CANCEL_ENV);
    auto_man_runtime::asset::Templates::copy("app", project.to_str().unwrap())
        .expect("the clean enumeration copy succeeds");
    let snap = snapshot(&project);
    let _ = std::fs::remove_dir_all(&root);
    snap.keys().cloned().collect()
}

/// (1) 写盘中断回滚：末位阻断 ⇒ 具名失败 ⇒ 工作区逐字节不变。
#[test]
fn p982_template_write_failure_rolls_back_byte_identical() {
    let _guard = LOCK.lock().unwrap();
    let outputs = app_family_outputs();
    assert!(outputs.len() >= 2, "the app family plans several files");
    let first = outputs[0].clone();
    let last = outputs[outputs.len() - 1].clone();

    let root = scratch("rollback");
    let project = root.join("app");
    std::fs::create_dir_all(&project).expect("project");
    // 既有工作区：首个产物位放**自定义原字节**（覆盖前快照还原的靶子）＋
    // 一个无关文件；末位产物位放**目录**（阻断 `File::create`）。
    std::fs::write(project.join(&first), b"ORIGINAL-BYTES-KEEP").expect("pre-existing");
    std::fs::write(project.join("unrelated.txt"), b"keep-me").expect("unrelated");
    std::fs::create_dir_all(project.join(&last)).expect("blocker dir at the last output");
    let before = snapshot(&project);

    std::env::remove_var("AUTO_ACE_TEMPLATE");
    std::env::remove_var(auto_man_runtime::asset::TEMPLATE_CANCEL_ENV);
    let err = auto_man_runtime::asset::Templates::copy("app", project.to_str().unwrap())
        .expect_err("the blocked write must fail");
    eprintln!("p982 tpl-tx write failure: {err:?}");
    let text = format!("{err:?}");
    assert!(
        text.contains("ace.template/write"),
        "the failure is named: {text}"
    );

    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "the workspace is byte-identical after the rolled-back write (no half products)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// (2) 取消通道：`AUTO_ACE_TEMPLATE_CANCEL_AFTER_MS=0`（立即取消）⇒ 具名拒
/// ⇒ 目标目录不变。
#[test]
fn p982_template_cancel_channel_refuses_and_keeps_workspace() {
    let _guard = LOCK.lock().unwrap();
    let root = scratch("cancel");
    let project = root.join("app");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("pre-existing.at"), b"do-not-touch").expect("pre");
    let before = snapshot(&project);

    std::env::set_var(auto_man_runtime::asset::TEMPLATE_CANCEL_ENV, "0");
    let result = auto_man_runtime::asset::Templates::copy("app", project.to_str().unwrap());
    std::env::remove_var(auto_man_runtime::asset::TEMPLATE_CANCEL_ENV);
    let err = result.expect_err("the immediate cancel must refuse");
    eprintln!("p982 tpl-tx cancel: {err:?}");
    let text = format!("{err:?}");
    assert!(
        text.contains("ace.template/cancelled"),
        "the cancellation is named: {text}"
    );
    assert_eq!(
        before,
        snapshot(&project),
        "a cancelled materialization leaves the workspace untouched"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// (3) 正控：同一夹具无阻断/取消 ⇒ 物化成功（非空证）。
#[test]
fn p982_template_transactional_success_path_still_materializes() {
    let _guard = LOCK.lock().unwrap();
    let root = scratch("positive");
    let project = root.join("app");
    std::fs::create_dir_all(&project).expect("project");
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    std::env::remove_var(auto_man_runtime::asset::TEMPLATE_CANCEL_ENV);
    auto_man_runtime::asset::Templates::copy("app", project.to_str().unwrap())
        .expect("the unblocked transactional copy succeeds");
    let snap = snapshot(&project);
    assert!(!snap.is_empty(), "the family materializes for real");
    assert!(
        snap.keys().any(|k| k.ends_with("pac.at")),
        "the generated tree carries pac.at"
    );

    let _ = std::fs::remove_dir_all(&root);
}
