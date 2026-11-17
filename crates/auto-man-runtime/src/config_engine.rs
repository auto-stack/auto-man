//! PLAN-982（B-CONFIG-I，E7）：pac 配置求值的**引擎选择与观测**。
//!
//! 纪律（计划 §3.5 / B 切片）：
//! - 默认走显式 ACE（`ConfigEngineChoice::AceAv`），**不静默回退**——ACE 装了却
//!   失败时，错误具名上报（`ace.diagnostic/...`、`ace.host/...`），不会改用旧引擎；
//! - 宿主没装 ACE 执行桥时（库被无桥宿主链接，例如 `auto-man-runtime` 的
//!   lib 测试）才落到 legacy，且这条选择可观测、可显式覆盖；
//! - 覆盖入口：环境变量 `AUTO_CONFIG_ENGINE=av|legacy`，以及测试用的
//!   [`set_choice_for_test`]（进程内单值，测试串行化由调用方负责）。
//!
//! 观测（不是自报标签）：[`observation`] 返回**实际**派发过的次数——
//! ACE 成功/失败各计一次 ACE 派发，legacy 计一次 legacy 派发；`engine` 是本
//! 进程当前选择。

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use auto_lang_core::ace_config::ConfigEngineChoice;
use auto_lang_core::config::AutoConfig;
use auto_lang_core::AutoResult;
use auto_val::Obj;

/// 覆盖值：0 = 按环境/桥自动，1 = 强制 ACE，2 = 强制 legacy。
static OVERRIDE: AtomicU8 = AtomicU8::new(0);
static ACE_DISPATCHES: AtomicU64 = AtomicU64::new(0);
static LEGACY_DISPATCHES: AtomicU64 = AtomicU64::new(0);

/// 本进程这一跑用哪个引擎（显式覆盖 → 环境 → 桥是否已装）。
pub fn choice() -> ConfigEngineChoice {
    match OVERRIDE.load(Ordering::Relaxed) {
        1 => return ConfigEngineChoice::AceAv,
        2 => return ConfigEngineChoice::LegacyVm,
        _ => {}
    }
    if let Ok(v) = std::env::var("AUTO_CONFIG_ENGINE") {
        match v.as_str() {
            "legacy" | "vm" => return ConfigEngineChoice::LegacyVm,
            // 显式 ACE：即使本构建没装桥也走 ACE（失败具名），不静默回退。
            "av" | "ace" => return ConfigEngineChoice::AceAv,
            _ => {}
        }
    }
    if auto_lang_core::ace_executor_installed() {
        ConfigEngineChoice::AceAv
    } else {
        // 没有执行桥的宿主（例如只链接 SDK 的库、或未开 `ace-candidate` 的
        // 默认 `auto` 构建）：legacy 是**环境事实**，不是语义回退——桥装上后
        // 同一路径立即走 ACE。这条选择必须**可观测**（B-CONFIG-R F-982B-03）：
        // 每次进程只提示一次，避免每份 pac 都刷屏。
        note_legacy_fallback_once();
        ConfigEngineChoice::LegacyVm
    }
}

/// 无桥 ⇒ legacy 的一次性提示（stderr；不改变控制流、不静默）。
fn note_legacy_fallback_once() {
    static NOTICE: std::sync::Once = std::sync::Once::new();
    NOTICE.call_once(|| {
        eprintln!(
            "auto.config/engine: no ACE configuration executor is installed in this build; \
             the legacy configuration engine serves pac documents (named boundary, \
             AUTO_CONFIG_ENGINE=av refuses instead of falling back)"
        );
    });
}

/// 当前选择的引擎名（`av` / `legacy`），收据与日志用。
pub fn engine_name() -> &'static str {
    match choice() {
        ConfigEngineChoice::AceAv => "av",
        ConfigEngineChoice::LegacyVm => "legacy",
    }
}

/// 测试用进程内覆盖（`None` 恢复自动选择）。
pub fn set_choice_for_test(choice: Option<ConfigEngineChoice>) {
    let v = match choice {
        None => 0,
        Some(ConfigEngineChoice::AceAv) => 1,
        Some(ConfigEngineChoice::LegacyVm) => 2,
    };
    OVERRIDE.store(v, Ordering::Relaxed);
}

