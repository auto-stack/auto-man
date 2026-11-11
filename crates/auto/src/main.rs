use auto_lang_core::error::AutoError;
use clap::Parser;
use log::info;
use miette::{MietteHandlerOpts, Result};
use serde_json::json;
use std::path::PathBuf;

use cli_spec::{init_logger, println_logo, Cli, Commands, DeviceAction, OutputFormat};
use diag::{format_error_json, format_success_json, output_success, to_miette_err};

mod cmd_a2c_stdlib;
// PLAN-816 T-05：UI 域子命令模块（ui-extra）——Core 形态缺席，分发臂
// 具名失败；cmd_autodesk 在 Core 形态以桌面协议缺席 stub 提供（孵化
// 标记永不认领 / -q 具名失败），Run 臂调用面保持编译。
#[cfg(feature = "ui-extra")]
mod cmd_autodesk;
#[cfg(not(feature = "ui-extra"))]
mod cmd_autodesk {
    //! Core 形态 stub：桌面协议宿主面不链接——run_if_client_entry 永不
    //! 认领（③ 独立形态现行行为），render-queue 具名失败。
    pub fn run_if_client_entry(_args: &[String]) -> Option<Result<(), String>> {
        None
    }
    pub fn rqhost_gate_validate(render: Option<&str>, _args: &[String]) -> Result<(), String> {
        Err(format!(
            "render-queue (-q) requires the full profile (ui-extra): desktop protocol host is not linked (render={render:?})"
        ))
    }
    #[allow(dead_code)]
    pub fn rqhost_injected_args(_wellknown: &str, args: &[String]) -> Vec<String> {
        args.to_vec()
    }
}
#[cfg(feature = "ui-extra")]
mod cmd_bp;
mod cmd_docs;
mod cmd_service;
mod cmd_service_export;
mod cmd_stdlib;
// PLAN-816 T-05：runtime ui_ops seam 的工具进程实现（I-TOOL 五操作臂）。
#[cfg(feature = "ui-extra")]
mod cmd_ui;
mod cmd_watch;
mod tool_ui_ops;

// PLAN-014 T-03：CLI 职责四分——解析声明（cli_spec）、诊断/机器输出
// （diag）、命令编排（cmd_*，按面收编 main.rs 巨臂）、分派（本文件
// real_main 保清晰分派）。行为合同 §5.3：help/flag/默认值/机器 JSON/
// stdio/退出码零改；复用既有 cmd 模块与 tool_ui_ops，不造第二工具
// 发现或 owned 进程设施。
mod cli_spec;
mod cmd_build;
mod cmd_env;
mod cmd_req;
mod cmd_run;
mod cmd_test;
mod cmd_trans;
mod cmd_wizard;
mod diag;

fn load_am_config() -> Option<auto_man_runtime::AmConfig> {
    auto_man_runtime::load_am_config()
}

fn main() -> Result<()> {
    // PLAN-816 T-03: 安装管理服务的 UI seam（runtime 编排的 UI 臂经 compat
    // 门面回调 auto-ui-tools/残留;未安装时 UI 臂具名失败,非 UI 路径不受影响）。
    // PLAN-816 T-05：UI 臂改经真实工具进程（I-TOOL 五操作；无静态 UI
    // 大库 fallback）。惰性发现——非 UI 命令不触达工具。
    tool_ui_ops::install();

    let cli = Cli::parse();

    // The binary is linked with a 4MB stack (see build.rs) so the main thread
    // has enough headroom for both UI (iced needs the main thread) and the
    // parser's deep recursion on complex UI files.
    let result = real_main(cli);
    // PLAN-080 §10-9: main 层 Err 退出审计——Err 路径（会打印 miette 报告）
    // 与外部终止在 exit-audit.log 上一刀切开（Err 有 main_err 行，外部终止
    // 无任何行）。
    if let Err(ref e) = result {
        auto_lang_core::vm::ffi::stdlib::exit_audit(1, "main_err");
        eprintln!("[X9] main Err: {e:?}");
    }
    result
}

