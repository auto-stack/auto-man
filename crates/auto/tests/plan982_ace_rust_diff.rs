//! PLAN-982（982/RUST）：**A2R 与旧转译的差异清单**——把"两者不是同一个生成器"
//! 这件事钉成可核对的断言，而不是叙述。
//!
//! 清单（本文件里每条都有断言）：
//! 1. **命名**：旧转译保留源名（`fn main`），A2R 降级为 `fn_main`；
//! 2. **运行时**：A2R 的产物自带 `a2x_runtime`（旧转译依赖外部运行时 crate）；
//! 3. **单文件通道**：A2R 打印的是 `generated.rs`（入口包装在 fileset 的\n//!    `main.rs` 里，`--merge` 用例已覆盖其可构建性）；
//! 4. **体量**：A2R 单文件输出显著大于旧转译（三件套其中一件即含运行时），
//!    因此"逐字节等价"不成立——可比较的是**语义**（由冷构建实跑门负责）。
//!
//! 这是"差异面登记"的永久面：任何一条变化都会让本文件红。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

fn auto_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_auto"))
}

struct Project {
    dir: PathBuf,
    script: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn project(tag: &str, source: &str) -> Project {
    let dir = std::env::temp_dir().join(format!(
        "p982-diff-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp project");
    let script = dir.join("lib.at");
    std::fs::write(&script, source).expect("write script");
    Project { dir, script }
}

fn render(project: &Project, a2r: bool) -> String {
    let mut cmd = Command::new(auto_exe());
    cmd.args(["rust", project.script.to_str().unwrap()])
        .current_dir(&project.dir)
        .env_remove("AUTO_ACE_RUST")
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_RUST_PROOF");
    if a2r {
        cmd.env("AUTO_ACE_RUST", "ace");
    }
    let out = cmd.output().expect("run auto rust");
    assert!(
        out.status.success(),
        "render failed (a2r={a2r}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

const SRC: &str = r#"
fn add(a int, b int) int { return a + b }

fn main() int {
    var total = add(2, 3)
    return total
}
"#;

/// 四条差异，逐条断言。任何一条不成立都说明"差异清单"过期了。
#[test]
fn p982_rust_engines_differ_only_in_the_registered_ways() {
    let project = project("shape", SRC);
    let _ = render(&project, false);
    // 旧转译**写文件**（`<stem>.a2r.rs`），A2R 的 `auto rust` **打印**到 stdout：
    // 这是第 0 条差异（输出通道），测试按各自通道取样。
    let legacy_file = project.dir.join("lib.a2r.rs");
    assert!(
        legacy_file.exists(),
        "the legacy transpiler writes `<stem>.a2r.rs`"
    );
    let legacy = std::fs::read_to_string(&legacy_file).expect("legacy output");
    let a2r = render(&project, true);

    // 0. 输出通道（旧转译写文件；A2R 打印/由 `-o` 决定）。\n    // 1. 命名。
    assert!(legacy.contains("fn main"), "legacy keeps the source name");
    assert!(a2r.contains("fn_main"), "A2R lowers the entry name");
    assert!(
        !a2r.contains("fn main("),
        "A2R does not emit a source-named entry"
    );

    // 2. 运行时。
    assert!(
        !legacy.contains("a2x_runtime"),
        "legacy has no inline runtime"
    );
    assert!(a2r.contains("a2x_runtime"), "A2R inlines its runtime");

    // 3. 单文件通道只输出 `generated.rs`：入口包装（`A2xResult` + `value:/trap:`）
    //    在 fileset 的 `main.rs` 里，由 `--merge` 用例覆盖（那里能独立构建）。
    assert!(
        a2r.contains("use crate::a2x_runtime::*;"),
        "the printed program binds the inlined runtime module"
    );

    // 4. 体量：A2R 单文件输出远大于旧转译（运行时被内联）。
    assert!(
        a2r.len() > legacy.len() * 2,
        "A2R output ({}) must be much larger than legacy ({}) — the difference is the inlined runtime",
        a2r.len(),
        legacy.len()
    );

    eprintln!(
        "rust engine diff: legacy {} bytes / {} lines vs a2r {} bytes / {} lines",
        legacy.len(),
        legacy.lines().count(),
        a2r.len(),
        a2r.lines().count()
    );
}
