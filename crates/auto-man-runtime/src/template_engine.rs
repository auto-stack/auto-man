//! PLAN-982（982/TEMPLATE）：**模板求值的引擎选择**。
//!
//! 模板脚手架（`Templates::copy`）逐文件物化：`pac.at` 是配置文档、其余是文本
//! 模板。配置文档的求值现在有两个引擎：
//!
//! - 旧：`AutoInterpreter::eval_template`（默认，未迁移前行为不变）；
//! - 新：`config_engine`（ACE），显式 `AUTO_ACE_TEMPLATE=ace` 选中。
//!
//! 选择是**显式**的：未知取值保持旧路径；ACE 选中时求值失败**具名**且不落回旧
//! 解释器（模板产物要么来自 ACE、要么是错误，不存在"看起来成功但用了旧引擎"）。

/// 环境变量名。
pub const TEMPLATE_ENGINE_ENV: &str = "AUTO_ACE_TEMPLATE";

/// 是否用 ACE 求值配置模板。
pub fn ace_template_selected() -> bool {
    matches!(
        std::env::var(TEMPLATE_ENGINE_ENV).as_deref(),
        Ok("1") | Ok("true") | Ok("yes") | Ok("ace") | Ok("a2r")
    )
}

/// 引擎名（诊断/回执用）。
pub fn engine_name() -> &'static str {
    if ace_template_selected() {
        "ace"
    } else {
        "legacy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p982_template_engine_env_is_explicit() {
        std::env::remove_var(TEMPLATE_ENGINE_ENV);
        assert!(!ace_template_selected());
        assert_eq!(engine_name(), "legacy");
        std::env::set_var(TEMPLATE_ENGINE_ENV, "ace");
        assert!(ace_template_selected());
        assert_eq!(engine_name(), "ace");
        // 未知取值：保持旧路径（不静默切 ACE）。
        std::env::set_var(TEMPLATE_ENGINE_ENV, "nope");
        assert!(!ace_template_selected());
        std::env::remove_var(TEMPLATE_ENGINE_ENV);
    }
}
