//! PLAN-982 A slice (route C) — permanent verification of the **explicit ACE
//! candidate leg** and the **one-SDK witness**.
//!
//! What is proven here (RG-A01's A slice):
//! 1. `p982_a_config_and_execution_share_one_sdk` — one run reads a real
//!    project configuration document through the SDK's designated config owner
//!    and executes a real script through the explicit ACE door; the recorded
//!    receipt shows both legs report the SAME SDK package+version, real config
//!    values, the ACE engine identity, the real stdout and the exit status.
//! 2. `p982_a_dual_projects_are_isolated` — two projects with the same script
//!    and different `pac.at` documents resolve their own values (no cross-talk,
//!    no static default).
//! 3. `p982_a_bad_config_refuses_before_any_write` — a configuration that fails
//!    evaluation refuses by name (`ace.config/invalid`) and the project
//!    directory is byte-for-byte unchanged (no state/lock writes).
//! 4. `p982_a_candidate_never_falls_back_to_legacy` — the two-arm oracle: the
//!    same command that the legacy engine runs (argv pass-through) is REFUSED
//!    by name under the candidate, and the script's own output never appears —
//!    no silent fallback, no engine switch.
//! 5. `p982_a_bounded_run_refuses_instead_of_legacy` — the candidate's budget
//!    terminal states (wall / cancel) are named refusals, and the same script
//!    does succeed without a budget (non-vacuity guard).
//!
//! What is NOT proven here (named delta, PLAN-982 §5 `GAP-982-CONFIG-ACE`): the
//! configuration *engine* migration to ACE. The config leg is the SDK's frozen
//! legacy owner (PLAN-969 facade spec §边界); its migration stays with M2-B.

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
/// explicit ACE candidate leg; `extra` are script argv values.
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

/// Recursive `relative path -> bytes` snapshot (the "writes nothing" oracle).
fn snapshot(dir: &Path) -> BTreeMap<String, u64> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) {
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
                out.insert(format!("{rel}/"), 0);
                walk(root, &path, out);
            } else if let Ok(meta) = entry.metadata() {
                out.insert(rel, meta.len());
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

    // Leg 2: real execution through the explicit ACE door.
    assert_eq!(receipt["execution"]["engine"], "av");
    assert_eq!(receipt["execution"]["outcome"], "success");
    assert_eq!(receipt["execution"]["detail"], "7");
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
    assert_ne!(
        a_receipt["config"]["values"]["name"], b_receipt["config"]["values"]["name"],
        "two projects with the same script keep their own configuration"
    );
}

#[test]
fn p982_a_bad_config_refuses_before_any_write() {
    let project = fixture("bad-config");
    let script = project.join("main.at");
    let before = snapshot(&project);

    let run = run(&script, true, &[], &[]);
    assert!(
        !run.output.status.success(),
        "a configuration that fails evaluation must refuse the run"
    );
    assert!(
        run.all().contains("ace.config/invalid"),
        "the refusal is named: {}",
        run.all()
    );
    let after = snapshot(&project);
    assert_eq!(
        before, after,
        "a refused configuration must write nothing (no state/lock files)"
    );

    let receipt = run.receipt.expect("the refusal is recorded with identity");
    assert_eq!(receipt["refusal"]["code"], "ace.config/invalid");
    assert_eq!(
        receipt["execution"]["outcome"], "not-started",
        "the execution leg never starts after a bad configuration"
    );
    assert_ne!(
        receipt["same_sdk"], false,
        "identity is still one SDK: {receipt}"
    );
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
