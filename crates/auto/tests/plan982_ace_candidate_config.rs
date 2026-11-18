//! PLAN-982 A slice (route C) — permanent verification of the **explicit ACE
//! candidate leg** and the **one-SDK witness**.
//!
//! What is proven here (RG-A01's A slice):
//! 1. `p982_a_config_and_execution_share_one_sdk` — one run reads a real
//!    project configuration document through the SDK's designated config owner
//!    and executes a real script through the explicit ACE door; the recorded
//!    receipt shows both legs report the SAME SDK package+version, the serving
//!    engine is the one the identity names, real config values (including the
//!    product's own dependency projection), the real stdout and exit status.
//! 2. `p982_a_dual_projects_are_isolated` — two projects with the same script
//!    and different `pac.at` documents resolve their own values (no cross-talk,
//!    no static default).
//! 3. `p982_a_bad_config_refuses_before_any_write` — a configuration that fails
//!    to parse and one that fails to *evaluate* both refuse by name
//!    (`ace.config/invalid`) and the project directory is unchanged
//!    byte-for-byte (no state/lock writes).
//! 4. `p982_a_candidate_never_falls_back_to_legacy` — the two-arm oracle: the
//!    same command that the legacy engine runs (argv pass-through) is REFUSED
//!    by name under the candidate, and the script's own output never appears —
//!    no silent fallback, no engine switch. The legacy `--dump-lowered`
//!    inspection surface is refused the same way (it must not print legacy
//!    lowering as if the candidate had succeeded).
//! 5. `p982_a_bounded_run_refuses_instead_of_legacy` — the candidate's budget
//!    terminal states (wall / cancel) are named refusals, and the same script
//!    does succeed without a budget (non-vacuity guard).
//!
//! PLAN-982 E7：配置腿已切到**显式引擎选择**（`auto_man_runtime::config_engine`：
//! 桥已装 ⇒ ACE，无桥宿主 ⇒ legacy）。本文件的见证额外记录 `config.engine` 与
//! `config.legacy_dispatch`，因此"A 切片只证明门面、配置引擎仍旧"的旧边界
//! 已被 E7 替换；剩下未证的是 Man 的**二阶段 context/dep** 与 `cargo tc`/780 矩阵。
//!
//! These cases only exist with the `ace-candidate` feature. With the feature
//! off the file reports an explicit, visible skip instead of a silent 0-test
//! green.

#![cfg(feature = "ace-candidate")]

use std::collections::BTreeMap;
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

fn fixture(rel: &str) -> PathBuf {
    fixtures_dir().join(rel)
}

struct Run {
    output: Output,
    receipt: Option<serde_json::Value>,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).to_string()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).to_string()
    }

    fn all(&self) -> String {
        format!("{}\n{}", self.stdout(), self.stderr())
    }
}