/// ui-extra 形态的 UI 宿主装配：SDK 钩子注册 + provider 实装
///（auto-ui-runtime 单点）。UI 文法由调用方经 `parse_domain` 显式组合，
/// 不再进程级注入；Core 形态无本调用。
#[cfg(feature = "ui-extra")]
fn install_ui_host_capabilities() {
    auto_lang_core::set_ui_host_install_hook(|| {
        auto_ui_runtime::install_ui_host_capabilities();
    });
    auto_ui_runtime::install_ui_host_capabilities();
}

fn real_main(cli: Cli) -> Result<()> {
    auto_web::install_http_service_host_if_absent();
    // PLAN-816 T-05：UI 宿主装配先行（ui-extra 形态；见函数注记）。
    #[cfg(feature = "ui-extra")]
    install_ui_host_capabilities();
    // PLAN-779: 宿主能力装配——UI host 启动即注册 frame provider 进程默认
    //（frame_bench adapter；幂等）。auto bin 默认带 ui-extra；无 UI 构建下
    // 本调用随 feature 一起缺席（core 缺席语义=未捕获 0，不损失行为——
    // 无渲染泵进程的 frame 读数本就恒 0）。
    #[cfg(feature = "ui-extra")]
    auto_ui_runtime::ui::frame_provider::install_process_default();
    // PLAN-785: clipboard text adapter 同点装配（if-absent 兼容默认；auto
    // 的 ui-iced 传递开启 auto-lang/ui-iced ⇒ ui-clipboard，门控随形）。
    #[cfg(feature = "ui-extra")]
    auto_ui_runtime::ui::clipboard_provider::install_process_default();
    // PLAN-790: clipboard media adapter 同点装配（if-absent 兼容默认；
    // ui-iced 传递开启 native-clipboard，windows × native-clipboard 双门控
    // 与 adapter 模块一致——非 Windows 不装配）。
    #[cfg(all(windows, feature = "ui-extra"))]
    auto_ui_runtime::ui::clipboard_media_provider::install_process_default();

    // Determine AI mode: -ai flag or --format json
    let ai_mode = cli.ai || matches!(cli.format, Some(OutputFormat::Json));

    miette::set_hook(Box::new(move |_| {
        Box::new(MietteHandlerOpts::new().terminal_links(true).build())
    }))
    .ok();

    // Set error limit from CLI if provided
    if let Some(limit) = cli.error_limit {
        auto_lang_core::set_error_limit(limit);
    }

    // Enable VM debug logging if requested
    if cli.debug {
        auto_lang_core::set_vm_debug(true);
    }

    // Plan 555 T08 / Plan 524：裸脚本执行与 dump-lowered 审查面
    //（PLAN-014 T-03 收编 cmd_run；true = 已处理，real_main 直接返回）。
    if cmd_run::run_script_file(
        ai_mode,
        cmd_run::ScriptArgs {
            file: cli.file.clone(),
            dump_lowered: cli.dump_lowered,
            script_args: cli.script_args.clone(),
        },
    )? {
        return Ok(());
    }

    match cli.command {
        // ========== Project Creation ==========
        Some(Commands::New { name, template }) => {
            if !ai_mode {
                init_logger();
                println_logo();
                info!("Creating new project: {}", name);
            }
            if let Some(t) = template {
                auto_man_runtime::Automan::create_by_template(&name, &t).map_err(|e| {
                    if ai_mode {
                        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            } else {
                auto_man_runtime::Automan::create_app(&name).map_err(|e| {
                    if ai_mode {
                        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            }
            if ai_mode {
                println!(
                    "{}",
                    format_success_json(json!({"message": "Project created", "path": name}))
                );
            }
        }
        Some(Commands::Init { template }) => {
            if !ai_mode {
                init_logger();
                println_logo();
                info!("Initializing Auto project in current directory");
            }
            if let Some(t) = template {
                auto_man_runtime::Automan::create_by_template(".", &t).map_err(|e| {
                    if ai_mode {
                        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            } else {
                auto_man_runtime::Automan::create_app(".").map_err(|e| {
                    if ai_mode {
                        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                        std::process::exit(1);
                    }
                    miette::miette!("{}", e)
                })?;
            }
            if ai_mode {
                println!(
                    "{}",
                    format_success_json(json!({"message": "Project initialized"}))
                );
            }
        }

        // ========== Build & Run ==========
        Some(Commands::Build {
            dir,
            port,
            back_port,
            front_port,
            render,
            scene,
            gen_only,
            strict,
            lenient,
        }) => {
            cmd_build::run(
                ai_mode,
                cmd_build::Args {
                    dir,
                    port,
                    back_port,
                    front_port,
                    render,
                    scene,
                    gen_only,
                    strict,
                    lenient,
                },
            )?;
        }
        // PLAN-031：rqhost daemon——阻塞直至末窗退出（锁管道被占 =
        // 第二实例干净退出码 0）。
        Some(Commands::Rqhost {
            pipe,
            bridge_ws,
            bridge_token,
        }) => {
            // PLAN-816 T-05：rqhost=桌面协议宿主（UI runtime 域）——Core
            // 形态具名失败（不静默 no-op）。
            #[cfg(not(feature = "ui-extra"))]
            {
                let _ = (&pipe, &bridge_ws, &bridge_token);
                return Err(miette::miette!(
                    "rqhost requires the full profile (ui-extra): desktop protocol host is not linked in core builds"
                ));
            }
            #[cfg(feature = "ui-extra")]
            {
                let wellknown = pipe.unwrap_or_else(|| {
                    auto_ui_runtime::ui::desktop_protocol::rqhost::wellknown_pipe()
                });
                if !ai_mode {
                    init_logger();
                    println_logo();
                }
                // PLAN-102 D-1：bridge 宿主分岔（去窗 + WS 桥）。
                if let Some(port) = bridge_ws {
                    let token = bridge_token.as_deref().unwrap_or_default().to_string();
                    auto_ui_runtime::ui::desktop_protocol::rqhost::run_bridge(
                        &wellknown, port, &token,
                    )
                    .map_err(|e| {
                        if ai_mode {
                            eprintln!("{}", format_error_json(&AutoError::Msg(e)));
                            std::process::exit(1);
                        }
                        miette::miette!("{e}")
                    })?;
                    return Ok(());
                }
                auto_ui_runtime::ui::desktop_protocol::rqhost::run_daemon(&wellknown).map_err(
                    |e| {
                        if ai_mode {
                            eprintln!("{}", format_error_json(&AutoError::Msg(e)));
                            std::process::exit(1);
                        }
                        miette::miette!("{e}")
                    },
                )?;
                if !ai_mode {
                    println!("------------- end --------------");
                }
                return Ok(());
            }
        }
        Some(Commands::Service {
            dir,
            server,
            http_config,
            http_config_inline,
            back_port,
        }) => {
            // PLAN-781 T-15: 宿主可信预算配置 fail-fast——非法值指名报错 +
            // 非零退出，先于任何业务 dispatch（决策报告 D-1；serve 面）。
            if let Err(e) = auto_lang_core::vm::budget_policy::parse_http_step_budget(
                std::env::var(auto_lang_core::vm::budget_policy::HTTP_STEP_BUDGET_ENV)
                    .ok()
                    .as_deref(),
            ) {
                return Err(miette::miette!("{e}"));
            }
            // PLAN-736: server-only 启动；错误经 main Err 面 = 非零退出。
            return cmd_service::cmd_service(cmd_service::ServeArgs {
                dir,
                server,
                http_config,
                http_config_inline,
                back_port,
            });
        }
        // PLAN-1006 T-04: Web 生成成员独立交付公开入口（export-only，不监听）。
        Some(Commands::ServiceExport {
            dir,
            delivery_root,
            resource,
        }) => {
            return cmd_service_export::cmd_service_export(cmd_service_export::ExportArgs {
                dir,
                delivery_root,
                resource,
            });
        }
        Some(Commands::Run {
            dir,
            port,
            back_port,
            front_port,
            render,
            render_queue,
            desktop_endpoint,
            server,
            no_merge,
            scene,
            theme,
            accent,
            desktop,
            gallery,
            apps,
            merged,
            args,
        }) => {
            cmd_run::run_project(
                ai_mode,
                cmd_run::ProjectArgs {
                    dir,
                    port,
                    back_port,
                    front_port,
                    render,
                    render_queue,
                    desktop_endpoint,
                    server,
                    no_merge,
                    scene,
                    theme,
                    accent,
                    desktop,
                    gallery,
                    apps,
                    merged,
                    args,
                },
            )?;
        }
        Some(Commands::Test {
            dir,
            filter,
            verbose,
        }) => {
            cmd_test::test(dir, filter, verbose)?;
        }
        Some(Commands::TestUi {
            dir,
            headed,
            filter,
            report,
        }) => {
            cmd_test::test_ui(
                ai_mode,
                cmd_test::TestUiArgs {
                    dir,
                    headed,
                    filter,
                    report,
                },
            )?;
        }
        Some(Commands::Clean { dir }) => {
            if !ai_mode {
                init_logger();
                println_logo();
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
            am.clean().map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            if ai_mode {
                println!(
                    "{}",
                    format_success_json(json!({"message": "Clean completed"}))
                );
            }
        }
        Some(Commands::Watch { dir, .. }) => {
            init_logger();
            println_logo();
            let dir = dir.unwrap_or_else(|| ".".to_string());
            let project_dir = PathBuf::from(&dir)
                .canonicalize()
                .map_err(|e| miette::miette!("Invalid project directory: {}", e))?;
            cmd_watch::run_watch(&project_dir).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
        }
        Some(Commands::Wizard { action }) => {
            cmd_wizard::run(action)?;
        }

        // ========== Dependencies ==========
        Some(Commands::Add { package }) => {
            if !ai_mode {
                init_logger();
                println_logo();
                info!("Adding dependency: {}", package);
            }
            // TODO: Implement Automan::add_dependency
            if ai_mode {
                eprintln!(
                    "{}",
                    format_error_json(&AutoError::Msg(
                        "'add' command is not yet implemented".to_string()
                    ))
                );
                std::process::exit(1);
            }
            miette::bail!("'add' command is not yet implemented in the library");
        }
        Some(Commands::Fetch) => {
            if !ai_mode {
                init_logger();
                println_logo();
                info!("Fetching dependencies...");
            }
            let config = load_am_config().unwrap_or_default();
            let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            am.pull().map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            am.scan().map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            if ai_mode {
                println!(
                    "{}",
                    format_success_json(json!({"message": "Dependencies fetched"}))
                );
            }
        }
        Some(Commands::Deps) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            auto_man_runtime::Automan::list_deps(&config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
        }

        // ========== Hardware & Embedded ==========
        Some(Commands::Device { action }) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            match action {
                DeviceAction::List => {
                    auto_man_runtime::Automan::list_devices(&config).map_err(|e| {
                        if ai_mode {
                            eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                            std::process::exit(1);
                        }
                        miette::miette!("{}", e)
                    })?;
                }
                DeviceAction::Select { port } => {
                    let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                        if ai_mode {
                            eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                            std::process::exit(1);
                        }
                        miette::miette!("{}", e)
                    })?;
                    am.set_port(port.clone().into()).map_err(|e| {
                        if ai_mode {
                            eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                            std::process::exit(1);
                        }
                        miette::miette!("{}", e)
                    })?;
                    if !ai_mode {
                        info!("Port updated successfully");
                    }
                    if ai_mode {
                        println!(
                            "{}",
                            format_success_json(json!({"message": "Port selected", "port": port}))
                        );
                    }
                }
            }
        }
        Some(Commands::Export { port, fmt }) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            am.export(port.clone(), fmt.clone()).map_err(|e| {
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
                        json!({"message": "Export completed", "port": port, "format": fmt})
                    )
                );
            }
        }

        // ========== Project Utils ==========
        Some(Commands::Info { target }) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            let am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            am.info(target).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
        }
        Some(Commands::Open) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            am.open_ide().map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            if ai_mode {
                println!("{}", format_success_json(json!({"message": "IDE opened"})));
            }
        }

        // ========== Environment ==========
        // NOTE: Upgrade disabled — zip dependency removed
        // Some(Commands::Upgrade) => {
        //     if !ai_mode {
        //         init_logger();
        //         println_logo();
        //     }
        //     auto_man_runtime::upgrade().map_err(|e| {
        //         if ai_mode {
        //             eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
        //             std::process::exit(1);
        //         }
        //         miette::miette!("{}", e)
        //     })?;
        //     if ai_mode {
        //         println!("{}", format_success_json(json!({"message": "Upgrade completed"})));
        //     }
        // }

        // ========== Environment ==========
        Some(Commands::Env { action }) => {
            cmd_env::run(ai_mode, action)?;
        }

        // ========== Code Generation ==========
        Some(Commands::Gen {
            output,
            project,
            scene,
            delivery_root,
        }) => {
            if !ai_mode {
                init_logger();
                println_logo();
            }
            let config = load_am_config().unwrap_or_default();
            let mut am = auto_man_runtime::Automan::new(".", config).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            if let Some(ref s) = scene {
                am.set_scene(s.clone());
            }
            // PLAN-792：显式交付模式根（仅 rust 后端生效；其余后端设置即报错，
            // 不静默忽略）。
            if let Some(ref d) = delivery_root {
                if am.pac_backend_is_rust_only() {
                    am.set_delivery_root(d.clone());
                } else {
                    return Err(miette::miette!(
                        "--delivery-root 仅支持 rust 后端（pac.at render/backend 需为 rust）"
                    ));
                }
            }
            am.gen(output, project).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
                    std::process::exit(1);
                }
                miette::miette!("{}", e)
            })?;
            if ai_mode {
                println!(
                    "{}",
                    format_success_json(json!({"message": "Code generated"}))
                );
            }
        }

        // ========== AutoUI Widget Library (Plan 331) ==========
        Some(Commands::Ui { action }) => {
            #[cfg(feature = "ui-extra")]
            {
                cmd_ui::run(action)?;
            }
            #[cfg(not(feature = "ui-extra"))]
            {
                let _ = action;
                return Err(miette::miette!(
                    "`auto ui` requires the full profile (ui-extra): widget registry tooling is not linked in core builds"
                ));
            }
        }

        // ========== AutoUI Blueprints (Plan 343, Design 17; PLAN-639 rename) ==========
        Some(Commands::Bp { action }) => {
            #[cfg(feature = "ui-extra")]
            {
                cmd_bp::run(action, cli.format)?;
            }
            #[cfg(not(feature = "ui-extra"))]
            {
                let _ = (action, &cli.format);
                return Err(miette::miette!(
                    "`auto bp` requires the full profile (ui-extra): blueprint registry is not linked in core builds"
                ));
            }
        }
        Some(Commands::Block { action }) => {
            eprintln!("warning: `auto block` is deprecated (renamed by PLAN-639); use `auto bp`");
            #[cfg(feature = "ui-extra")]
            {
                cmd_bp::run(action, cli.format)?;
            }
            #[cfg(not(feature = "ui-extra"))]
            {
                let _ = (action, &cli.format);
                return Err(miette::miette!(
                    "`auto block` requires the full profile (ui-extra): blueprint registry is not linked in core builds"
                ));
            }
        }

        // ========== Schema Docs (Plan 435 P8-1) ==========
        Some(Commands::Docs { action }) => {
            cmd_docs::run(action)?;
        }

        // ========== Stdlib Assembly (PLAN-738) ==========
        Some(Commands::Stdlib { action }) => {
            cmd_stdlib::run(action, cli.format)?;
        }

        // ========== Legacy / Dev Tools ==========
        Some(Commands::Parse { code }) => {
            if !ai_mode {
                println!("Parsing Auto {} to JSON", code);
            }
            let json = auto_lang_core::run(&code).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&e));
                    std::process::exit(1);
                }
                to_miette_err(e)
            })?;
            output_success(ai_mode, &json);
        }
        Some(Commands::Eval { code }) => {
            let result = auto_lang_core::run(&code).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&e));
                    std::process::exit(1);
                }
                to_miette_err(e)
            })?;
            output_success(ai_mode, &result);
        }
        Some(Commands::OldRepl) => {
            auto_lang_core::autovm_repl::main_loop().map_err(|e| miette::miette!("{}", e))?;
        }
        Some(Commands::Config { path }) => {
            let code = std::fs::read_to_string(path.as_str()).map_err(|e| {
                if ai_mode {
                    eprintln!(
                        "{}",
                        format_error_json(&AutoError::Io(format!("Failed to read file: {}", e)))
                    );
                    std::process::exit(1);
                }
                miette::miette!("Failed to read file: {}", e)
            })?;
            let args = auto_val::Obj::new();
            let c = auto_lang_core::eval_config_with_vm(code.as_str(), &args).map_err(|e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&e));
                    std::process::exit(1);
                }
                to_miette_err(e)
            })?;
            output_success(ai_mode, &c.repr());
        }
        // ========== Transpile (single file) ==========
        Some(Commands::C { path, target }) => {
            cmd_trans::c(ai_mode, path, target)?;
        }
        Some(Commands::Rust { path }) => {
            cmd_trans::rust(ai_mode, path)?;
        }
        Some(Commands::Python { path }) => {
            cmd_trans::python(ai_mode, path)?;
        }
        Some(Commands::JavaScript { path }) => {
            cmd_trans::javascript(ai_mode, path)?;
        }
        Some(Commands::GDScript { path }) => {
            cmd_trans::gdscript(ai_mode, path)?;
        }
        Some(Commands::Trans { path, target }) => {
            cmd_trans::trans(ai_mode, path, target)?;
        }
        Some(Commands::R2a { path, output }) => {
            cmd_trans::r2a(ai_mode, path, output)?;
        }
        Some(Commands::A2cStdlib) => {
            cmd_a2c_stdlib::run()?;
        }

        // Plan 216 Phase 4: C FFI Bindgen
        Some(Commands::Cffi { header }) => {
            let headers = ["string.h", "math.h", "stdio.h", "stdlib.h", "time.h"];
            if let Some(h) = header {
                // Show functions for a specific header
                match auto_lang_core::vm::ffi::c_ffi::load_builtin_manifest(&h) {
                    Some(manifest) => {
                        println!("=== C FFI: {} ===", manifest.header);
                        println!("Library: {}", manifest.library);
                        println!();
                        for func in &manifest.functions {
                            let variadic = if func.variadic { " (variadic)" } else { "" };
                            println!(
                                "  {}{} — {:?}({})",
                                func.name,
                                variadic,
                                func.return_type,
                                func.params
                                    .iter()
                                    .map(|p| format!("{:?} {}", p.ty, p.name))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                        }
                    }
                    None => {
                        eprintln!("No manifest found for: {}", h);
                        eprintln!("Available headers: {}", headers.join(", "));
                    }
                }
            } else {
                // List all available headers
                println!("=== C FFI Bindings (Plan 216) ===");
                println!();
                for h in &headers {
                    match auto_lang_core::vm::ffi::c_ffi::load_builtin_manifest(h) {
                        Some(manifest) => {
                            println!("  {} — {} functions", h, manifest.functions.len());
                        }
                        None => {}
                    }
                }
                println!();
                println!("Use --header <name> to see functions for a specific header.");
            }
        }

        // ========== MCP Server (Plan 265) ==========
        Some(Commands::Mcp) => {
            let mut server = auto_lang_core::mcp::McpServer::new();
            server.run();
        }

        // ========== AutoVM Daemon (Plan 269) ==========
        Some(Commands::Serve {
            foreground,
            stdio,
            pipe_name,
            max_sessions,
            timeout,
        }) => {
            cmd_req::serve(cmd_req::ServeArgs {
                foreground,
                stdio,
                pipe_name,
                max_sessions,
                timeout,
            })?;
        }

        // ========== AutoVM Client (Plan 269) ==========
        Some(Commands::Req {
            session,
            pipe_name,
            new_session,
            inspect,
            reset,
            delete,
            snapshot,
            list,
            json,
            code,
        }) => {
            cmd_req::req(cmd_req::ReqArgs {
                session,
                pipe_name,
                new_session,
                inspect,
                reset,
                delete,
                snapshot,
                list,
                json,
                code,
            })?;
        }

        // ========== Debug (Plan 199) ==========
        Some(Commands::Debug { file, agent }) => {
            if agent {
                auto_lang_core::debug_file_agent(&file).map_err(to_miette_err)?;
            } else {
                println!("----------------------");
                println!("Debugging Auto {}", file);
                println!("----------------------");
                auto_lang_core::debug_file(&file).map_err(to_miette_err)?;
            }
        }

        None => {
            // Default: Use BigVM REPL
            auto_lang_core::autovm_repl::main_loop().map_err(|e| miette::miette!("{}", e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
/// PLAN-736: `auto service` CLI 参数面（解析级；不启动任何服务）。
#[cfg(test)]
mod cli_service_tests {
    use super::Cli;
    use clap::Parser;
    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("auto").chain(args.iter().copied()))
            .expect("service args parse")
    }
    #[test]
    fn service_command_shape() {
        let cli = parse(&["service", "--server", "vm", "--http-config-inline", "{}"]);
        match cli.command {
            Some(super::Commands::Service {
                server,
                http_config_inline,
                ..
            }) => {
                assert_eq!(server, "vm");
                assert_eq!(http_config_inline.as_deref(), Some("{}"));
            }
            _ => panic!("expected Service"),
        }
    }

    #[test]
    fn service_export_command_shape() {
        let cli = parse(&[
            "service-export",
            "--dir",
            "proj",
            "--delivery-root",
            "bundle",
            "--resource",
            "notes.txt",
            "--resource",
            "seed.json",
        ]);
        match cli.command {
            Some(super::Commands::ServiceExport {
                dir,
                delivery_root,
                resource,
            }) => {
                assert_eq!(dir.as_deref(), Some("proj"));
                assert_eq!(delivery_root, "bundle");
                assert_eq!(resource, vec!["notes.txt", "seed.json"]);
            }
            _ => panic!("expected ServiceExport"),
        }
    }

    #[test]
    fn service_export_delivery_root_is_required() {
        assert!(Cli::try_parse_from(["auto", "service-export"]).is_err());
    }

    #[test]
    fn service_config_and_inline_are_separate_fields() {
        let cli = parse(&[
            "service",
            "--server",
            "rust",
            "--http-config",
            "svc.json",
            "-B",
            "9100",
        ]);
        match cli.command {
            Some(super::Commands::Service {
                http_config,
                back_port,
                ..
            }) => {
                assert_eq!(http_config.as_deref(), Some("svc.json"));
                assert_eq!(back_port.as_deref(), Some("9100"));
            }
            _ => panic!("expected Service"),
        }
    }
}

#[cfg(test)]
mod cli_passthrough_tests {
    use super::Cli;
    use clap::Parser;

    fn parse(argv: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("auto").chain(argv.iter().copied()))
    }

    /// Plan 524/W0 形态 B 实测矩阵的入库面：透传成立 + 子命令/全局旗标零破坏。
    #[test]
    fn passthrough_positional_args_land_in_script_args() {
        let c = parse(&["packages/aavm/aavm.at", "crates/x/b07_fib.at"]).expect("直达形态");
        assert_eq!(c.file.as_deref(), Some("packages/aavm/aavm.at"));
        assert_eq!(c.script_args, ["crates/x/b07_fib.at"]);
    }

    #[test]
    fn passthrough_multiple_and_hyphen_values() {
        let c = parse(&["probe.at", "alpha", "beta"]).expect("多参");
        assert_eq!(c.script_args, ["alpha", "beta"]);
        let c = parse(&["probe.at", "--flag-like"]).expect("hyphen 值");
        assert_eq!(c.script_args, ["--flag-like"]);
    }

    #[test]
    fn subcommands_unaffected() {
        let c = parse(&["new", "myapp"]).expect("子命令仍工作");
        assert!(c.file.is_none());
        assert!(c.script_args.is_empty());
        assert!(c.command.is_some());
        let c = parse(&["--error-limit", "5", "new", "myapp"]).expect("全局旗标+子命令");
        assert!(c.command.is_some());
        assert_eq!(c.error_limit, Some(5));
    }

    #[test]
    fn bare_file_without_args_and_empty_invocation() {
        let c = parse(&["probe.at"]).expect("仅 file");
        assert_eq!(c.file.as_deref(), Some("probe.at"));
        assert!(c.script_args.is_empty());
        let c = parse(&[]).expect("空参");
        assert!(c.file.is_none());
        assert!(c.command.is_none());
    }

    /// 裸值撞子命令名 → `--` 消歧（W0 实测 D1）。
    #[test]
    fn double_dash_forces_trailing_passthrough() {
        let c = parse(&["probe.at", "--", "run"]).expect("-- 消歧");
        assert_eq!(c.script_args, ["run"]);
        assert!(c.command.is_none(), "-- 后的裸值不得劫持为子命令");
    }

    /// 已知全局旗标后置于 file 时被旗标吞（文档化行为，W0 实测 B9）。
    #[test]
    fn known_global_flag_after_file_is_consumed_by_flag() {
        let c = parse(&["probe.at", "--error-limit", "5"]).expect("全局旗标后置");
        assert_eq!(c.error_limit, Some(5));
        assert!(c.script_args.is_empty());
    }
}
