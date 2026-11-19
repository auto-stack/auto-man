//! PLAN-982（982/RUST）：**Rust 转译引擎的显式选择与观测**。
//!
//! 与 `config_engine` 同族、同纪律：选择是**具名**的、被选择的那条路径会记账，
//! 失败/未实现不静默落到另一条引擎。
//!
//! - `AUTO_ACE_RUST=1|true|ace` ⇒ A2R（`auto-lang-ace-host` 驱动，需
//!   `ace-candidate` 特性）；未设置 ⇒ 旧 `transpile_rust*`（现状不变）；
//! - `AUTO_ACE_RUST=legacy|0|vm` ⇒ 显式旧路径（对照臂）；
//! - 未知取值 ⇒ 保持默认（旧路径），**不**静默改成 A2R；
//! - 每次派发都进计数器，`observation()` 给真实数字（测试与候选见证用）。
//!
//! 记账是本模块唯一职责的一半：真正的渲染在 `crates/auto` 的 CLI 层或宿主
//! 驱动里，本模块只决定"哪条腿"并计数。

use std::sync::atomic::{AtomicU64, Ordering};

/// 本次进程选择的 Rust 转译引擎。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustEngineChoice {
    /// 新 A2R（`ace-a2x-rust`，经 `auto-lang-ace-host` 驱动）。
    AceA2r,
    /// 旧 `auto_lang_core::trans_rust*`（默认，未迁移前的行为）。
    Legacy,
}

static ACE_A2R_DISPATCHES: AtomicU64 = AtomicU64::new(0);
static LEGACY_DISPATCHES: AtomicU64 = AtomicU64::new(0);

/// 环境变量名（显式选择/对照）。
pub const RUST_ENGINE_ENV: &str = "AUTO_ACE_RUST";

/// 当前选择。
pub fn choice() -> RustEngineChoice {
    match std::env::var(RUST_ENGINE_ENV).as_deref() {
        Ok("1") | Ok("true") | Ok("yes") | Ok("ace") | Ok("a2r") => RustEngineChoice::AceA2r,
        Ok("legacy") | Ok("0") | Ok("false") | Ok("vm") => RustEngineChoice::Legacy,
        // 未设置或未知取值：保持现状（旧路径），绝不静默切到 A2R。
        _ => RustEngineChoice::Legacy,
    }
}

/// A2R 是否被显式选中。
pub fn ace_a2r_selected() -> bool {
    choice() == RustEngineChoice::AceA2r
}

/// 记录一次 A2R 派发（由 CLI 在真正调用宿主驱动前记账）。
pub fn note_ace_a2r_dispatch() {
    ACE_A2R_DISPATCHES.fetch_add(1, Ordering::Relaxed);
}

/// 记录一次旧路径派发（默认路径也必须可观测——否则"零 legacy"无从谈起）。
pub fn note_legacy_dispatch() {
    LEGACY_DISPATCHES.fetch_add(1, Ordering::Relaxed);
}

/// A2R 文件渲染器（由 `auto` crate 在启动时注入宿主驱动）。
///
/// 参数：`(source, module, entry)`；返回 `(文件名, 文本)` 列表或 `(code, message)`。
/// 运行时 crate 不能依赖 `auto-lang-ace-host`（分层），因此用**注册**而不是直接调用。
pub type RustRenderer = fn(&str, &str, &str) -> Result<Vec<(String, String)>, (String, String)>;

static RENDERER: std::sync::Mutex<Option<RustRenderer>> = std::sync::Mutex::new(None);

/// 安装渲染器（幂等；返回是否首次安装）。
pub fn install_renderer(renderer: RustRenderer) -> bool {
    let mut slot = RENDERER.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(renderer);
    true
}

/// 是否已安装渲染器（默认构建没有 ⇒ 项目面保持旧路径/具名拒绝）。
pub fn renderer_installed() -> bool {
    RENDERER.lock().map(|slot| slot.is_some()).unwrap_or(false)
}

/// 通过安装的渲染器渲染一个单模块工程（未安装 ⇒ 具名错误，不静默）。
pub fn render_files(
    source: &str,
    module: &str,
    entry: &str,
) -> Result<Vec<(String, String)>, (String, String)> {
    let renderer = *RENDERER
        .lock()
        .map_err(|_| {
            (
                "ace.rust/renderer-poisoned".to_string(),
                "the A2R renderer registry is poisoned".to_string(),
            )
        })?
        .as_ref()
        .ok_or_else(|| {
            (
                "ace.rust/renderer-unavailable".to_string(),
                "no A2R renderer is installed in this build".to_string(),
            )
        })?;
    renderer(source, module, entry)
}
/// PLAN-982（982/RUST · round 200）：**包级** A2R 渲染器——参数
/// `(modules, entry)`（工程全部 `.at` 的 `(模块名, 源码)` ＋入口模块名）；
/// 返回 `(文件名, 文本)` 列表或 `(code, message)`。与单模块渲染器同一分层
/// 理由（运行时 crate 不能依赖 `auto-lang-ace-host`，注册注入）。
pub type PackageRustRenderer =
    fn(&[(String, String)], &str) -> Result<Vec<(String, String)>, (String, String)>;

