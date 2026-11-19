//! PLAN-982 A slice (route C, user ruling 2026-11-17): the **explicit ACE
//! candidate execution leg** for the minimal CLI script command, plus the
//! one-SDK witness that binds the config leg and the execution leg together.
//!
//! Why this module exists
//! ----------------------
//! PLAN-982/CONFIG requires the product's configuration evaluation and the
//! product's actual command consumption to be served by **one SDK**, with no
//! silent fallback to a different engine. Two facts shape the A slice:
//!
//! 1. The ACE door (`auto_lang_core::ace_entry` / `run_ace`) is the explicit,
//!    host-installed execution engine (`AceEngineChoice::AceAV`).
//! 2. The configuration *engine* for `pac.at` / `am.at` documents is still the
//!    SDK's designated **legacy owner** — PLAN-969's verified facade spec
//!    (`docs/specs/ace-migration/m2-facade.md` §边界) froze
//!    `eval_config_with_vm` there and explicitly did not copy the config
//!    surface into the ACE door. Migrating that engine is the named, unowned
//!    capability gap `GAP-982-CONFIG-ACE` (PLAN-982 §5/§10) and belongs to
//!    the M2-B slice.
//!
//! The A slice therefore proves the *identity* half, which is what the
//! product actually consumes today: one SDK build, an explicit candidate
//! door, a recorded identity for both legs, real values/stdout/exit, and a
//! **named refusal instead of any legacy fallback** whenever the candidate
//! cannot serve the request.
//!
//! Contract of the candidate leg (opt-in only)
//! -------------------------------------------
//! * Selection is explicit: `AUTO_ACE_CANDIDATE=1` (never a default, never a
//!   silent switch).
//! * When selected, `auto <script.at>`:
//!   1. validates the project configuration document next to (or above) the
//!      script through the SDK config owner, recording the resolved values;
//!   2. executes the script through the ACE door (console lane) and re-emits
//!      the captured streams (their content matches the legacy leg's output;
//!      the trailing spacing is printed separately to match it too);
//!   3. records one identity line covering both legs, the script input size
//!      and a run timestamp, and checks that both legs name one SDK
//!      package+version **and** that the engine which actually served the run
//!      is the engine that identity names;
//!   4. refuses by name (`ace.…` codes) — with zero legacy dispatch — for
//!      argv, the legacy `--dump-lowered` inspection surface, unsupported
//!      script shapes, diagnostics, denials, resource denials, cancellation
//!      and an identity mismatch. It never calls the legacy script runner and
//!      never prints legacy lowering.
//! * Nothing is written **inside the project tree**: a bad configuration or a
//!   refused run leaves no state/lock files behind (the A slice's "failure
//!   writes nothing" discipline). The only file this leg ever writes is the
//!   optional receipt below, and only when its path is supplied explicitly
//!   (a verification hook, not a product default).
//!
//! Test hooks (never product defaults)
//! -----------------------------------
//! * `AUTO_ACE_PROOF_RECEIPT=<path>` — write the JSON identity/evidence line.
//! * `AUTO_ACE_WALL_MS=<ms>` — per-run wall budget passed to the ACE door.
//! * `AUTO_ACE_CANCEL_AFTER_MS=<ms>` — set the run's cancel flag from a
//!   watchdog thread (proves the cancelled terminal state).

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

/// Explicit opt-in switch for the ACE candidate leg.
pub(crate) const SELECT_ENV: &str = "AUTO_ACE_CANDIDATE";
/// Optional receipt path for the one-SDK evidence line.
pub(crate) const RECEIPT_ENV: &str = "AUTO_ACE_PROOF_RECEIPT";
/// Optional per-run wall budget (milliseconds).
pub(crate) const WALL_MS_ENV: &str = "AUTO_ACE_WALL_MS";
/// Optional watchdog that sets the run's cancel flag (milliseconds).
pub(crate) const CANCEL_AFTER_MS_ENV: &str = "AUTO_ACE_CANCEL_AFTER_MS";

/// True when the candidate leg is explicitly selected.
pub(crate) fn selected() -> bool {
    match std::env::var(SELECT_ENV) {
        Ok(v) => matches!(v.trim(), "1" | "true" | "yes"),
        Err(_) => false,
    }
}

/// Run the script through the explicit ACE candidate leg.
///
/// Called by `cmd_run::run_script_file` only when [`selected`] is true; the
/// caller returns immediately afterwards and never reaches the legacy
/// runner (nor the legacy `--dump-lowered` inspection surface) on this path.
pub(crate) fn run_script(
    ai_mode: bool,
    path: &str,
    script_args: &[String],
    dump_lowered: bool,
) -> miette::Result<()> {
    // PLAN-982（Q-982-2 裁定 (a) 第一步）：**显式候选入口** `AUTO_ACE_SCRIPT_SYS=1`
    // ⇒ 脚本走 sys-stdlib 包车道（`process.args()` 公开面＋宿主供
    // `[argv0, ...args]`、逐执行隔离；无参/有参同一条路径）。默认（未设）
    // 保持 console 腿不动——第二步兼容门（stdout/退出码/stdin 组合/same_sdk/
    // 零回退）通过后再统一切换默认。
    if std::env::var("AUTO_ACE_SCRIPT_SYS")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        if dump_lowered {
            return refuse(
                ai_mode,
                "ace.dump-lowered/unsupported",
                "dump-lowered is a legacy inspection surface; the sys lane refuses it",
                None,
            );
        }
        return run_sys_script_lane(ai_mode, path, script_args);
    }
    // The AV lane has no argv surface (PLAN-969 T-08 named refusal). The A
    // slice refuses by name instead of dropping the arguments.
    if !script_args.is_empty() {
        return refuse(
            ai_mode,
            "ace.args/unsupported",
            "the console lane has no argv surface; the sys lane \
             (AUTO_ACE_SCRIPT_SYS=1) carries script arguments, or re-run without \
             the candidate selection",
            None,
        );
    }
    // `--dump-lowered` is a legacy inspection surface. A selected candidate
    // must not print legacy lowering as if it had succeeded.
    if dump_lowered {
        return refuse(
            ai_mode,
            "ace.dump-lowered/unsupported",
            "dump-lowered is a legacy inspection surface; the ACE candidate leg \
             refuses it instead of printing legacy lowering",
            None,
        );
    }

    #[cfg(windows)]
    {
        return run_script_on_ace(ai_mode, path);
    }
    // The ACE execution bridge is Windows-only (ace-abi pattern: the crate
    // compiles empty elsewhere), so the candidate leg refuses by name on
    // other targets instead of quietly serving the request with the legacy
    // engine.
    #[cfg(not(windows))]
    {
        return refuse(
            ai_mode,
            "ace.host/unsupported-platform",
            "the ACE execution bridge is Windows-only in this slice; the \
             candidate leg refuses instead of falling back to the legacy engine",
            None,
        );
    }
}