/// 一次配置求值的实际派发统计。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineObservation {
    /// 当前引擎名。
    pub engine: &'static str,
    /// ACE 派发次数（含失败）。
    pub ace_dispatches: u64,
    /// legacy 派发次数。
    pub legacy_dispatches: u64,
}

/// 读出观测计数（并给出当前引擎名）。
pub fn observation() -> EngineObservation {
    EngineObservation {
        engine: engine_name(),
        ace_dispatches: ACE_DISPATCHES.load(Ordering::Relaxed),
        legacy_dispatches: LEGACY_DISPATCHES.load(Ordering::Relaxed),
    }
}

/// 清零计数（测试用）。
pub fn reset_observation() {
    ACE_DISPATCHES.store(0, Ordering::Relaxed);
    LEGACY_DISPATCHES.store(0, Ordering::Relaxed);
}

fn note(choice: ConfigEngineChoice) {
    match choice {
        ConfigEngineChoice::AceAv => {
            ACE_DISPATCHES.fetch_add(1, Ordering::Relaxed);
        }
        ConfigEngineChoice::LegacyVm => {
            LEGACY_DISPATCHES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// 读一份 pac 文档（`pac.at`）——引擎按 [`choice`]，不静默回退。
pub fn parse_file(path: &Path, args: &Obj) -> AutoResult<AutoConfig> {
    let engine = choice();
    note(engine);
    AutoConfig::from_file_with_engine(path, args, engine)
}

/// 读一段配置源码——引擎按 [`choice`]，不静默回退。
pub fn parse_code(code: impl Into<String>, args: &Obj) -> AutoResult<AutoConfig> {
    let engine = choice();
    note(engine);
    AutoConfig::from_code_with_engine(code, args, engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三个用例共享进程内的覆盖/计数，必须串行（互不干扰）。
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn p982_engine_choice_is_observable_and_overridable() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 显式覆盖优先于环境与桥。
        set_choice_for_test(Some(ConfigEngineChoice::AceAv));
        assert_eq!(engine_name(), "av");
        set_choice_for_test(Some(ConfigEngineChoice::LegacyVm));
        assert_eq!(engine_name(), "legacy");
        set_choice_for_test(None);
        // 无桥宿主 ⇒ legacy（本测试进程没装 ACE 执行桥）。
        assert_eq!(engine_name(), "legacy");
    }

    #[test]
    fn p982_engine_observation_counts_real_dispatches() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_observation();
        set_choice_for_test(Some(ConfigEngineChoice::LegacyVm));
        let cfg = parse_code("name: \"observed\"\n", &Obj::new()).expect("legacy serves it");
        assert_eq!(cfg.root.get_prop("name").to_astr().as_str(), "observed");
        let obs = observation();
        assert_eq!(obs.legacy_dispatches, 1, "a legacy dispatch is counted");
        assert_eq!(obs.ace_dispatches, 0, "no ACE dispatch was attempted");
        set_choice_for_test(None);
    }

    #[test]
    fn p982_explicit_ace_without_a_bridge_is_a_named_failure() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_observation();
        set_choice_for_test(Some(ConfigEngineChoice::AceAv));
        let err = match parse_code("name: \"x\"\n", &Obj::new()) {
            Ok(_) => panic!("no executor installed: the ACE path must refuse by name"),
            Err(e) => e,
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("executor-unregistered"),
            "expected the named host refusal, got {msg}"
        );
        assert_eq!(observation().ace_dispatches, 1);
        assert_eq!(observation().legacy_dispatches, 0, "no silent fallback");
        set_choice_for_test(None);
    }
    #[test]
    fn p982_explicit_engine_env_is_honoured() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_choice_for_test(None);
        std::env::set_var("AUTO_CONFIG_ENGINE", "legacy");
        assert_eq!(engine_name(), "legacy");
        std::env::set_var("AUTO_CONFIG_ENGINE", "av");
        assert_eq!(
            engine_name(),
            "av",
            "an explicit ACE selection must not be downgraded to legacy"
        );
        std::env::set_var("AUTO_CONFIG_ENGINE", "not-an-engine");
        // 未知取值：回到环境事实（本测试进程无桥 ⇒ legacy），但绝不静默改成 av。
        assert_eq!(engine_name(), "legacy");
        std::env::remove_var("AUTO_CONFIG_ENGINE");
    }
}