static PACKAGE_RENDERER: std::sync::Mutex<Option<PackageRustRenderer>> =
    std::sync::Mutex::new(None);

/// 安装包级渲染器（幂等；返回是否首次安装）。
pub fn install_package_renderer(renderer: PackageRustRenderer) -> bool {
    let mut slot = PACKAGE_RENDERER.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(renderer);
    true
}

/// 通过安装的包级渲染器渲染整个多模块工程（未安装 ⇒ 具名错误，不静默）。
pub fn render_package_files(
    modules: &[(String, String)],
    entry: &str,
) -> Result<Vec<(String, String)>, (String, String)> {
    let renderer = *PACKAGE_RENDERER
        .lock()
        .map_err(|_| {
            (
                "ace.rust/renderer-poisoned".to_string(),
                "the A2R package renderer registry is poisoned".to_string(),
            )
        })?
        .as_ref()
        .ok_or_else(|| {
            (
                "ace.rust/renderer-unavailable".to_string(),
                "no A2R package renderer is installed in this build".to_string(),
            )
        })?;
    renderer(modules, entry)
}

/// 回执文件名（测试钩子）：写 `{engine, a2r_dispatches, legacy_dispatches}`。
pub const RUST_PROOF_ENV: &str = "AUTO_ACE_RUST_PROOF";

/// 若 `AUTO_ACE_RUST_PROOF` 指定了路径，写一份**真实计数**回执（原子替换）。
///
/// 这是"旧 RustTrans 零 dispatch"的**对外**见证面：进程内计数在真 CLI 测试里
/// 跨不过进程边界，回执把数字落盘，测试读它而不是相信叙述。
pub fn write_proof_if_requested() {
    let Some(path) = std::env::var_os(RUST_PROOF_ENV) else {
        return;
    };
    let (ace, legacy, engine) = observation();
    let payload = serde_json::json!({
        "schema": "auto.plan982.rust.proof/1",
        "engine": engine,
        "a2r_dispatches": ace,
        "legacy_dispatches": legacy,
    });
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let tmp = format!("{}.tmp", path.to_string_lossy());
    if std::fs::write(&tmp, payload.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}
/// 引擎名（回执/诊断用）。
pub fn engine_name() -> &'static str {
    match choice() {
        RustEngineChoice::AceA2r => "a2r",
        RustEngineChoice::Legacy => "legacy",
    }
}

/// 真实计数：(a2r 派发数, legacy 派发数, 当前引擎名)。
pub fn observation() -> (u64, u64, &'static str) {
    (
        ACE_A2R_DISPATCHES.load(Ordering::Relaxed),
        LEGACY_DISPATCHES.load(Ordering::Relaxed),
        engine_name(),
    )
}

/// 测试用：清零计数并临时设置/清除环境变量（进程内串行，测试需自己加锁）。
pub fn set_choice_for_test(ace: Option<bool>) {
    match ace {
        Some(true) => std::env::set_var(RUST_ENGINE_ENV, "ace"),
        Some(false) => std::env::remove_var(RUST_ENGINE_ENV),
        None => std::env::remove_var(RUST_ENGINE_ENV),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn p982_rust_engine_env_selects_and_counts() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_choice_for_test(None);
        assert_eq!(choice(), RustEngineChoice::Legacy);
        assert_eq!(engine_name(), "legacy");
        note_legacy_dispatch();
        assert_eq!(observation().1, 1);

        set_choice_for_test(Some(true));
        assert_eq!(choice(), RustEngineChoice::AceA2r);
        note_ace_a2r_dispatch();
        let (ace, legacy, name) = observation();
        assert_eq!((ace, legacy, name), (1, 1, "a2r"));

        // 未知取值：保持现状（旧路径），不静默切 A2R。
        std::env::set_var(RUST_ENGINE_ENV, "not-an-engine");
        assert_eq!(choice(), RustEngineChoice::Legacy);
        std::env::remove_var(RUST_ENGINE_ENV);
    }
}
