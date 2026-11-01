//! PLAN-014 T-03：诊断/机器输出层（自 main.rs 原样收编）。
//! 职责：AutoError→miette 包装、AI/JSON 诊断与成功信封、ai_mode 输出分岔。
//! JSON 形状/severity/span/help/related 语义零改（合同 §5.3）。
//! 注：命令臂内联的 `if ai_mode { eprintln!; exit(1) }` 错误出口仍随臂
//! 分布（Run 臂带专属 exit_audit 挂点）；本层为其唯一格式化来源，
//! 出口收编留后续有审计语义核时再做，不属本任务行为面。
use auto_lang_core::error::AutoError;
use miette::Diagnostic;
use serde_json::{json, Value};

// Helper to convert AutoError to miette Report - this preserves all diagnostic info
pub(crate) fn to_miette_err(err: AutoError) -> miette::Report {
    miette::Report::new(err)
}

/// Format error as JSON for machine consumption
pub(crate) fn format_error_json(err: &AutoError) -> String {
    let mut error_obj: Value = json!({
        "message": err.to_string(),
    });

    // Add error code if available
    if let Some(code) = err.code() {
        error_obj["code"] = json!(code.to_string());
    }

    // Add severity level
    let severity = if matches!(err, AutoError::Warning(_)) {
        "warning"
    } else {
        "error"
    };
    error_obj["severity"] = json!(severity);

    // Try to get source span information from labels
    if let Some(labels) = err.labels() {
        let spans: Vec<Value> = labels
            .map(|label| {
                let mut span_obj = json!({
                    "offset": label.offset(),
                    "len": label.len(),
                });
                // Add label text if present
                if let Some(text) = label.label() {
                    span_obj["label"] = json!(text);
                }
                span_obj
            })
            .collect();
        error_obj["spans"] = json!(spans);
    }

    // Add help text if available
    if let Some(help) = err.help() {
        error_obj["help"] = json!(help.to_string());
    }

    // Add related errors (for MultipleErrors)
    if let Some(related) = err.related() {
        let related_errors: Vec<Value> = related
            .map(|diag| {
                json!({
                    "message": diag.to_string(),
                    "code": diag.code().map(|c| c.to_string()),
                })
            })
            .collect();
        if !related_errors.is_empty() {
            error_obj["related"] = json!(related_errors);
        }
    }

    error_obj.to_string()
}

/// Format success result as JSON for AI consumption
pub(crate) fn format_success_json<T: serde::Serialize>(result: T) -> String {
    json!({
        "status": "success",
        "result": result
    })
    .to_string()
}

/// Output success result in appropriate format based on AI mode
pub(crate) fn output_success(ai_mode: bool, result: &str) {
    if ai_mode {
        println!("{}", format_success_json(result));
    } else {
        println!("{}", result);
    }
}
