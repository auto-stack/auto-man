//! PLAN-982（B-CONFIG-I，E8）：**V-CFG-PRODUCT / V-CFG-RETIRE 的 Man 侧两例**。
//!
//! 与语言侧七例（`auto-lang-ace-host/tests/p982_vcfg_cases.rs`）互补：这里只跑
//! **真实候选 CLI**（`AUTO_ACE_CANDIDATE=1`）→ 配置腿 → 产品投影（Pac/target/
//! dep）→ 实际执行，并把引擎与 legality 轴当作独立事实断言，不用"新旧相等"
//! 或 `same_sdk` 代替业务值。
//!
//! 只写不依赖 A 切片断言的部分：A 切片（`plan982_ace_candidate_config.rs`）仍是
//! 门面/见证用例；本文件补 **引擎选择轴** 与 **退役轴**（旧求值边不可隐式回到
//! 候选路径）。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli() -> &'static str {
    env!("CARGO_BIN_EXE_auto")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("plan982")
}

struct Run {
    output: Output,
    receipt: Option<serde_json::Value>,
}

impl Run {
    fn all(&self) -> String {
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        )
    }
}

/// Run the real CLI on a fixture script with an explicit config-engine env.
fn run(script: &Path, engine_env: Option<&str>) -> Run {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let receipt_path = std::env::temp_dir().join(format!(
        "p982b-receipt-{}-{}.json",
        std::process::id(),
        nanos
    ));
    let _ = std::fs::remove_file(&receipt_path);
    let mut cmd = Command::new(cli());
    cmd.arg(script);
    cmd.env("AUTO_ACE_CANDIDATE", "1");
    cmd.env_remove("AUTO_ACE_WALL_MS");
    cmd.env_remove("AUTO_ACE_CANCEL_AFTER_MS");
    match engine_env {
        Some(value) => {
            cmd.env("AUTO_CONFIG_ENGINE", value);
        }
        None => {
            cmd.env_remove("AUTO_CONFIG_ENGINE");
        }
    }
    cmd.env("AUTO_ACE_PROOF_RECEIPT", &receipt_path);
    let output = cmd.output().expect("the auto binary spawns");
    let receipt = std::fs::read_to_string(&receipt_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    Run { output, receipt }
}

/// V-CFG-PRODUCT：候选 CLI 的配置腿**真由 ACE 服务**，产品投影（Pac/dep/
/// target）来自真实求值，且默认路径不发生 legacy 派发。
#[test]
fn p982_b_product_config_engine_is_ace_with_zero_legacy_dispatch() {
    let script = fixtures_dir().join("proj-a").join("main.at");
    let run = run(&script, None);
    let receipt = run.receipt.expect("the candidate run records a receipt");
    // 引擎轴：实际服务引擎 + legacy 派发计数（不是自报标签）。
    assert_eq!(
        receipt["config"]["engine"], "av",
        "the configuration leg must be served by ACE: {receipt}"
    );
    assert_eq!(
        receipt["config"]["legacy_dispatch"], 0,
        "no legacy config dispatch on the default path: {receipt}"
    );
    // 业务值：真实文档 → 产品投影。
    assert_eq!(receipt["config"]["present"], true);
    assert_eq!(receipt["config"]["values"]["name"], "p982-a");
    assert_eq!(receipt["config"]["values"]["version"], "0.1.0");
    assert_eq!(
        receipt["config"]["deps"], 1,
        "the dependency target is really evaluated: {receipt}"
    );
    assert_eq!(receipt["config"]["targets"], 1);
    // 执行腿照常成功（配置腿不是空跑）。
    assert_eq!(receipt["execution"]["outcome"], "success");
}

/// V-CFG-RETIRE：旧求值边只能**显式**选择（`AUTO_CONFIG_ENGINE=legacy`），
/// 且这条选择在收据里可见；未知取值不得静默把候选路径降级到 legacy。
#[test]
fn p982_b_retire_legacy_requires_an_explicit_selection() {
    let script = fixtures_dir().join("proj-a").join("main.at");

    // 显式 legacy：允许、具名、值仍正确（对照轴仍可用）。
    let explicit = run(&script, Some("legacy"));
    let receipt = explicit
        .receipt
        .expect("the explicit-legacy run records a receipt");
    assert_eq!(
        receipt["config"]["engine"], "legacy",
        "an explicit legacy selection is visible, never silent: {receipt}"
    );
    assert_eq!(receipt["config"]["values"]["name"], "p982-a");
    assert_eq!(
        receipt["config"]["legacy_dispatch"], 1,
        "the explicit legacy run records exactly one legacy dispatch: {receipt}"
    );

    // 未知取值：不静默降级（仍是默认 ACE，legacy 派发为 0）。
    let unknown = run(&script, Some("not-an-engine"));
    let receipt = unknown
        .receipt
        .clone()
        .expect("the unknown-engine run records a receipt");
    assert_eq!(
        receipt["config"]["engine"], "av",
        "an unknown engine name never silently selects legacy: {receipt}"
    );
    assert_eq!(receipt["config"]["legacy_dispatch"], 0);
    assert!(
        unknown.output.status.success(),
        "the run still succeeds on the default engine: {}",
        unknown.all()
    );
}