/// The Windows execution body: install the host bridge, evaluate the config
/// leg, run the script through the ACE door and record the witness.
#[cfg(windows)]
fn run_script_on_ace(ai_mode: bool, path: &str) -> miette::Result<()> {
    use auto_lang_core::{run_ace, run_ace_text, AceEngineChoice, AceOutcome, AceRequest};

    use crate::diag::output_success;

    // Explicit host installation: an unregistered executor is a named host
    // failure, never a default-engine switch.
    if let Err(message) = auto_lang_ace_host::install() {
        return refuse(ai_mode, "ace.host/install-failed", &message, None);
    }

    let started_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(e) => {
            return refuse(
                ai_mode,
                "ace.source/read-failed",
                &format!("cannot read `{path}`: {e}"),
                None,
            )
        }
    };

    // ---- Leg 1: configuration through the SDK's designated config owner ----
    let config_leg = auto_lang_core::sdk_identity::config_owner_identity();
    let execution_leg = auto_lang_core::sdk_identity::execution_owner_identity();
    // The identity predicate is a build-consistency check, not a claim about
    // engine semantics: it only becomes meaningful together with the engine
    // the bridge ACTUALLY served (checked below against the reported engine).
    let legs_same_sdk = auto_lang_core::sdk_identity::same_sdk(&config_leg, &execution_leg);
    let config = match read_project_config(path) {
        Ok(config) => config,
        Err(e) => {
            // A bad configuration refuses BEFORE anything runs (and writes
            // nothing inside the project tree — see the A-slice discipline).
            let receipt = json!({
                "schema": auto_lang_core::sdk_identity::SDK_IDENTITY_SCHEMA,
                "sdk": auto_lang_core::sdk_identity::sdk_identity(),
                "same_sdk": legs_same_sdk,
                "config": {
                    "owner": config_leg,
                    "present": true,
                    "error": e,
                },
                "execution": { "outcome": "not-started" },
                "script": path,
                "script_bytes": source.len(),
                "started_unix_ms": started_unix_ms,
            });
            return refuse(ai_mode, "ace.config/invalid", &e, Some(receipt));
        }
    };

    // ---- Leg 2: execution through the explicit ACE door ----
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(ms) = env_millis(CANCEL_AFTER_MS_ENV) {
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            flag.store(true, Ordering::SeqCst);
        });
    }
    let wall = env_millis(WALL_MS_ENV).map(Duration::from_millis);
    let grants = console_grants();
    let request = AceRequest {
        console: true,
        grants: &grants,
        wall,
        cancel: Some(cancel.as_ref()),
        ..AceRequest::simple(&source)
    };
    let report = match read_piped_stdin().filter(|_| main_takes_single_str_param(&source)) {
        Some(input) => run_ace_text(AceEngineChoice::AceAV, &request, &[input.as_bytes()]),
        None => run_ace(AceEngineChoice::AceAV, &request),
    };
    // One SDK means: both legs mint their identity from this crate AND the
    // engine that actually served the run is the engine that identity names.
    let same_sdk =
        legs_same_sdk && report.engine == auto_lang_core::sdk_identity::EXECUTION_ENGINE_ID;

    let (stdout, stderr) = report
        .console
        .as_ref()
        .map(|c| (c.stdout.clone(), c.stderr.clone()))
        .unwrap_or_default();

    let (outcome_tag, value_repr) = match &report.outcome {
        AceOutcome::Success { value, .. } => ("success", Some(ace_value_repr(*value))),
        AceOutcome::Diagnostic {
            stage, code, span, ..
        } => (
            "diagnostic",
            Some(format!(
                "stage={stage} code={code} span={:?}",
                span.map(|s| (s.start, s.end))
            )),
        ),
        AceOutcome::Denied { code, .. } => ("denied", Some(code.clone())),
        AceOutcome::ResourceDenied { kind, .. } => ("resource-denied", Some(kind.clone())),
        AceOutcome::Cancelled => ("cancelled", None),
        AceOutcome::HostFailure { code, .. } => ("host-failure", Some(code.clone())),
    };

    let receipt = json!({
        "schema": auto_lang_core::sdk_identity::SDK_IDENTITY_SCHEMA,
        "sdk": auto_lang_core::sdk_identity::sdk_identity(),
        "same_sdk": same_sdk,
        "config": {
            "owner": config_leg,
            "path": config.pac_path.as_ref().map(|p| p.display().to_string()),
            "present": config.present,
            "values": config.values,
            "deps": config.deps,
            "targets": config.targets,
            // PLAN-982 E7：实际引擎与派发计数（不是自报标签）。
            "engine": auto_man_runtime::config_engine::engine_name(),
            "legacy_dispatch": auto_man_runtime::config_engine::observation().legacy_dispatches,
        },
        // PLAN-982/DEVSDK（用户裁定 A：只做"发现 + 见证"）：把**发现规则本身**写进
        // 见证——本候选腿按"最近的 `pac.at`"发现工程 SDK；找到没找到、以及服务它的
        // 配置引擎，都是可断言字段（`legacy_dispatch` 是**真实计数**，不是自报标签）。
        "sdk_discovery": {
            "rule": "nearest-pac.at",
            "found": config.present,
            "pac_at": config.pac_path.as_ref().map(|p| p.display().to_string()),
            "config_engine": auto_man_runtime::config_engine::engine_name(),
            "legacy_dispatch": auto_man_runtime::config_engine::observation().legacy_dispatches,
        },
        "execution": {
            "owner": execution_leg,
            "engine": report.engine,
            "engine_is_identity_engine":
                report.engine == auto_lang_core::sdk_identity::EXECUTION_ENGINE_ID,
            "outcome": outcome_tag,
            "detail": value_repr,
            "steps": match &report.outcome {
                AceOutcome::Success { steps, .. } => Some(*steps),
                _ => None,
            },
            "stdout_bytes": stdout.len(),
            "stderr_bytes": stderr.len(),
            "dispatches": report.console.as_ref().map(|c| c.dispatches),
        },
        "script": path,
        "script_bytes": source.len(),
        "started_unix_ms": started_unix_ms,
    });

    match &report.outcome {
        AceOutcome::Success { value, .. } => {
            if !same_sdk {
                return refuse(
                    ai_mode,
                    "ace.sdk/identity-mismatch",
                    "the config leg, the execution leg and the serving engine did not \
                     report one SDK identity; the candidate run is refused",
                    Some(receipt),
                );
            }
            // Observable behaviour parity: the script's own streams come out
            // as they were captured, then the returned value prints exactly
            // where the legacy leg prints it (including its trailing spacing).
            if !stdout.is_empty() {
                use std::io::Write;
                let mut out = std::io::stdout();
                let _ = out.write_all(&stdout);
                let _ = out.flush();
            }
            if !stderr.is_empty() {
                use std::io::Write;
                let mut err = std::io::stderr();
                let _ = err.write_all(&stderr);
                let _ = err.flush();
            }
            write_receipt(&receipt);
            output_success(ai_mode, &ace_value_repr(*value));
            if !ai_mode {
                // Same trailing spacing the legacy leg prints after a script.
                println!();
            }
            Ok(())
        }
        AceOutcome::Diagnostic {
            stage,
            code,
            message,
            span,
        } => refuse(
            ai_mode,
            &format!("ace.diagnostic/{stage}/{code}"),
            &format!(
                "{message} (original span {:?})",
                span.map(|s| (s.start, s.end))
            ),
            Some(receipt),
        ),
        AceOutcome::Denied { code, message } => refuse(ai_mode, code, message, Some(receipt)),
        AceOutcome::ResourceDenied { kind, message } => refuse(
            ai_mode,
            &format!("ace.resource/{kind}"),
            message,
            Some(receipt),
        ),
        AceOutcome::Cancelled => refuse(
            ai_mode,
            "ace.run/cancelled",
            "the ACE run observed cancellation; the run is refused (never retried \
             on the legacy engine)",
            Some(receipt),
        ),
        AceOutcome::HostFailure { code, message } => refuse(ai_mode, code, message, Some(receipt)),
    }
}

