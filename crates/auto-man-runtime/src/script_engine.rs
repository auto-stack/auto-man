//! PLAN-982（982/WEBTARGET）：**`ts` 目标的引擎选择与渲染器注册**。
//!
//! 与 [`crate::rust_engine`] 同一纪律，但作用于产品的 `ts`/`typescript` 目标：
//!
//! - `AUTO_ACE_TS=ace` 显式选中 A2X 的脚本引擎（默认走旧 `trans_typescript`）；
//! - 运行时 crate 不能依赖宿主桥（分层），渲染器由 `auto` 启动时**注册**进来；
//! - 选中但没装渲染器 ⇒ **具名拒绝** `ace.ts/renderer-unavailable`（不静默回退）；
//! - 多模块工程仍具名拒绝（跨模块 `use` 未迁移）。

/// 环境变量名。
pub const TS_ENGINE_ENV: &str = "AUTO_ACE_TS";

/// 渲染器：`(source, module, entry)` → `(文件名, 文本)` 列表，或 `(code, message)`。
pub type ScriptRenderer = fn(&str, &str, &str) -> Result<Vec<(String, String)>, (String, String)>;

static RENDERER: std::sync::Mutex<Option<ScriptRenderer>> = std::sync::Mutex::new(None);

/// 是否显式选中 A2X 脚本引擎。
pub fn ace_a2js_selected() -> bool {
    matches!(
        std::env::var(TS_ENGINE_ENV).as_deref(),
        Ok("1") | Ok("true") | Ok("yes") | Ok("ace") | Ok("a2x") | Ok("a2js")
    )
}

/// 引擎名（诊断/回执用）。
pub fn engine_name() -> &'static str {
    if ace_a2js_selected() {
        "ace"
    } else {
        "legacy"
    }
}

/// 安装渲染器（幂等；返回是否首次安装）。
pub fn install_renderer(renderer: ScriptRenderer) -> bool {
    let mut slot = RENDERER.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }
    *slot = Some(renderer);
    true
}

/// 是否已安装渲染器。
pub fn renderer_installed() -> bool {
    RENDERER.lock().map(|slot| slot.is_some()).unwrap_or(false)
}

/// 通过已安装的渲染器渲染一个单模块脚本工程。
pub fn render_files(
    source: &str,
    module: &str,
    entry: &str,
) -> Result<Vec<(String, String)>, (String, String)> {
    let renderer = *RENDERER
        .lock()
        .map_err(|_| {
            (
                "ace.ts/renderer-poisoned".to_string(),
                "the A2JS renderer registry is poisoned".to_string(),
            )
        })?
        .as_ref()
        .ok_or_else(|| {
            (
                "ace.ts/renderer-unavailable".to_string(),
                "no A2JS renderer is installed in this build".to_string(),
            )
        })?;
    renderer(source, module, entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p982_ts_engine_env_is_explicit() {
        std::env::remove_var(TS_ENGINE_ENV);
        assert!(!ace_a2js_selected());
        assert_eq!(engine_name(), "legacy");
        std::env::set_var(TS_ENGINE_ENV, "ace");
        assert!(ace_a2js_selected());
        assert_eq!(engine_name(), "ace");
        std::env::set_var(TS_ENGINE_ENV, "nope");
        assert!(!ace_a2js_selected());
        std::env::remove_var(TS_ENGINE_ENV);
    }
}
