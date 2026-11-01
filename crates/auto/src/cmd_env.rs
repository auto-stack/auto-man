//! PLAN-014 T-03：`auto env` 命令编排（自 main.rs Env 臂原样收编）。
//! 职责：reset/install 配置面与 AutoCache 子命令编排。
//! 行为合同：输出 JSON/文本、退出码零改。
use crate::cli_spec::{init_logger, println_logo, CacheCommands, EnvAction};
use crate::diag::{format_error_json, format_success_json};
use crate::load_am_config;
use auto_lang_core::error::AutoError;
use serde_json::json;

pub(crate) fn run(ai_mode: bool, action: EnvAction) -> miette::Result<()> {
if !ai_mode {
    init_logger();
    println_logo();
}
match action {
    EnvAction::Reset => {
        auto_man_runtime::Automan::reset_index().map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                std::process::exit(1);
            }
            miette::miette!("{}", e)
        })?;
        if ai_mode {
            println!("{}", format_success_json(json!({"message": "Index reset"})));
        }
    }
    EnvAction::Install { file } => {
        auto_man_runtime::Automan::install_config(&file).map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                std::process::exit(1);
            }
            miette::miette!("{}", e)
        })?;
        if ai_mode {
            println!(
                "{}",
                format_success_json(
                    json!({"message": "Config installed", "file": file})
                )
            );
        }
    }
    EnvAction::Cache { command } => {
        let config = load_am_config().unwrap_or_default();
        let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                std::process::exit(1);
            }
            miette::miette!("{}", e)
        })?;
        match command {
            CacheCommands::Stats => {
                am.cache_stats().map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            }
            CacheCommands::List { type_, limit } => {
                am.cache_list(type_, limit).map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            }
            CacheCommands::Prune => {
                am.cache_prune().map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
                if ai_mode {
                    println!(
                        "{}",
                        format_success_json(json!({"message": "Cache pruned"}))
                    );
                }
            }
            CacheCommands::Clear => {
                am.cache_clear().map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
                if ai_mode {
                    println!(
                        "{}",
                        format_success_json(json!({"message": "Cache cleared"}))
                    );
                }
            }
            CacheCommands::Inspect { name } => {
                am.cache_inspect(&name).map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            }
            CacheCommands::Verify => {
                am.cache_verify().map_err(|e| {
                    if ai_mode {
                        eprintln!(
                            "{}",
                            format_error_json(&AutoError::Msg(e.to_string()))
                        );
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
                if ai_mode {
                    println!(
                        "{}",
                        format_success_json(json!({"message": "Cache verified"}))
                    );
                }
            }
        }
    }
}
    Ok(())
}
