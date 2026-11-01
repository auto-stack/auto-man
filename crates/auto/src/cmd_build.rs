//! PLAN-014 T-03：`auto build` 命令编排（自 main.rs Build 臂原样收编）。
//! 职责：strict 穿进程登记、端口解析注入、scan/build 编排。
//! 行为合同：flag/默认值/输出/退出码零改；Core 形态经共享源路径复用。
use crate::cli_spec::{init_logger, println_logo};
use crate::load_am_config;
use crate::tool_ui_ops;
use auto_lang_core::error::AutoError;
use crate::diag::{format_error_json, format_success_json};
use serde_json::json;

/// Build 臂参数（clap 变体字段一一对应；main.rs 以字段简写构造）。
pub(crate) struct Args {
    pub(crate) dir: Option<String>,
    pub(crate) port: Option<String>,
    pub(crate) back_port: Option<String>,
    pub(crate) front_port: Option<String>,
    pub(crate) render: Option<String>,
    pub(crate) scene: Option<String>,
    pub(crate) gen_only: bool,
    pub(crate) strict: bool,
    pub(crate) lenient: bool,
}

pub(crate) fn run(ai_mode: bool, a: Args) -> miette::Result<()> {
    let Args {
        dir,
        port,
        back_port,
        front_port,
        render,
        scene,
        gen_only,
        strict,
        lenient,
    } = a;
if !ai_mode {
    init_logger();
    println_logo();
}
// Plan 015 P0#1: strict validation is the DEFAULT for `auto build`
// (silent stale-SFC success classes). --lenient restores the old
// behavior; --strict stays accepted as a no-op for compatibility.
// PLAN-816 T-05 R1（F-05）：strict 经显式登记**穿进程边界**到
// 工具（build_ui/generate_ui 的 op 级字段），不以本进程全局
// 冒充跨进程传递；ui-extra 下的本地全局仅为同进程旧面保持。
let strict_on = strict || !lenient;
tool_ui_ops::set_generation_strict(Some(strict_on));
#[cfg(feature = "ui-extra")]
if strict_on {
    auto_ui_compiler::ui_gen::validators::set_strict(true);
}
let dir = dir.unwrap_or_else(|| ".".to_string());
let config = load_am_config().unwrap_or_default();
let mut am = auto_man_runtime::Automan::new(&dir, config).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
        std::process::exit(1);
    }
    miette::miette!("{}", e)
})?;
if let Some(p) = port {
    am.set_port(p.into()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
            std::process::exit(1);
        }
        miette::miette!("{}", e)
    })?;
}
if let Some(ref b) = render {
    am.set_render(b.clone());
}
if let Some(ref s) = scene {
    am.set_scene(s.clone());
}
am.set_gen_only(gen_only);
// Plan 330/401: bake ports into generated artifacts the same way
// `run` does, for symmetry (so `build` then manual run matches).
// CLI flags win; otherwise fall back to pac.at back_port/front_port.
let (pac_front, pac_back) = am.pac_dev_ports();
let effective_back = back_port
    .clone()
    .or_else(|| pac_back.map(|p| p.to_string()));
if let Some(p) = &effective_back {
    if p.trim().parse::<u16>().is_err() {
        return Err(miette::miette!(
            "Invalid backend port '{}': must be a number 0-65535",
            p
        ));
    }
    std::env::set_var("AUTO_HTTP_PORT", p.trim());
    println!("  Backend API server port: {}", p.trim());
}
let effective_front = front_port
    .clone()
    .or_else(|| pac_front.map(|p| p.to_string()));
if let Some(p) = &effective_front {
    if p.trim().parse::<u16>().is_err() {
        return Err(miette::miette!(
            "Invalid frontend port '{}': must be a number 0-65535",
            p
        ));
    }
    std::env::set_var("AUTO_FRONT_PORT", p.trim());
    println!("  Frontend dev server port: {}", p.trim());
}
am.scan().map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
        std::process::exit(1);
    }
    miette::miette!("{}", e)
})?;
am.build().map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
        std::process::exit(1);
    }
    miette::miette!("{}", e)
})?;
if ai_mode {
    println!(
        "{}",
        format_success_json(json!({"message": "Build completed"}))
    );
}
    Ok(())
}