/// Run the real CLI binary on a fixture script. `candidate` selects the
/// explicit ACE candidate leg; `extra` are CLI arguments after the script.
fn run(script: &Path, candidate: bool, extra: &[&str], envs: &[(&str, &str)]) -> Run {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let receipt_path = std::env::temp_dir().join(format!(
        "p982-receipt-{}-{}.json",
        std::process::id(),
        nanos
    ));
    let _ = std::fs::remove_file(&receipt_path);

    let mut cmd = Command::new(cli());
    cmd.arg(script).args(extra);
    if candidate {
        cmd.env("AUTO_ACE_CANDIDATE", "1");
    } else {
        cmd.env_remove("AUTO_ACE_CANDIDATE");
    }
    // The budget hooks are inherited environment state: clear them so an
    // operator's exported values can never turn the non-vacuity guard red.
    cmd.env_remove("AUTO_ACE_WALL_MS");
    cmd.env_remove("AUTO_ACE_CANCEL_AFTER_MS");
    cmd.env("AUTO_ACE_PROOF_RECEIPT", &receipt_path);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("the auto binary spawns");
    let receipt = std::fs::read_to_string(&receipt_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    Run { output, receipt }
}

/// Recursive `relative path -> (bytes, content hash)` snapshot. The hash is a
/// plain FNV-1a over the file bytes so "unchanged" means byte-level unchanged,
/// not merely same-size.
fn snapshot(dir: &Path) -> BTreeMap<String, (u64, u64)> {
    fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash
    }
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, (u64, u64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                out.insert(format!("{rel}/"), (0, 0));
                walk(root, &path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(rel, (bytes.len() as u64, fnv1a(&bytes)));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn p982_a_config_and_execution_share_one_sdk() {
    let script = fixture("proj-a/main.at");
    let run = run(&script, true, &[], &[]);
    assert!(
        run.output.status.success(),
        "the candidate run must succeed.\nstdout:\n{}\nstderr:\n{}",
        run.stdout(),
        run.stderr()
    );

    let stdout = run.stdout();
    let receipt = run.receipt.clone().expect("the one-SDK receipt is written");
    assert_eq!(
        receipt["same_sdk"], true,
        "both legs must report one SDK package+version: {receipt}"
    );

    // Leg 1: real configuration values through the SDK config owner.
    assert_eq!(receipt["config"]["present"], true);
    assert_eq!(receipt["config"]["values"]["name"], "p982-a");
    assert_eq!(receipt["config"]["values"]["version"], "0.1.0");
    assert_eq!(receipt["config"]["values"]["scene"], "cli");
    assert_eq!(
        receipt["config"]["deps"], 1,
        "the dependency target is really evaluated through the product projection"
    );
    assert_eq!(
        receipt["config"]["targets"], 1,
        "the evaluated document produced exactly the declared target graph"
    );
    assert_eq!(receipt["config"]["values"]["pac_name"], "p982-a");
    let config_owner = receipt["config"]["owner"].as_str().unwrap();
    assert!(
        config_owner.contains("leg=config") && config_owner.contains("package=auto-lang-core"),
        "config leg identity: {config_owner}"
    );
    // PLAN-982 E7：配置求值实际由 ACE 引擎服务，且这次运行没有 legacy 派发。
    assert_eq!(
        receipt["config"]["engine"], "av",
        "the configuration leg must report the ACE engine: {receipt}"
    );
    assert_eq!(
        receipt["config"]["legacy_dispatch"], 0,
        "no legacy configuration dispatch may happen on the ACE path: {receipt}"
    );
    // PLAN-982/DEVSDK（用户裁定 A：只做"发现 + 见证"）：发现规则与其结果都是见证字段。
    assert_eq!(
        receipt["sdk_discovery"]["rule"], "nearest-pac.at",
        "the discovery rule is named: {receipt}"
    );
    assert_eq!(
        receipt["sdk_discovery"]["found"], true,
        "the project document is discovered: {receipt}"
    );
    assert_eq!(
        receipt["sdk_discovery"]["config_engine"], "av",
        "the discovered SDK is served by ACE: {receipt}"
    );
    assert_eq!(
        receipt["sdk_discovery"]["legacy_dispatch"], 0,
        "discovery never falls back to the legacy engine: {receipt}"
    );

    // Leg 2: real execution through the explicit ACE door.
    assert_eq!(receipt["execution"]["engine"], "av");
    assert_eq!(
        receipt["execution"]["engine_is_identity_engine"], true,
        "the serving engine is the one the identity names: {receipt}"
    );
    assert_eq!(receipt["execution"]["outcome"], "success");
    assert_eq!(receipt["execution"]["detail"], "7");
    assert!(receipt["execution"]["steps"].as_u64().unwrap_or(0) > 0);
    let execution_owner = receipt["execution"]["owner"].as_str().unwrap();
    assert!(
        execution_owner.contains("leg=execution") && execution_owner.contains("engine=av"),
        "execution leg identity: {execution_owner}"
    );

    let sdk = receipt["sdk"].as_str().unwrap();
    assert!(
        sdk.contains("package=auto-lang-core"),
        "sdk identity: {sdk}"
    );
    assert!(
        sdk.contains(&format!(
            "version={}",
            auto_lang_core::sdk_identity::sdk_version()
        )),
        "the receipt names this build's SDK version: {sdk}"
    );
    // The receipt binds the real input, and each run is distinguishable.
    assert!(
        receipt["script_bytes"].as_u64().unwrap_or(0) > 0,
        "the receipt binds the script input: {receipt}"
    );
    assert!(
        receipt["started_unix_ms"].as_u64().unwrap_or(0) > 0,
        "the receipt carries a run timestamp: {receipt}"
    );

    // Observable behaviour: the script's own output and the run's exit status.
    assert!(
        stdout.contains("42"),
        "the script's printed value is observable: {stdout}"
    );
}

#[test]
fn p982_a_dual_projects_are_isolated() {
    let a = run(&fixture("proj-a/main.at"), true, &[], &[]);
    let b = run(&fixture("proj-b/main.at"), true, &[], &[]);
    assert!(a.output.status.success(), "{}", a.all());
    assert!(b.output.status.success(), "{}", b.all());

    let a_receipt = a.receipt.expect("proj-a receipt");
    let b_receipt = b.receipt.expect("proj-b receipt");
    assert_eq!(a_receipt["config"]["values"]["name"], "p982-a");
    assert_eq!(b_receipt["config"]["values"]["name"], "p982-b");
    assert_eq!(b_receipt["config"]["values"]["version"], "2.3.4");
    assert_eq!(
        b_receipt["config"]["deps"], 1,
        "the second project's own dependency is evaluated too"
    );
    assert_ne!(
        a_receipt["config"]["values"]["name"], b_receipt["config"]["values"]["name"],
        "two projects with the same script keep their own configuration"
    );
}

#[test]
fn p982_a_bad_config_refuses_before_any_write() {
    // Two distinct failure kinds, both through the same SDK config owner:
    // a document that cannot be parsed and a document that cannot be evaluated.
    // The expected diagnostic substring pins WHICH kind failed, so a regression
    // back to parse-only coverage stays red.
    // PLAN-982 E7：配置腿由 ACE 服务后，拒绝的**具名诊断**是 ACE 的
    // `source.undefined` / `source.syntax`（不再是旧 VM 的 "Expected term" /
    // "Undefined variable" 文案）；覆盖范围仍是"解析失败"和"求值失败"两类。
    for (project_name, expected_kind) in [
        ("bad-config", "source.undefined"),
        ("bad-eval", "source.syntax"),
        // 求值期失败：文档编好了，AV 在 `1 / 0` 上 trap（B-CONFIG-R F-982B-05）。
        ("bad-runtime", "ace.config/trap"),
    ] {
        let expected_kind: &str = expected_kind;
        let project = fixture(project_name);
        let script = project.join("main.at");
        let before = snapshot(&project);

        let run = run(&script, true, &[], &[]);
        assert!(
            !run.output.status.success(),
            "a {project_name} configuration must refuse the run"
        );
        assert!(
            run.all().contains("ace.config/invalid"),
            "the refusal is named for {project_name}: {}",
            run.all()
        );
        let after = snapshot(&project);
        assert_eq!(
            before, after,
            "a refused {project_name} configuration must leave the project tree \
             byte-identical (no state/lock files)"
        );

        let receipt = run.receipt.expect("the refusal is recorded with identity");
        assert_eq!(receipt["refusal"]["code"], "ace.config/invalid");
        let detail = receipt["config"]["error"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected_kind),
            "the {project_name} refusal keeps its own failure kind ({expected_kind}): {detail}"
        );
        assert_eq!(
            receipt["execution"]["outcome"], "not-started",
            "the execution leg never starts after a {project_name} configuration"
        );
        assert_eq!(
            receipt["same_sdk"], true,
            "identity is still one SDK for {project_name}: {receipt}"
        );
    }
}

#[test]
fn p982_a_candidate_never_falls_back_to_legacy() {
    let script = fixture("proj-a/main.at");

    // Arm 1 — the LEGACY engine runs this exact command (argv pass-through is
    // its documented behaviour). This arm is the oracle: the command is
    // runnable, so a refusal under the candidate cannot be blamed on the input.
    let legacy = run(&script, false, &["p982-extra"], &[]);
    assert!(
        legacy.output.status.success(),
        "the legacy arm must run the same command.\nstdout:\n{}\nstderr:\n{}",
        legacy.stdout(),
        legacy.stderr()
    );
    assert!(
        legacy.stdout().contains("42"),
        "the legacy arm observes the script output: {}",
        legacy.stdout()
    );

    // Arm 2 — the CANDIDATE refuses by name and never dispatches the script.
    let candidate = run(&script, true, &["p982-extra"], &[]);
    assert!(
        !candidate.output.status.success(),
        "the candidate must refuse instead of falling back"
    );
    assert!(
        candidate.all().contains("ace.args/unsupported"),
        "the refusal is named: {}",
        candidate.all()
    );
    assert!(
        !candidate.stdout().contains("42"),
        "no legacy dispatch may run the script: {}",
        candidate.stdout()
    );

    // Same discipline for the legacy inspection surface: a selected candidate
    // must not print legacy lowering and exit 0.
    let dump = run(&script, true, &["--dump-lowered"], &[]);
    assert!(
        !dump.output.status.success(),
        "dump-lowered under a selected candidate must not succeed"
    );
    assert!(
        dump.all().contains("ace.dump-lowered/unsupported"),
        "the dump-lowered refusal is named: {}",
        dump.all()
    );
    assert!(
        !dump.stdout().contains("42"),
        "no legacy lowering is printed: {}",
        dump.stdout()
    );

    // The legacy arm keeps the inspection surface working when the candidate
    // is not selected (the gate must not regress the default behaviour).
    let legacy_dump = run(&script, false, &["--dump-lowered"], &[]);
    assert!(
        legacy_dump.output.status.success(),
        "the legacy dump-lowered surface still works: {}",
        legacy_dump.all()
    );
    assert!(
        !legacy_dump.stdout().trim().is_empty(),
        "the legacy dump-lowered surface still prints lowering"
    );
}

#[test]
fn p982_a_bounded_run_refuses_instead_of_legacy() {
    let script = fixture("budget/main.at");

    // Non-vacuity guard: without a budget the very same script succeeds
    // through the candidate leg.
    let unbounded = run(&script, true, &[], &[]);
    assert!(
        unbounded.output.status.success(),
        "the budget fixture must succeed unbounded (guards vacuity).\nstdout:\n{}\nstderr:\n{}",
        unbounded.stdout(),
        unbounded.stderr()
    );

    // Wall budget: the run ends in a NAMED refusal, never a legacy dispatch.
    let walled = run(&script, true, &[], &[("AUTO_ACE_WALL_MS", "1")]);
    assert!(
        !walled.output.status.success(),
        "a bounded run must not silently complete"
    );
    let receipt = walled.receipt.expect("the bounded run records its outcome");
    let outcome = receipt["execution"]["outcome"].as_str().unwrap_or_default();
    assert_eq!(
        outcome, "resource-denied",
        "the wall budget is a resource-denial terminal state: {receipt}"
    );
    let code = receipt["refusal"]["code"].as_str().unwrap_or_default();
    assert!(
        code.starts_with("ace.resource/") && code.contains("Wall"),
        "the wall budget names its own resource axis: {code}"
    );

    // Cancel flag: the same discipline for host cancellation.
    let cancelled = run(&script, true, &[], &[("AUTO_ACE_CANCEL_AFTER_MS", "1")]);
    assert!(
        !cancelled.output.status.success(),
        "a cancelled run must not silently complete"
    );
    let receipt = cancelled
        .receipt
        .expect("the cancelled run records its outcome");
    let outcome = receipt["execution"]["outcome"].as_str().unwrap_or_default();
    assert!(
        outcome == "cancelled" || outcome == "resource-denied",
        "cancellation ends in a named terminal state: {receipt}"
    );
}