/// PLAN-982（EXEC · 脚本 stdin，round 198 起两腿共用）：管道 stdin 预读
/// （非 TTY 且非空才取——与 `auto config` 的 CLI 侧同款判据）。
fn read_piped_stdin() -> Option<String> {
    if <std::io::Stdin as std::io::IsTerminal>::is_terminal(&std::io::stdin()) {
        return None;
    }
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    if input.is_empty() {
        None
    } else {
        Some(input)
    }
}

/// PLAN-982（EXEC · 脚本 stdin，round 198 起两腿共用）：入口形判据——
/// `fn main(<name> str)`（恰一个借用 `str` 形参）的脚本把管道 stdin 作为该
/// 形参的运行时文本（round-182 受检通道）；**其它形状的脚本不受影响**
/// （stdin 保持既有"不可读"边界，不静默改变语义）。机械文本扫描（与服务
/// 路由的 `text_route_param` 同纪律，不引入第二解析器）；**运行端是权威**
/// ——sys 腿的入口单元核对（`ac.runtime.text.v1`＋全 `StrView` 签名）与
/// console 腿的 MIR entry-sig 各自把守。
fn main_takes_single_str_param(source: &str) -> bool {
    let needle = "fn main(";
    let Some(at) = source.find(needle) else {
        return false;
    };
    let open = at + needle.len();
    let Some(rel) = source[open..].find(')') else {
        return false;
    };
    let close = open + rel;
    let params = source[open..close].trim();
    let mut words = params.split_whitespace();
    let _name = words.next();
    let ty = words.next();
    matches!(ty, Some("str")) && words.next().is_none()
}

