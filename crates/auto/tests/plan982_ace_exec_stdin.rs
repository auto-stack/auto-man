//! PLAN-982（EXEC，CLI 侧 stdin 替代）：管道输入在 `auto config` 上作为文本文档
//! 参数 `stdin` 注入。
//!
//! ACE 的通用 stdin 读取需要 ISA 级变更（`HostCall` 是单实参指令），本轮交付的是
//! **不触碰 ISA** 的替代面：把标准输入**预读**成配置参数，文档里用 `${stdin}` 取。
//! 断言三条：
//! 1. 有管道输入时 `${stdin}` 插值出输入内容；
//! 2. 无输入（空管道）时 `stdin` 参数**不存在**，`${stdin}` 是具名失败（不是空串）；
//! 3. 该注入在 ACE 引擎下同样生效（`AUTO_CONFIG_ENGINE=ace`）。

#![cfg(feature = "ace-candidate")]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const PAC: &str = "name: \"stdin-probe\"\nbuild_id: \"${stdin}\"\n";

struct Project {
    dir: PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn project(tag: &str) -> Project {
    let dir = std::env::temp_dir().join(format!(
        "p982-stdin-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("pac.at"), PAC).expect("pac");
    Project { dir }
}

fn run_config(
    project: &Project,
    input: Option<&str>,
    engine: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.args(["config", "pac.at"])
        .current_dir(&project.dir)
        .env_remove("AUTO_CONFIG_ENGINE")
        .env_remove("AUTO_ACE_CANDIDATE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(engine) = engine {
        cmd.env("AUTO_CONFIG_ENGINE", engine);
    }
    let mut child = cmd.spawn().expect("spawn auto config");
    if let Some(input) = input {
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    drop(child.stdin.take());
    child.wait_with_output().expect("wait")
}

#[test]
fn p982_exec_stdin_is_injected_as_a_document_argument() {
    let project = project("inject");
    let out = run_config(&project, Some("build-42"), Some("ace"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "the ACE config read must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("build-42"),
        "`${{stdin}}` must interpolate the piped input, got: {stdout}"
    );
}

#[test]
fn p982_exec_stdin_is_absent_when_nothing_is_piped() {
    let project = project("empty");
    let out = run_config(&project, None, Some("ace"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // 空管道 ⇒ 不注入：`${stdin}` 必须**具名失败**，不能变成空串。
    assert!(
        !out.status.success(),
        "an absent stdin argument must fail by name (stdout: {})",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("stdin") || stderr.contains("source."),
        "the failure names the missing argument: {stderr}"
    );
}
