//! PLAN-982（982/TEMPLATE V）：**独立用户模板同样运行**。
//!
//! `auto new` 的 `--template` 以前只认**嵌入族**（`app`/`clib`/…）。本用例证明
//! 新增的**用户模板目录**路径：`template` 指向一个目录时，按其内容逐文件渲染
//! （`${name}` 替换；`pac.at` 在 `AUTO_ACE_TEMPLATE=ace` 时走 ACE），产物能被
//! 产品 `Pac` 消费。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-user-template-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn write_user_template(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).expect("template dir");
    std::fs::write(
        dir.join("pac.at"),
        "name: \"${name}\"\nversion: \"0.2.0\"\n",
    )
    .expect("pac");
    std::fs::write(dir.join("README.md"), "project ${name}\n").expect("readme");
    std::fs::write(
        dir.join("src").join("main.at"),
        "fn main() int { return 1 }\n",
    )
    .expect("main");
}

#[test]
fn p982_user_template_directory_is_rendered_and_consumable() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let root = scratch("ok");
    let template = root.join("my-template");
    write_user_template(&template);
    let project = root.join("proj");

    std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    auto_man_runtime::automan::Automan::create_by_template(
        project.to_str().unwrap(),
        template.to_str().unwrap(),
    )
    .expect("the user template renders");
    std::env::remove_var("AUTO_ACE_TEMPLATE");

    // 1. 结构保留：模板的每个相对路径都出现（`name` 占位符按工程名替换）。
    for rel in ["pac.at", "README.md", "src/main.at"] {
        assert!(
            project.join(rel).is_file(),
            "the user template must materialize `{rel}`"
        );
    }

    // 2. 渲染：`${name}` 在 pac.at 与文本文件里都被替换。
    let pac = std::fs::read_to_string(project.join("pac.at")).expect("pac");
    assert!(
        pac.contains("proj"),
        "pac.at carries the project name: {pac}"
    );
    let readme = std::fs::read_to_string(project.join("README.md")).expect("readme");
    assert_eq!(readme.trim(), "project proj");

    // 3. 消费：生成的 pac.at 能被产品 `Pac` 读取。
    let args = auto_val::Obj::new();
    let config = auto_man_runtime::config_engine::parse_file(&project.join("pac.at"), &args)
        .expect("the generated pac.at loads");
    let pac = auto_man_runtime::Pac::new(config);
    assert_eq!(pac.name.as_str(), "proj");
    assert!(pac.version.as_str().contains("0.2.0"));

    let _ = std::fs::remove_dir_all(&root);
}
/// **事务式物化**：坏模板（`pac.at` 在 ACE 下解析失败）必须在**落盘前**失败，
/// 目标目录连一个新文件都不能多（round 62 的实现边走边写，会留半成品）。
#[test]
fn p982_user_template_failure_writes_nothing() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    auto_lang_ace_host::install().expect("bridge");

    let root = scratch("bad");
    let template = root.join("bad-template");
    std::fs::create_dir_all(&template).expect("template dir");
    // 合法文本文件（若边走边写，会先被写出去）。
    std::fs::write(template.join("README.md"), "project ${name}\n").expect("readme");
    // 非法配置文档：`${` 不闭合，ACE 解析期就失败。
    std::fs::write(template.join("pac.at"), "name: \"${oops\"\n").expect("pac");

    let project = root.join("proj");
    std::env::set_var("AUTO_ACE_TEMPLATE", "ace");
    let result = auto_man_runtime::automan::Automan::create_by_template(
        project.to_str().unwrap(),
        template.to_str().unwrap(),
    );
    std::env::remove_var("AUTO_ACE_TEMPLATE");
    assert!(result.is_err(), "a bad template must fail");

    // 目标目录要么不存在，要么为空：不得有半成品。
    match std::fs::read_dir(&project) {
        Ok(entries) => {
            let files: Vec<_> = entries.flatten().map(|e| e.file_name()).collect();
            assert!(
                files.is_empty(),
                "a failed template must not materialize files: {files:?}"
            );
        }
        Err(_) => {}
    }
    let _ = std::fs::remove_dir_all(&root);
}