/// PLAN-982（Q-982-2 裁定 (a) 第一步）：`AUTO_ACE_SCRIPT_SYS=1` 的显式候选
/// 入口——sys-stdlib 包车道（宿主桥 `run_sys_script`）。argv＝`[argv0,
/// ...args]`（宿主进程 argv[0] 沿用 legacy 契约），逐执行隔离；`fn main()`
/// 入口 ABI 不变，脚本经 stdlib `process.args()` 读参。失败按具名码退出。
/// round 198（aggregates 文本入口裁定）起：①wall/cancel 接线
/// （`AUTO_ACE_WALL_MS`/`AUTO_ACE_CANCEL_AFTER_MS` 与 console 腿同款——
/// 裁定④补现场）；②管道 stdin 按**受检入口签名**预读交 `fn main(input
/// str)`（request.input ⇒ 入口借用 `str` 实参，`ac.runtime.text.v1` 谱系）；
/// ③same_sdk 见证收据（配置腿/执行腿/实际服务引擎同一 SDK＋零旧 VM 分派）。
fn run_sys_script_lane(ai_mode: bool, path: &str, script_args: &[String]) -> miette::Result<()> {
    use crate::diag::output_success;
    use auto_lang_core::{AceOutcome, AceRequest};

    let started_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let source =
        std::fs::read_to_string(path).map_err(|e| miette::miette!("Failed to read {path}: {e}"))?;
    let args: Vec<&str> = script_args.iter().map(|s| s.as_str()).collect();

    // ---- Leg 1: configuration through the SDK's designated config owner (the
    // same_sdk witness's config leg; a bad configuration refuses BEFORE the run).
    let config_leg = auto_lang_core::sdk_identity::config_owner_identity();
    let execution_leg = auto_lang_core::sdk_identity::execution_owner_identity();
    let legs_same_sdk = auto_lang_core::sdk_identity::same_sdk(&config_leg, &execution_leg);
    let config = match read_project_config(path) {
        Ok(config) => config,
        Err(e) => {
            let receipt = json!({
                "schema": auto_lang_core::sdk_identity::SDK_IDENTITY_SCHEMA,
                "sdk": auto_lang_core::sdk_identity::sdk_identity(),
                "same_sdk": legs_same_sdk,
                "lane": "sys",
                "config": {
                    "owner": config_leg,
                    "present": true,
                    "error": e,
                },
                "execution": { "outcome": "not-started" },
                "script": path,
                "script_bytes": source.len(),
                "started_unix_ms": started_unix_ms,
            });
            return refuse(
                ai_mode,
                "ace.config/invalid",
                "configuration error",
                Some(receipt),
            );
        }
    };

    // ---- Leg 2 budgets (ruling ④: the real CLI wires AUTO_ACE_WALL_MS and the
    // cancel signal into the sys candidate request — same discipline as the
    // console leg; the door owns the budgets).
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(ms) = env_millis(CANCEL_AFTER_MS_ENV) {
        let flag = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            flag.store(true, Ordering::SeqCst);
        });
    }
    let wall = env_millis(WALL_MS_ENV).map(Duration::from_millis);

    // ---- stdin by the CHECKED entry shape (ruling ①): the CLI pre-reads the
    // pipe only when the entry is exactly-one-borrowed-`str` (mechanical scan,
    // `text_route_param` discipline); the RUNTIME is the authority — the door's
    // entry-unit check (`ac.runtime.text.v1` + all-StrView) names any mismatch.
    let stdin_text = read_piped_stdin().filter(|_| main_takes_single_str_param(&source));
    let stdin_bytes = stdin_text.as_ref().map(|t| t.len()).unwrap_or(0);

    let request = AceRequest {
        source: &source,
        entry_def_id: "d_main",
        capabilities: &[],
        grants: &[],
        provider: None,
        wall,
        cancel: Some(cancel.as_ref()),
        // PLAN-982（Q-982-2 第二步）: console 面——脚本可用 print/eprint/say，
        // 捕获回 AceConsoleReport 后按 A 片纪律再发（与 console 腿同
        // observable；待生产者 console 面切片落地后生效）。
        console: true,
        args: &args,
        input: stdin_text.as_deref(),
    };
    println!(
        "  ACE sys lane: AUTO_ACE_SCRIPT_SYS=1 (sys-stdlib packages lane, argv x{}, stdin {} bytes)",
        args.len(),
        stdin_bytes
    );
    let report = auto_lang_ace_host::run_sys_script(&request);

    // One SDK means: both legs mint their identity from this crate AND the
    // engine that actually served the run is the engine that identity names.
    let same_sdk =
        legs_same_sdk && report.engine == auto_lang_core::sdk_identity::EXECUTION_ENGINE_ID;

    let (stdout, stderr) = report
        .console
        .as_ref()
        .map(|c| (c.stdout.clone(), c.stderr.clone()))
        .unwrap_or_default();

    let (outcome_tag, value_repr) = match &report.outcome {
        AceOutcome::Success { value, .. } => ("success", Some(ace_value_repr(*value))),
        AceOutcome::Diagnostic {
            stage, code, span, ..
        } => (
            "diagnostic",
            Some(format!(
                "stage={stage} code={code} span={:?}",
                span.map(|s| (s.start, s.end))
            )),
        ),
        AceOutcome::Denied { code, .. } => ("denied", Some(code.clone())),
        AceOutcome::ResourceDenied { kind, .. } => ("resource-denied", Some(kind.clone())),
        AceOutcome::Cancelled => ("cancelled", None),
        AceOutcome::HostFailure { code, .. } => ("host-failure", Some(code.clone())),
    };

    let receipt = json!({
        "schema": auto_lang_core::sdk_identity::SDK_IDENTITY_SCHEMA,
        "sdk": auto_lang_core::sdk_identity::sdk_identity(),
        "same_sdk": same_sdk,
        "lane": "sys",
        "config": {
            "owner": config_leg,
            "path": config.pac_path.as_ref().map(|p| p.display().to_string()),
            "present": config.present,
            "values": config.values,
            "deps": config.deps,
            "targets": config.targets,
            // 实际引擎与派发计数（不是自报标签）——same_sdk 收据核对
            // 「配置与实际执行的 SDK 身份」＋「零旧 VM 分派」。
            "engine": auto_man_runtime::config_engine::engine_name(),
            "legacy_dispatch": auto_man_runtime::config_engine::observation().legacy_dispatches,
        },
        "execution": {
            "owner": execution_leg,
            "engine": report.engine,
            "engine_is_identity_engine":
                report.engine == auto_lang_core::sdk_identity::EXECUTION_ENGINE_ID,
            "outcome": outcome_tag,
            "detail": value_repr,
            "steps": match &report.outcome {
                AceOutcome::Success { steps, .. } => Some(*steps),
                _ => None,
            },
            "stdout_bytes": stdout.len(),
            "stderr_bytes": stderr.len(),
            "dispatches": report.console.as_ref().map(|c| c.dispatches),
            "argv_count": args.len(),
            "stdin_bytes": stdin_bytes,
        },
        "script": path,
        "script_bytes": source.len(),
        "started_unix_ms": started_unix_ms,
    });

    match &report.outcome {
        AceOutcome::Success { value, .. } => {
            if !same_sdk {
                return refuse(
                    ai_mode,
                    "ace.sdk/identity-mismatch",
                    "the config leg, the execution leg and the serving engine did not \
                     report one SDK identity; the candidate run is refused",
                    Some(receipt),
                );
            }
            if let Some(console) = &report.console {
                use std::io::Write;
                if !console.stdout.is_empty() {
                    let mut out = std::io::stdout();
                    let _ = out.write_all(&console.stdout);
                    let _ = out.flush();
                }
                if !console.stderr.is_empty() {
                    let mut err = std::io::stderr();
                    let _ = err.write_all(&console.stderr);
                    let _ = err.flush();
                }
            }
            write_receipt(&receipt);
            output_success(ai_mode, &ace_value_repr(*value));
            if !ai_mode {
                println!();
            }
            Ok(())
        }
        AceOutcome::Diagnostic {
            stage,
            code,
            message,
            span,
        } => refuse(
            ai_mode,
            &format!("ace.sys/{stage}/{code}"),
            &format!("{message} (original span {span:?})"),
            Some(receipt),
        ),
        AceOutcome::Denied { code, message } => refuse(ai_mode, code, message, Some(receipt)),
        AceOutcome::ResourceDenied { kind, message } => refuse(
            ai_mode,
            &format!("ace.sys/resource-{kind}"),
            message,
            Some(receipt),
        ),
        AceOutcome::Cancelled => refuse(
            ai_mode,
            "ace.sys/cancelled",
            "the sys-lane run observed cancellation; the run is refused (never retried \
             on the legacy engine)",
            Some(receipt),
        ),
        AceOutcome::HostFailure { code, message } => refuse(ai_mode, code, message, Some(receipt)),
    }
}

