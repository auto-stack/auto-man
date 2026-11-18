//! PLAN-982（982/C）：**C 转译引擎的显式选择与观测**。
//!
//! 与 `config_engine` 同族、同纪律：选择是**具名**的、被选择的那条路径会记账，
//! 失败/未实现不静默落到另一条引擎。
//!
//! - `AUTO_ACE_RUST=1|true|ace` ⇒ A2C（`auto-lang-ace-host` 驱动，需
//!   `ace-candidate` 特性）；未设置 ⇒ 旧 `transpile_c*`（现状不变）；
//! - `AUTO_ACE_RUST=legacy|0|vm` ⇒ 显式旧路径（对照臂）；
//! - 未知取值 ⇒ 保持默认（旧路径），**不**静默改成 A2C；
//! - 每次派发都进计数器，`observation()` 给真实数字（测试与候选见证用）。
//!
//! 记账是本模块唯一职责的一半：真正的渲染在 `crates/auto` 的 CLI 层或宿主
//! 驱动里，本模块只决定"哪条腿"并计数。

use std::sync::atomic::{AtomicU64, Ordering};

/// 本次进程选择的 Rust 转译引擎。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CEngineChoice {
    /// 新 A2C（`ace-a2x-c`，经 `auto-lang-ace-host` 驱动）。
    AceA2c,
    /// 旧 `auto_lang_core::trans_c*`（默认，未迁移前的行为）。
    Legacy,
}

static ACE_A2C_DISPATCHES: AtomicU64 = AtomicU64::new(0);
static LEGACY_DISPATCHES: AtomicU64 = AtomicU64::new(0);

/// 环境变量名（显式选择/对照）。
pub const C_ENGINE_ENV: &str = "AUTO_ACE_C";

/// 当前选择。
pub fn choice() -> CEngineChoice {
    match std::env::var(C_ENGINE_ENV).as_deref() {
        Ok("1") | Ok("true") | Ok("yes") | Ok("ace") | Ok("a2c") => CEngineChoice::AceA2c,
        Ok("legacy") | Ok("0") | Ok("false") | Ok("vm") => CEngineChoice::Legacy,
        // 未设置或未知取值：保持现状（旧路径），绝不静默切到 A2C。
        _ => CEngineChoice::Legacy,
    }
}

/// A2C 是否被显式选中。
pub fn ace_a2c_selected() -> bool {
    choice() == CEngineChoice::AceA2c
}

/// 记录一次 A2C 派发（由 CLI 在真正调用宿主驱动前记账）。
pub fn note_ace_a2c_dispatch() {
    ACE_A2C_DISPATCHES.fetch_add(1, Ordering::Relaxed);
}

/// 记录一次旧路径派发（默认路径也必须可观测——否则"零 legacy"无从谈起）。
pub fn note_legacy_dispatch() {
    LEGACY_DISPATCHES.fetch_add(1, Ordering::Relaxed);
}

/// 包级 A2C 文件渲染器（由 `auto` crate 在启动时注入宿主驱动）。
///
/// 参数：`(modules, entry)`——`modules` = `(模块名, 源码)` 列表（工程全部
/// `.at`），`entry` = 入口模块名。C 的跨模块语义（`use util` + 限定调用）
/// **只在包级成立**：逐文件渲染无法解析跨模块引用。返回 `(文件名, 文本)`
/// 列表或 `(code, message)`。运行时 crate 不能依赖 `auto-lang-ace-host`
/// （分层），因此用**注册**而不是直接调用。
pub type CRenderer =
    fn(&[(String, String)], &str) -> Result<Vec<(String, String)>, (String, String)>;

static CRENDERER: std::sync::Mutex<Option<CRenderer>> = std::sync::Mutex::new(None);

/// 安装渲染器（幂等；返回是否首次安装）。
pub fn install_renderer(renderer: CRenderer) -> bool {
    let mut slot = CRENDERER.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(renderer);
    true
}

/// 是否已安装渲染器（默认构建没有 ⇒ 项目面保持旧路径/具名拒绝）。
pub fn renderer_installed() -> bool {
    CRENDERER.lock().map(|slot| slot.is_some()).unwrap_or(false)
}

/// 通过安装的渲染器把**整个工程**一次渲染成 C fileset（未安装 ⇒ 具名错误）。
pub fn render_package_files(
    modules: &[(String, String)],
    entry: &str,
) -> Result<Vec<(String, String)>, (String, String)> {
    let renderer = *CRENDERER
        .lock()
        .map_err(|_| {
            (
                "ace.c/renderer-poisoned".to_string(),
                "the A2C renderer registry is poisoned".to_string(),
            )
        })?
        .as_ref()
        .ok_or_else(|| {
            (
                "ace.c/renderer-unavailable".to_string(),
                "no A2C renderer is installed in this build".to_string(),
            )
        })?;
    renderer(modules, entry)
}
/// 回执文件名（测试钩子）：写 `{engine, a2c_dispatches, legacy_dispatches}`。
pub const C_PROOF_ENV: &str = "AUTO_ACE_C_PROOF";

/// 若 `AUTO_ACE_RUST_PROOF` 指定了路径，写一份**真实计数**回执（原子替换）。
///
/// 这是"旧 RustTrans 零 dispatch"的**对外**见证面：进程内计数在真 CLI 测试里
/// 跨不过进程边界，回执把数字落盘，测试读它而不是相信叙述。
pub fn write_proof_if_requested() {
    let Some(path) = std::env::var_os(C_PROOF_ENV) else {
        return;
    };
    let (ace, legacy, engine) = observation();
    let payload = serde_json::json!({
        "schema": "auto.plan982.c.proof/1",
        "engine": engine,
        "a2c_dispatches": ace,
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
        CEngineChoice::AceA2c => "a2c",
        CEngineChoice::Legacy => "legacy",
    }
}

/// 真实计数：(a2c 派发数, legacy 派发数, 当前引擎名)。
pub fn observation() -> (u64, u64, &'static str) {
    (
        ACE_A2C_DISPATCHES.load(Ordering::Relaxed),
        LEGACY_DISPATCHES.load(Ordering::Relaxed),
        engine_name(),
    )
}

/// 测试用：清零计数并临时设置/清除环境变量（进程内串行，测试需自己加锁）。
pub fn set_choice_for_test(ace: Option<bool>) {
    match ace {
        Some(true) => std::env::set_var(C_ENGINE_ENV, "ace"),
        Some(false) => std::env::remove_var(C_ENGINE_ENV),
        None => std::env::remove_var(C_ENGINE_ENV),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn p982_c_engine_env_selects_and_counts() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_choice_for_test(None);
        assert_eq!(choice(), CEngineChoice::Legacy);
        assert_eq!(engine_name(), "legacy");
        note_legacy_dispatch();
        assert_eq!(observation().1, 1);

        set_choice_for_test(Some(true));
        assert_eq!(choice(), CEngineChoice::AceA2c);
        note_ace_a2c_dispatch();
        let (ace, legacy, name) = observation();
        assert_eq!((ace, legacy, name), (1, 1, "a2c"));

        // 未知取值：保持现状（旧路径），不静默切 A2C。
        std::env::set_var(C_ENGINE_ENV, "not-an-engine");
        assert_eq!(choice(), CEngineChoice::Legacy);
        std::env::remove_var(C_ENGINE_ENV);
    }
}
