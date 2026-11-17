//! PLAN-982（982/RUST，项目面）：**渲染器注册链**的端到端证据。
//!
//! 产品 `auto build`（rust 目标）在候选引擎下走 `auto-man-runtime` 的渲染器
//! 注册表：`auto` 启动时把宿主 A2R 驱动装进运行时，`Target::transpile_auto`
//! 单模块工程时调用它并把三件套写进 `rust/src/`。本文件钉住两件事：
//! 1. 注册表确实装上了真实驱动（`renderer_installed()` 为真）；
//! 2. 经注册表渲染出来的文件**能独立冷构建并运行**（不是"写出来就算过"）。

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

#[test]
fn p982_rust_renderer_registry_serves_the_project_path() {
    // 产品的启动点（`ace_candidate::install_rust_renderer`）装的就是这条闭包；
    // 集成测试是独立 crate root，因此这里按**同一实现**直接安装。
    auto_man_runtime::rust_engine::install_renderer(|source, module, entry| {
        auto_lang_ace_host::rust_transpile::transpile_scalar_source(source, module, entry)
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
        auto_man_runtime::rust_engine::renderer_installed(),
        "the product startup installs the A2R renderer"
    );

    let files = auto_man_runtime::rust_engine::render_files(SRC, "p982-proj", "main")
        .expect("the registered renderer renders");
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["main.rs", "a2x_runtime.rs", "generated.rs"]);

    // 冷构建 + 实跑：写盘后只用 `rustc` 编译，跑出源语义。
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "p982-renderer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (name, text) in &files {
        std::fs::write(dir.join(name), text).expect("write");
    }
    let exe = dir.join(if cfg!(windows) { "probe.exe" } else { "probe" });
    let compile = Command::new("rustc")
        .args(["--edition", "2021", "main.rs", "-o"])
        .arg(&exe)
        .current_dir(&dir)
        .output()
        .expect("rustc");
    assert!(
        compile.status.success(),
        "the rendered project must compile cold:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&exe).current_dir(&dir).output().expect("run");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("value:5"),
        "the rendered program must run the source semantics"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