/// PLAN-982（982/RUST）：把宿主 A2R 驱动注册成运行时可见的渲染器。
///
/// 运行时 crate 不能依赖 `auto-lang-ace-host`（分层），因此由这里在启动时注入。
/// 幂等：重复调用只装一次。
pub(crate) fn install_rust_renderer() {
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
}
/// PLAN-982（982/WEBTARGET）：把 A2JS 驱动注册成 `ts` 目标可见的渲染器。
///
/// 与 `install_rust_renderer` 同一理由（运行时 crate 不能依赖宿主桥）。
pub(crate) fn install_script_renderer() {
    auto_man_runtime::script_engine::install_renderer(|source, module, entry| {
        auto_lang_ace_host::script_transpile::transpile_scalar_source(source, module, entry)
            .map(|fileset| {
                fileset
                    .files
                    .into_iter()
                    .map(|f| (f.name, f.text))
                    .collect()
            })
            .map_err(|e| (e.code.to_string(), e.message))
    });
}
/// PLAN-982（982/C）：把宿主**包级 A2C** 驱动注册成 C 目标可见的渲染器。
///
/// 与 `install_rust_renderer` 同一理由（运行时 crate 不能依赖宿主桥）。跨模块
/// 限定调用只在包级成立，因此渲染器吃**整工程**（`(模块名, 源码)` 列表）。
pub(crate) fn install_c_renderer() {
    auto_man_runtime::c_engine::install_renderer(|modules, entry| {
        auto_lang_ace_host::c_transpile::transpile_package_c(modules, entry)
            .map(|fileset| {
                fileset
                    .files
                    .into_iter()
                    .map(|f| (f.name, f.text))
                    .collect()
            })
            .map_err(|(code, message)| (code.to_string(), message))
    });
}
/// Console grants the candidate leg requests (explicit, per run).
fn console_grants() -> Vec<String> {
    vec!["console.stdout".to_string(), "console.stderr".to_string()]
}

/// The project configuration values the candidate leg resolved, recorded for
/// the one-SDK witness. Values are read through the SDK's config owner
/// (`auto_lang_core::config::AutoConfig`), never through a second parser.
struct ProjectConfig {
    pac_path: Option<PathBuf>,
    present: bool,
    values: serde_json::Value,
    /// Dependency targets in the product's own projection (`Pac::deps`).
    deps: usize,
    /// Total targets in the product's own projection.
    targets: usize,
}

