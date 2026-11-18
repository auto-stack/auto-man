//! PLAN-982（U0，2026-11-18 用户指令）：**Vue 目标的 `auto run` CLI 分派**。
//!
//! 分工（用户裁定）：980 负责生成产物与 `a2x_rt`（工具侧 lane 接线）；
//! 982 负责启动并传回构建诊断（本文件钉住的 CLI 面）；983 负责产品验收。
//!
//! 用例三条：
//! 1. **非 Vue 目标保持具名拒绝**：候选腿下 `auto run`（默认 vm 腿）仍
//!    `ace.exec/unsupported-command`；
//! 2. **Vue 目标分派接通**：候选腿下 `auto run --render vue` **不再**统一
//!    拒绝——分派进入 `run_vue`（`UiOps::vue_run` → auto-ui-tool），并显式
//!    发射 `AUTO_UI_ACE_VUE=1`（980 对签的生成选择键）。工具缺席
//!    （`AUTO_UI_TOOL_BIN` 指向不存在的路径）⇒ **具名**发现错误；
//! 3. **等待点（具名 ignore）**：真实 CLI 的 `0 → 1 → 0` 与 ACE handler
//!    覆盖断言——等 980 工具侧 lane 接线（生成产物 + a2x_rt）。

use std::path::{Path, PathBuf};
use std::process::Command;

const EXAMPLE_SUBPATH: &str = "examples/ui/002-counter";

fn counter_example() -> Option<PathBuf> {
    // 原样 002-counter（不改一字）：env 显式 → 组内 sibling → 主检出。
    if let Some(root) = std::env::var_os("AUTO_UI_ROOT") {
        let p = PathBuf::from(root).join(EXAMPLE_SUBPATH);
        if p.is_dir() {
            return Some(p);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for base in [
        manifest.join("../../../auto-ui"),
        PathBuf::from("D:/autostack/auto-ui"),
    ] {
        let p = base.join(EXAMPLE_SUBPATH);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-u0-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// 原样复制 002-counter（整树拷贝，零改写）。
fn copy_example(dst: &Path) -> PathBuf {
    let src = counter_example().expect(
        "the unmodified 002-counter example (set AUTO_UI_ROOT or keep the \
         auto-ui sibling/main checkout present)",
    );
    fn walk(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap().flatten() {
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                walk(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    walk(&src, dst);
    dst.to_path_buf()
}

fn run_cli(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.args(args)
        .current_dir(dir)
        .env_remove("AUTO_ACE_CANDIDATE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn auto");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// (1) 非 Vue 目标：候选腿仍具名拒绝（vm 腿未迁移）。
#[test]
fn p982_u0_non_vue_run_keeps_the_named_refusal() {
    let dir = scratch("refusal");
    let (ok, log) = run_cli(&dir, &["run"], &[("AUTO_ACE_CANDIDATE", "1")]);
    eprintln!(
        "p982 u0 refusal: ok={ok} log-tail={}",
        log.lines().rev().take(4).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        !ok,
        "the default (vm) run leg stays refused under the candidate"
    );
    assert!(
        log.contains("ace.exec/unsupported-command"),
        "the refusal is named: {log}"
    );
    assert!(
        log.contains("`auto run`"),
        "the refusal names the command: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (2) Vue 目标：分派接通 + 生成选择键发射 + 工具缺席具名错误。
#[test]
fn p982_u0_vue_run_dispatches_and_names_the_missing_provider() {
    let dir = scratch("vue-missing");
    copy_example(&dir);
    // 工具缺席：显式 env 键指向不存在的路径 ⇒ 具名 `EnvPathMissing`。
    let missing_tool = dir.join("no-such-auto-ui-tool.exe");
    let (ok, log) = run_cli(
        &dir,
        &["run", "--render", "vue"],
        &[
            ("AUTO_ACE_CANDIDATE", "1"),
            ("AUTO_UI_TOOL_BIN", missing_tool.to_str().unwrap()),
        ],
    );
    eprintln!(
        "p982 u0 vue-missing: ok={ok} log-tail={}",
        log.lines().rev().take(6).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        !ok,
        "the missing provider fails the command (no silent legacy launch)"
    );
    // 分派证据：候选闸放行 + 生成选择键发射。
    assert!(
        !log.contains("ace.exec/unsupported-command"),
        "the vue target is NOT blanket-refused: {log}"
    );
    assert!(
        log.contains("AUTO_UI_ACE_VUE=1"),
        "the ACE vue lane selection is emitted: {log}"
    );
    // 工具缺席：具名发现错误（不静默、不猜路径；诊断换行/装饰轨归一后比对）。
    let flat: String = log
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        flat.contains("auto-ui-tool discovery failed")
            && flat.contains("AUTO_UI_TOOL_BIN points to missing path"),
        "the missing provider is a named discovery error: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (3) 等待点（具名 ignore）：真实 CLI 启动的 `0 → 1 → 0` 与 ACE handler
/// 覆盖断言——依赖 980 的工具侧 lane 接线（`generate_sfc_with_lane_product`
/// 进 auto-ui-tool 的生成 + `a2x_rt.mjs` 随产物落盘 + `AUTO_UI_ACE_VUE` 的
/// lane 缺席具名拒绝）。接线后去 ignore 即为 983 的 U0 产品验收入口。
#[ignore = "waits the 980 tool-side lane wiring (generation artifact + a2x_rt + named refusal on lane absence); see PLAN-982 round 184"]
#[test]
fn p982_u0_vue_run_serves_the_counter_page_with_ace_coverage() {
    let dir = scratch("vue-page");
    copy_example(&dir);
    let (ok, log) = run_cli(
        &dir,
        &["run", "--render", "vue"],
        &[("AUTO_ACE_CANDIDATE", "1")],
    );
    assert!(ok, "the launched vue page serves 0 -> 1 -> 0: {log}");
    assert!(
        log.contains("AUTO_UI_ACE_VUE=1"),
        "the ACE lane selection is visible: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
