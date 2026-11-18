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
    // The AV lane has no argv surface (PLAN-969 T-08 named refusal). The A
    // slice refuses by name instead of dropping the arguments.
    if !script_args.is_empty() {
        return refuse(
            ai_mode,
            "ace.args/unsupported",
            "the ACE candidate leg has no argv surface in this slice; refusing \
             rather than dropping the script arguments or falling back",
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

    /// PLAN-982（EXEC · 脚本 stdin）：管道 stdin 预读（非 TTY 且非空才取——与
    /// `auto config` 的 CLI 侧同款判据）。
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

    /// PLAN-982（EXEC · 脚本 stdin）：入口形判据——`fn main(<name> str)`（恰一个
    /// 借用 `str` 形参）的脚本把管道 stdin 作为该形参的运行时文本（round-182
    /// 受检通道）；**其它形状的脚本不受影响**（stdin 保持既有"不可读"边界，
    /// 不静默改变语义）。机械文本扫描（与服务路由的 `text_route_param` 同纪律，
    /// 不引入第二解析器）。
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

/// PLAN-982（EXEC · round 186）：候选腿的 `auto test`（无过滤词）——ACE
/// 项目测试门（`run_ace_project_tests` + 宿主 ProjectHost，971 形状）。
/// 收集语义与既有 test runner 同形（单文件或目录递归）；每个零参 `int`
/// 函数即一个测试候选（宿主发现合同）。失败 ⇒ 非零退出。
pub(crate) fn run_tests(ai_mode: bool, dir: Option<String>) -> miette::Result<()> {
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