/// Read the nearest `pac.at` above the script through the SDK config owner.
/// A document that fails to parse or evaluate is an error (the candidate leg
/// refuses); an absent document is not. The nearest-ancestor rule is the
/// product's own project-discovery rule.
fn read_project_config(script: &str) -> Result<ProjectConfig, String> {
    let script_path = Path::new(script);
    let start = script_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let Some(pac_path) = find_pac_at(&start) else {
        return Ok(ProjectConfig {
            pac_path: None,
            present: false,
            values: json!({}),
            deps: 0,
            targets: 0,
        });
    };
    let args = auto_val::Obj::new();
    // PLAN-982 E7：候选腿的配置读也走**显式引擎选择**（桥已装 ⇒ ACE），
    // 并把实际引擎记进候选见证（`config.engine`），失败具名上报不回退。
    let config = auto_man_runtime::config_engine::parse_file(&pac_path, &args)
        .map_err(|e| format!("`{}`: {e}", pac_path.display()))?;
    let mut values = serde_json::Map::new();
    for key in ["name", "version", "scene", "render", "title"] {
        if config.root.has_prop(key) {
            values.insert(
                key.to_string(),
                json!(config.root.get_prop(key).to_astr().as_str().to_string()),
            );
        }
    }
    // The product's own projection of the evaluated document: the target graph
    // (dependencies included) proves the multi-module shape was really
    // evaluated through the SDK, not degraded to a static default.
    let pac = auto_man_runtime::Pac::new(config);
    let deps = pac.deps().len();
    let targets = pac.targets.len();
    values.insert("pac_name".to_string(), json!(pac.name.as_str()));
    Ok(ProjectConfig {
        pac_path: Some(pac_path),
        present: true,
        values: serde_json::Value::Object(values),
        deps,
        targets,
    })
}

/// Walk up from the script's directory looking for `pac.at` (project root).
fn find_pac_at(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        let candidate = dir.join("pac.at");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn env_millis(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse::<u64>().ok()
}

/// Map the ACE-visible value onto the CLI's printable representation.
#[cfg(windows)]
fn ace_value_repr(value: auto_lang_core::AceValue) -> String {
    use auto_lang_core::AceValue;
    match value {
        AceValue::Unit => "nil".to_string(),
        AceValue::Bool(v) => v.to_string(),
        AceValue::I8(v) => v.to_string(),
        AceValue::U8(v) => v.to_string(),
        AceValue::I32(v) => v.to_string(),
        AceValue::U32(v) => v.to_string(),
        AceValue::I64(v) => v.to_string(),
        AceValue::U64(v) => v.to_string(),
        AceValue::F32(v) => v.to_string(),
        AceValue::F64(v) => v.to_string(),
        AceValue::Char(c) => c.to_string(),
        AceValue::Opaque(kind) => format!("<opaque:{kind}>"),
    }
}

/// A named refusal: machine JSON in `--ai` mode, a miette diagnostic
/// otherwise. The same code reaches the receipt so a failed run is still
/// attributable.
/// PLAN-982 EXEC：候选腿只服务**裸脚本入口**；其余 run 家族子命令（项目
/// run/service/req/debug/test）在当前切片内没有 ACE 实现，因此**具名拒绝**，
/// 绝不静默落回旧执行器（EXEC 的"未知目标不 legacy fallback"）。
pub(crate) fn refuse_unsupported_command(ai_mode: bool, command: &str) -> miette::Result<()> {
    refuse(
        ai_mode,
        "ace.exec/unsupported-command",
        &format!(
            "`auto {command}` is not served by the ACE candidate leg yet; install nothing and \
             re-run without the candidate selection, or wait for the 982/EXEC slice"
        ),
        None,
    )
}
fn refuse(
    ai_mode: bool,
    code: &str,
    message: &str,
    receipt: Option<serde_json::Value>,
) -> miette::Result<()> {
    if let Some(mut receipt) = receipt {
        receipt["refusal"] = json!({ "code": code, "message": message });
        write_receipt(&receipt);
    }
    if ai_mode {
        eprintln!(
            "{}",
            json!({ "status": "error", "code": code, "message": message })
        );
        std::process::exit(1);
    }
    Err(miette::miette!("{code}: {message}"))
}

/// Write the one-SDK evidence line when a receipt path is configured.
fn write_receipt(receipt: &serde_json::Value) {
    let Ok(path) = std::env::var(RECEIPT_ENV) else {
        return;
    };
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(receipt) {
        let _ = std::fs::write(&path, text);
    }
}

/// PLAN-982（EXEC · round 186/189）：候选腿的 `auto test`——ACE 项目测试门
/// （`run_ace_project_tests` + 宿主 ProjectHost，971 形状）。收集语义与既有
/// test runner 同形（单文件或目录递归）；每个零参 `int` 函数即一个测试候选
///（宿主发现合同）。round 189 起 `--filter` 一并走 ACE：限定名子串、**执行前
/// 筛选**、零匹配＝零执行＋退出 0（legacy 匹配/退出语义；`discovered` 仍为
/// 完整生产者事实）。失败 ⇒ 非零退出。
pub(crate) fn run_tests(
    ai_mode: bool,
    dir: Option<String>,
    filter: Option<&str>,
) -> miette::Result<()> {
    use auto_lang_core::{
        run_ace_project_tests, AceEngineChoice, AceModuleSpec, AceTestRunRequest,
    };

    let target = dir.unwrap_or_else(|| ".".to_string());
    let path = std::path::Path::new(&target);
    let files: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        let mut found = Vec::new();
        fn collect_at_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        collect_at_files(&p, out);
                    } else if p.extension().map_or(false, |e| e == "at" || e == "au") {
                        out.push(p);
                    }
                }
            }
        }
        collect_at_files(path, &mut found);
        found.sort();
        if found.is_empty() {
            return refuse(
                ai_mode,
                "ace.test/no-sources",
                &format!("no .at files found in '{target}'"),
                None,
            );
        }
        found
    } else {
        return refuse(
            ai_mode,
            "ace.test/target-missing",
            &format!("'{target}' not found"),
            None,
        );
    };

    let mut specs: Vec<AceModuleSpec> = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file)
            .map_err(|e| miette::miette!("Failed to read {}: {e}", file.display()))?;
        let id = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "main".to_string());
        sources.push((id.clone(), source));
    }
    for (id, source) in &sources {
        specs.push(AceModuleSpec {
            package: "app",
            version: "1.0.0",
            id,
            source,
        });
    }
    let entry_module = format!(
        "app::{}",
        sources.first().map(|(id, _)| id.as_str()).unwrap_or("main")
    );
    // 包线显式能力键（packages 前门合同；缺位 = source.module-capability 具名拒）。
    let capabilities = vec!["ac.source.modules.packages/v1".to_string()];
    let request = AceTestRunRequest {
        modules: &specs,
        entry_module: &entry_module,
        entry_def_id: "d_main",
        capabilities: &capabilities,
        filter,
        wall: None,
        cancel: None,
    };
    let report = run_ace_project_tests(AceEngineChoice::AceAV, &request);
    // 诊断可见性：非 Passed 终态把 outcome 具名带出（NoTestsDiscovered /
    // Diagnostic 的 message 是排障的第一现场）。
    if !matches!(report.outcome, auto_lang_core::AceTestRunOutcome::Passed) {
        let detail = match &report.outcome {
            auto_lang_core::AceTestRunOutcome::Failed { message } => format!("failed: {message}"),
            auto_lang_core::AceTestRunOutcome::NoTestsDiscovered { message } => {
                format!("no tests discovered: {message}")
            }
            auto_lang_core::AceTestRunOutcome::Diagnostic(d) => {
                format!("diagnostic {}: {} ({:?})", d.code, d.message, d.span)
            }
            other => format!("{other:?}"),
        };
        eprintln!("ACE test outcome: {detail}");
    }
    let mut failures = 0usize;
    for test in &report.tests {
        let state = if test.passed { "ok" } else { "FAILED" };
        if !test.passed {
            failures += 1;
        }
        println!("test {} ... {state}", test.name);
    }
    // 零匹配可见性（legacy 退出 0，但不静默）：过滤词无命中 ⇒ 一行具名
    // 输出＋discovered 计数对照（`discovered` 是完整生产者事实）。
    if filter.is_some() && report.tests.is_empty() {
        println!(
            "0 tests matched '{}' (discovered {} candidate(s) in the project)",
            filter.unwrap_or_default(),
            report.discovered.len()
        );
    }
    println!(
        "ACE test report: engine={} tests={} failures={} exit_code={}",
        report.engine,
        report.tests.len(),
        failures,
        report.exit_code
    );
    if report.exit_code != 0 {
        return Err(miette::miette!(
            "ace.test/failures: protocol exit {} (failures={failures})",
            report.exit_code
        ));
    }
    Ok(())
}

/// PLAN-982（EXEC · round 187）：候选腿的 `auto debug <file>`——ACE 调试门
/// （971 形状：**入口断点 + continue 到终态**，非交互）。交互式 GDB 面与
/// agent 协议保持具名拒绝（未迁移）。失败码：目标缺失/编译拒/资源/取消 ⇒
/// 非零退出（与 test 的失败码纪律同族）。
pub(crate) fn run_debug(ai_mode: bool, file: &str) -> miette::Result<()> {
    use auto_lang_core::{
        start_ace_debug, AceDebugCommand, AceDebugRequest, AceEngineChoice, AceModuleSpec,
        AceProjectOutcome,
    };

    let path = std::path::Path::new(file);
    if !path.is_file() {
        return refuse(
            ai_mode,
            "ace.debug/target-missing",
            &format!("'{file}' not found"),
            None,
        );
    }
    let source =
        std::fs::read_to_string(path).map_err(|e| miette::miette!("Failed to read {file}: {e}"))?;
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "main".to_string());
    let spec = AceModuleSpec {
        package: "app",
        version: "1.0.0",
        id: &id,
        source: &source,
    };
    let entry_module = format!("app::{id}");
    // 包线显式能力键（packages 前门合同；缺位 = source.module-capability 具名拒）。
    let capabilities = vec!["ac.source.modules.packages/v1".to_string()];
    // 无显式断点：会话的起点 stop 即入口断点（start 在入口首指令前暂停）。
    let specs = [spec];
    let request = AceDebugRequest::simple(&specs, &entry_module, &capabilities, &[]);
    let mut session = match start_ace_debug(AceEngineChoice::AceAV, &request) {
        Ok(session) => session,
        Err(e) => {
            // 编译拒/链接拒/入口不可暂停都是具名失败（diagnostic 是第一现场）。
            let detail = e
                .diagnostic
                .as_ref()
                .map(|d| format!(" ({}) {}", d.stage, d.message))
                .unwrap_or_default();
            return refuse(ai_mode, &e.code, &format!("{}{detail}", e.message), None);
        }
    };
    // 入口停：报出生产者自己的源级位置（模块/函数，原始 span 坐标系）。
    let entry = session.snapshot();
    match entry.frames.first() {
        Some(frame) => println!(
            "ACE debug: entry stop at {}::{}",
            frame.module, frame.function
        ),
        None => println!("ACE debug: entry stop (steps {})", entry.steps),
    }
    println!("ACE debug: continuing to the terminal state");
    // 无断点 ⇒ 一次 continue 内部即跑到终态（finish/cancel/wall）。
    let terminal = session.resume(AceDebugCommand::Continue);
    session.close();
    match terminal.outcome {
        Some(AceProjectOutcome::Success { value, steps }) => {
            println!(
                "ACE debug: finished in {steps} steps, value = {}",
                ace_value_repr(value)
            );
            Ok(())
        }
        Some(AceProjectOutcome::Diagnostic { code, message, .. }) => refuse(
            ai_mode,
            &format!("ace.debug/{code}"),
            &format!("the debugged program was rejected: {message}"),
            None,
        ),
        Some(AceProjectOutcome::Denied { code, message }) => refuse(ai_mode, &code, &message, None),
        Some(AceProjectOutcome::ResourceDenied { kind, message }) => refuse(
            ai_mode,
            &format!("ace.debug/resource-{kind}"),
            &message,
            None,
        ),
        Some(AceProjectOutcome::Cancelled) => refuse(
            ai_mode,
            "ace.debug/cancelled",
            "the debug session observed cancellation",
            None,
        ),
        Some(AceProjectOutcome::HostFailure { code, message }) => {
            refuse(ai_mode, &code, &message, None)
        }
        None => refuse(
            ai_mode,
            "ace.debug/no-terminal-outcome",
            "the session ended without a terminal outcome",
            None,
        ),
    }
}

/// PLAN-982（EXEC · round 187）：候选腿的 `auto serve`——ACE 会话 daemon
/// （970 T-03：管道/stdio JSONL，legacy 方法名；会话宿主为 970 桥）。前台/
/// stdio/后台三形态与 legacy serve 同形；会话生命周期＝daemon 进程（与
/// legacy 一致，不挂 T-08 store——重启即清，留作后续显式面）。
pub(crate) fn run_serve(
    foreground: bool,
    stdio: bool,
    pipe_name: &str,
    max_sessions: usize,
    timeout_secs: u64,
) -> miette::Result<()> {
    // PLAN-982（EXEC · round 190）：legacy `--max-sessions`/`--timeout` 的 1:1
    // 映射（两旗标的语义与默认值——20 会话／1800s 空闲——本就与
    // `AceSessionLimits::default()` 一致）；legacy `--timeout 0`＝不过期 ⇒
    // u64::MAX 秒（宿主清扫按秒比较）。
    let limits = auto_lang_core::AceSessionLimits {
        max_sessions: max_sessions.max(1),
        idle_timeout_secs: if timeout_secs == 0 {
            u64::MAX
        } else {
            timeout_secs
        },
        ..auto_lang_core::AceSessionLimits::default()
    };
    let _guard = install_session_host(limits)?;
    if stdio {
        auto_lang_core::AceDaemon::new().run_stdio();
    } else if foreground {
        auto_lang_core::AceDaemon::new().run_pipe(pipe_name);
    } else {
        // 后台形态：与 legacy serve 同一 spawn 纪律（分离进程、管道名与
        // 限额旗标透传给前台子进程）。
        let exe = std::env::current_exe()
            .map_err(|e| miette::miette!("Cannot find auto executable: {}", e))?;
        let mut spec = auto_tool_execution::ExecutionSpec::new([
            exe.as_os_str().to_os_string(),
            "serve".into(),
            "--foreground".into(),
            "--pipe-name".into(),
            pipe_name.into(),
            "--max-sessions".into(),
            max_sessions.to_string().into(),
            "--timeout".into(),
            timeout_secs.to_string().into(),
        ]);
        spec.stdin = auto_tool_execution::StdioPolicy::Inherit;
        spec.stdout = auto_tool_execution::StdioPolicy::Inherit;
        spec.stderr = auto_tool_execution::StdioPolicy::Inherit;
        #[cfg(windows)]
        {
            spec.windows_creation_flags = 0x0800_0000; // CREATE_NO_WINDOW
        }
        auto_tool_execution::spawn_detached(&spec)
            .map_err(|e| miette::miette!("Failed to start daemon: {}", e))?;
        println!("ACE session daemon started (pipe: {pipe_name})");
    }
    Ok(())
}

/// Install the ACE session host (named failure on install — never a legacy
/// route). Round 190：serve 腿带 CLI 限额；one-shot 腿用默认限额。
fn install_session_host(
    limits: auto_lang_core::AceSessionLimits,
) -> miette::Result<auto_lang_ace_host::session_bridge::SessionHostGuard> {
    auto_lang_ace_host::session_bridge::install_session_host(limits)
        .map_err(|e| miette::miette!("ace.session/host-install: {e}"))
}

/// PLAN-982（EXEC · round 187）：候选腿 `auto req` 的**匿名 one-shot** 腿
/// ——legacy 面在此调用 `eval_one_shot`（旧 VM 进程内求值）；候选腿改为
/// 进程内装 ACE 会话宿主，临时会话求值后即关（输出/退出形状与 legacy 面
/// 零改：值/JSON 形状、错误走 stderr、退出 0）。会话化路径（--session/
/// --list/--new-session）不经此——它们是 wire 客户端，talks to whichever
/// daemon answers the pipe（ACE daemon 由 `run_serve` 起）。
pub(crate) fn run_req_one_shot(json: bool, code: &str) -> miette::Result<()> {
    use auto_lang_core::{AceSessionOp, AceSessionOutcome, AceSessionValue};

    let _guard = install_session_host(auto_lang_core::AceSessionLimits::default())?;
    let created = auto_lang_core::ace_session_create(None, None);
    let Some(session) = created.session.clone() else {
        return Err(miette::miette!(
            "ace.session/create: no session id in report (outcome={:?})",
            created.outcome
        ));
    };
    let report = auto_lang_core::ace_session_op(
        &session,
        AceSessionOp::Eval {
            input: code,
            wall: None,
            cancel: None,
        },
    );
    let _ = auto_lang_core::ace_session_op(&session, AceSessionOp::Close);
    let ok = matches!(report.outcome, AceSessionOutcome::Ok);
    let value = match report.value {
        AceSessionValue::Int(n) => n.to_string(),
        AceSessionValue::Bool(b) => b.to_string(),
        AceSessionValue::None => match &report.outcome {
            AceSessionOutcome::Diagnostic { diagnostics, .. } => diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_default(),
            AceSessionOutcome::Denied { message, .. }
            | AceSessionOutcome::ResourceDenied { message, .. }
            | AceSessionOutcome::HostFailure { message, .. } => message.clone(),
            _ => String::new(),
        },
    };
    if json {
        let status = if ok { "ok" } else { "error" };
        println!(
            "{}",
            serde_json::json!({ "status": status, "value": value })
        );
    } else if ok {
        println!("{}", value);
    } else {
        eprintln!("{}", value);
    }
    Ok(())
}
