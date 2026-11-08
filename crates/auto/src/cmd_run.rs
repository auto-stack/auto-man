//! PLAN-014 T-03：`auto run`/裸脚本命令编排（自 main.rs 原样收编）。
//! 职责：裸脚本 VM 执行与 dump-lowered 审查面（run_script_file）、
//! run 面编排（run_project：双模入口/端口/env 注入/-q 分岔/am.run）。
//! 行为合同：exit_audit（main_err_ai_mode）、Core/Full 形态分岔、
//! env 通道与输出逐行零改；owned/daemon 设施仍走既有 tool_ui_ops/
//! auto_tool_execution，不造第二设施。
use crate::cli_spec::{init_logger, println_logo};
use crate::cmd_autodesk;
use crate::diag::{format_error_json, output_success, to_miette_err};
use crate::load_am_config;
use auto_lang_core::error::AutoError;
use colored::Colorize;
use log::info;

/// 裸脚本入口参数（Cli 的 file/dump_lowered/script_args 三字段）。
pub(crate) struct ScriptArgs {
    pub(crate) file: Option<String>,
    pub(crate) dump_lowered: bool,
    pub(crate) script_args: Vec<String>,
}

/// 执行裸脚本（含 dump-lowered 审查面与 ui-extra 动态 UI 分岔）。
/// 返回 true = 已处理（real_main 直接 Ok 返回）；false = 无脚本，继续分派。
pub(crate) fn run_script_file(ai_mode: bool, s: ScriptArgs) -> miette::Result<bool> {
    let ScriptArgs {
        file,
        dump_lowered,
        script_args,
    } = s;
    if let Some(path) = &file.clone() {
        if dump_lowered {
let lowered = auto_lang_core::dump_lowered(path).map_err(to_miette_err)?;
println!("{}", lowered);
return Ok(true);
        }
    }

    // Execution: Run an Auto script directly via AutoVM
    if let Some(path) = file {
        if !ai_mode {
println!("----------------------");
println!("Running Auto {} ", path);
println!("----------------------");
        }
        // Plan 524: CLI 透传（`auto <file> [args...]`）→ process.args() 表。
        // PLAN-816 T-05：动态 UI 分岔（Plan 227）属宿主装配面——ui-extra
        // 形态经 auto-ui-runtime 直跑（旧行为冻结）；Core 形态恒走纯语言
        // 执行（等价旧无 ui-iced 构建）。
        #[cfg(feature = "ui-extra")]
        if let Ok(code) = std::fs::read_to_string(&path) {
if auto_ui_runtime::has_ui_keywords(&code) {
    auto_lang_core::set_script_args(script_args.clone());
    let result =
        auto_ui_runtime::run_file_dynamic_ui(&code, Some(&path)).map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&e));
                std::process::exit(1);
            }
            to_miette_err(e)
        })?;
    output_success(ai_mode, &result);
    if !ai_mode {
        println!();
    }
    return Ok(true);
}
        }
        let result = auto_lang_core::run_file_with_args(&path, script_args).map_err(|e| {
if ai_mode {
    eprintln!("{}", format_error_json(&e));
    std::process::exit(1);
}
to_miette_err(e)
        })?;
        output_success(ai_mode, &result);
        if !ai_mode {
println!();
        }
        return Ok(true);
    }
    Ok(false)
}

/// Run 臂参数（clap 变体字段一一对应）。
pub(crate) struct ProjectArgs {
    pub(crate) dir: Option<String>,
    pub(crate) port: Option<String>,
    pub(crate) back_port: Option<String>,
    pub(crate) front_port: Option<String>,
    pub(crate) render: Option<String>,
    pub(crate) render_queue: bool,
    pub(crate) desktop_endpoint: Option<String>,
    pub(crate) server: Option<String>,
    pub(crate) no_merge: bool,
    pub(crate) scene: Option<String>,
    pub(crate) theme: Option<String>,
    pub(crate) accent: Option<String>,
    pub(crate) desktop: bool,
    pub(crate) gallery: bool,
    pub(crate) apps: Option<String>,
    pub(crate) merged: bool,
    pub(crate) args: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_project(ai_mode: bool, a: ProjectArgs) -> miette::Result<()> {
    let ProjectArgs {
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
        mut args,
    } = a;
// PLAN-781 T-15: 宿主可信预算配置 fail-fast——非法值指名报错 +
// 非零退出，先于任何业务 dispatch/窗口/服务启动（决策报告 D-1；
// run 面覆盖 vm/vue/desktop 全形态）。
if let Err(e) = auto_lang_core::vm::budget_policy::parse_http_step_budget(
    std::env::var(auto_lang_core::vm::budget_policy::HTTP_STEP_BUDGET_ENV)
        .ok()
        .as_deref(),
) {
    return Err(miette::miette!("{e}"));
}
if !ai_mode {
    init_logger();
    println_logo();
}
// Plan 480 S2: 双模入口裁决——孵化标记在册 → 协议 client 循环
// （走完即返回）；无标记 = ③ 独立形态，现行行为零改动。
if let Some(client_result) = cmd_autodesk::run_if_client_entry(&args) {
    client_result.map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&AutoError::Msg(e)));
            std::process::exit(1);
        }
        miette::miette!("{e}")
    })?;
    if !ai_mode {
        println!("------------- end --------------");
    }
    return Ok(());
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
    // PLAN-063 T-02：desktop 是底座声明不是前端目标（BackendType 无
    // 底座变体）——前端归一 vm（与 PLAN-690 `remote`→Vm 别名同律）；
    // 底座选择经下方 -q/底座 gate 的 env 通道（AUTO_VM_RENDER）。
    let frontend = if b.eq_ignore_ascii_case("desktop") {
        "vm"
    } else {
        b.as_str()
    };
    am.set_render(frontend.to_string());
}
if let Some(ref s) = scene {
    am.set_scene(s.clone());
}
let pac_api = am.pac_api();
let is_rust_api = pac_api.as_deref() == Some("rust");
let server = server.or_else(|| {
    if is_rust_api {
        Some("rust".to_string())
    } else {
        None
    }
});

// Plan 317: --server=vm uses AutoVM HTTP server (with module
// flattening for use db), --server=rust (default) uses a2r.
let vm_server_mode = server.as_deref() == Some("vm");
if vm_server_mode {
    am.set_vm_server_mode(true);
}
// Plan 345: --server=rust with --render=vm selects VM+Rust mode
// (frontend VM + backend Rust axum server, HTTP between them).
// AUTO_BACKEND_IMPL tells run_vm_ui which backend to start in split mode.
if let Some(s) = &server {
    if s == "rust" || s == "vm" {
        std::env::set_var("AUTO_BACKEND_IMPL", s);
    }
}
// Plan 340: --merge/--no-merge controls VM+VM in-process merging.
// When backend is Rust (`api: "rust"` or `--server=rust`), merging is impossible:
// Rust backend runs in a separate process, so VM+Rust is always split mode.
let merge_mode = !no_merge && server.as_deref() != Some("rust") && !is_rust_api;
std::env::set_var("AUTO_VM_MERGE", if merge_mode { "1" } else { "0" });
if merged {
    // The Rust UI runner already uses AUTO_VM_MERGE=1 by default;
    // retaining this explicit switch makes the documented command
    // (`auto run --render rust --server rust --merged`) parse as a
    // mode flag instead of leaking `--merged` to Cargo.
    std::env::set_var("AUTO_VM_MERGE", "1");
}
if !merge_mode {
    let backend = server
        .as_deref()
        .unwrap_or(if is_rust_api { "rust" } else { "vm" });
    let frontend = render.as_deref().unwrap_or("vm");
    println!(
        "  {} split mode: frontend {} ↔ backend {} over HTTP",
        "→".bright_cyan(),
        frontend,
        backend
    );
}
// Plan 330/401: resolve dev-server ports. CLI flags (-B/-F) win;
// otherwise fall back to pac.at `back_port`/`front_port`; otherwise
// leave unset (global default 8080/3000 applies downstream). The
// resolved value is injected into AUTO_HTTP_PORT/AUTO_FRONT_PORT so
// all consumers stay in sync: the generated backend main.rs (reads
// it at runtime via inherited child env), the vite proxy, and the
// rust-ui client (both read crate::util::http_port() at generation
// time, inside am.run below).
let (pac_front, pac_back) = am.pac_dev_ports();
let back_src = if back_port.is_some() {
    "cli"
} else if pac_back.is_some() {
    "pac.at"
} else {
    ""
};
// CLI flag wins; else pac.at default. Both as owned Option<String>.
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
    if std::env::var_os("AUTO_HTTP_BASE").is_none() {
        std::env::set_var("AUTO_HTTP_BASE", format!("http://127.0.0.1:{}", p.trim()));
    }
    let via = if back_src.is_empty() {
        String::new()
    } else {
        format!(" (from {})", back_src)
    };
    println!("  Backend API server port: {}{}", p.trim(), via);
}
let front_src = if front_port.is_some() {
    "cli"
} else if pac_front.is_some() {
    "pac.at"
} else {
    ""
};
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
    let via = if front_src.is_empty() {
        String::new()
    } else {
        format!(" (from {})", front_src)
    };
    println!("  Frontend dev server port: {}{}", p.trim(), via);
}
// Plan 411: VM native window startup size from pac.at `window: "WxH"`.
// Injected via env so the iced renderer (same process for -r vm)
// picks it up at window creation; non-VM renders ignore it.
if let Some((w, h)) = am.pac_window_size() {
    std::env::set_var("AUTO_VM_WINDOW", format!("{}x{}", w as u32, h as u32));
    println!("  VM window size: {}x{} (from pac.at)", w as u32, h as u32);
}
// Plan 504: `window: "fit"` — content-fit mode. Same env channel;
// the renderer measures the first frame and resizes.
if am.pac_window_fit() {
    std::env::set_var("AUTO_VM_WINDOW", "fit");
    println!("  VM window size: fit (content-measured, from pac.at)");
}
// VM native window title from pac.at, same env injection path as
// AUTO_VM_WINDOW above. PLAN-015: locale-aware — AUTO_LOCALE
// (default zh) prefers `title_zh:` when declared, else `title:`.
if let Some(t) = am.pac_display_title() {
    std::env::set_var("AUTO_VM_TITLE", &t);
    println!("  VM window title: {} (from pac.at)", t);
}
// PLAN-617 T-11: pac.at `media_root` → `AUTO_MEDIA_ROOT` env（媒体
// 服务与生成后端从进程 env 读根目录，子进程继承）。解析序是
// env → pac.at → 无（诚实空库），故 env 已设时这里不覆盖。
if std::env::var_os("AUTO_MEDIA_ROOT").is_none() {
    if let Some(root) = am.pac_media_root() {
        std::env::set_var("AUTO_MEDIA_ROOT", &root);
        println!("  Media root: {} (from pac.at)", root);
    }
}
// PLAN-043 Part 1: pac.at `photo_root` → `AUTO_PHOTO_ROOT` env
// （photo_service 解析序的 pac 层；同 media_root 的 env 优先纪律）。
if std::env::var_os("AUTO_PHOTO_ROOT").is_none() {
    if let Some(root) = am.pac_photo_root() {
        std::env::set_var("AUTO_PHOTO_ROOT", &root);
        println!("  Photo root: {} (from pac.at)", root);
    }
}
// Plan 458: UI theme + accent presets. CLI --theme/--accent >
// pac.at `theme:`/`accent:` > built-in default (dark/indigo,
// applied by the consumers when these env vars are absent).
// Plan 504 S7: priority chain gains an os-config tier —
// CLI > os-config (~/.config/autoos/apps/<name>/config.at) >
// pac.at > default. Injected via env so the iced renderer
// (-r vm, same process) and the vue index.html generator read
// the same effective value.
#[cfg_attr(not(feature = "ui-extra"), allow(unused_variables))]
let osc_app = am.pac_name();
#[cfg(feature = "ui-extra")]
let (osc_theme, osc_accent) = osc_app
    .as_deref()
    .map(auto_ui_runtime::ui::osconfig_apps::read_app_theme_accent)
    .unwrap_or((None, None));
// Core 形态：os-config 读取属 UI 域（ui-extra）——CLI>pac.at
// 两级仍解析，os-config 层缺席（与 --theme 显式值语义正交）。
#[cfg(not(feature = "ui-extra"))]
let (osc_theme, osc_accent) = (None, None);
#[cfg(feature = "ui-extra")]
let theme_from_osc = osc_theme.is_some();
#[cfg(feature = "ui-extra")]
let accent_from_osc = osc_accent.is_some();
#[cfg(not(feature = "ui-extra"))]
let _ = (&osc_theme, &osc_accent);
let theme_eff = theme
    .as_deref()
    .map(str::to_lowercase)
    .or(osc_theme)
    .or_else(|| am.pac_theme());
#[cfg(feature = "ui-extra")]
match theme_eff.as_deref() {
    Some(t) if auto_ui_runtime::ui::style::theme::THEME_PREFS.contains(&t) => {
        std::env::set_var("AUTO_UI_THEME", t);
        let via = if theme.is_some() {
            " (from cli)"
        } else if theme_from_osc {
            " (from os-config)"
        } else {
            " (from pac.at)"
        };
        println!("  UI theme: {}{}", t, via);
    }
    Some(t) => {
        println!(
            "  UI theme: {} unknown (want dark|light) — using default dark",
            t
        );
    }
    None => {}
}
// Core 形态：主题名校验表属 UI 域——显式值原样透传（下游
// 工具进程/生成器自行校验），不做 CLI 侧白名单拦截。
#[cfg(not(feature = "ui-extra"))]
if let Some(t) = theme_eff.as_deref() {
    std::env::set_var("AUTO_UI_THEME", t);
    println!("  UI theme: {} (passed through; core profile)", t);
}
// Plan 465: desktop host mode + apps dir for the vue registry.
// Env-injection idiom shared with AUTO_UI_THEME so both the vue
// and tauri pipelines (which run vue generation internally) see it.
if desktop {
    std::env::set_var("AUTO_DESKTOP", "1");
    println!("  Desktop host mode: ON");
}
if gallery {
    std::env::set_var("AUTO_GALLERY", "1");
    println!("  Gallery host mode: ON");
}
if let Some(a) = &apps {
    std::env::set_var("AUTO_DESKTOP_APPS", a);
    println!("  Desktop apps dir: {}", a);
}
let accent_eff = accent
    .as_deref()
    .map(str::to_lowercase)
    .or(osc_accent)
    .or_else(|| am.pac_accent());
#[cfg(feature = "ui-extra")]
match accent_eff.as_deref() {
    Some(a) if auto_ui_runtime::ui::style::theme::ACCENT_PRESETS.contains(&a) => {
        std::env::set_var("AUTO_UI_ACCENT", a);
        let via = if accent.is_some() {
            " (from cli)"
        } else if accent_from_osc {
            " (from os-config)"
        } else {
            " (from pac.at)"
        };
        println!("  UI accent: {}{}", a, via);
    }
    Some(a) => {
        println!(
            "  UI accent: {} unknown (want {}) — using default indigo",
            a,
            auto_ui_runtime::ui::style::theme::ACCENT_PRESETS.join("|")
        );
    }
    None => {}
}
#[cfg(not(feature = "ui-extra"))]
if let Some(a) = accent_eff.as_deref() {
    std::env::set_var("AUTO_UI_ACCENT", a);
    println!("  UI accent: {} (passed through; core profile)", a);
}
// Plan 418: UI action/binding config (Action declaration layer).
// Resolved against the project dir (auto run's cwd) so the
// renderer can load it via a stable absolute path.
if let Some(cfg) = am.pac_ui_config() {
    let path = std::path::Path::new(&cfg);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    if abs.exists() {
        std::env::set_var("AUTO_VM_ACTION_CONFIG", abs.to_string_lossy().to_string());
        println!("  VM action config: {} (from pac.at)", cfg);
    } else {
        println!(
            "  VM action config: {} not found (from pac.at) — skipped",
            cfg
        );
    }
}
// Plan 451: app identity for the OS user keymap layer — pac.at
// `name` (the DSL-actions id source when no ui_config file exists).
if let Some(n) = am.pac_name() {
    std::env::set_var("AUTO_APP_ID", &n);
}
// PLAN-031 T-06：-q 两轨分岔 gate（run_if_client_entry 同位族——
// 此处已过孵化 gate 与 pac/api 侦测）。ensure 保 daemon 在线
// （探活→spawn→退避）；vm 轨信号 = env（装载链内 lib.rs 分岔
// 认领）；rust 轨信号 = 前注 args（cargo `--` 透传 → 生成 gate
// 消费）。两信号并发注入，各轨只认各的；vue/jet/arkts/tauri
// 前端 = 出界显式报错（--rq-host 预留未实现）。
// PLAN-816 T-05：-q 渲染队列=桌面协议域（ui-extra）——Core
// 形态具名失败（stub 内实现）。
// PLAN-031 T-06 / PLAN-063 T-02：queue 族底座 gate（`-q` = rqhost 底座
// 语义糖；`--render=remote|desktop` = 显式底座声明同族生效——remote 缺
// -q 不再静默直渲染；desktop 连接在先的虚拟桌面合成器端点，不代孵）。
let base_desktop = render
    .as_deref()
    .map(|r| r.trim().eq_ignore_ascii_case("desktop"))
    .unwrap_or(false);
let base_remote = render
    .as_deref()
    .map(|r| {
        let l = r.trim().to_lowercase();
        l == "remote" || l == "rq" || l == "rqhost"
    })
    .unwrap_or(false);
if render_queue || base_remote || base_desktop {
    cmd_autodesk::rqhost_gate_validate(render.as_deref(), &args).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&AutoError::Msg(e.clone())));
            std::process::exit(1);
        }
        miette::miette!("{e}")
    })?;
    if base_desktop {
        // PLAN-063 T-02：desktop 底座 = 连接在先的虚拟桌面合成器端点
        //（不孵化——rqhost ensure/spawn 分支不进入；端点缺席 = 具名
        // 报错提示先启动桌面）。env 通道 = AUTO_VM_RENDER +
        // AUTO_DESKTOP_ENDPOINT（run_vm_rqhost_client::resolve_vm_rq_base
        // 消费；纯 std env，无需 ui-extra 符号——Core 形态已在上方
        // rqhost_gate_validate 具名失败，不落此臂）。
        let endpoint = desktop_endpoint.clone().ok_or_else(|| {
            miette::miette!(
                "--render=desktop 需要 --desktop-endpoint <pipe>（虚拟桌面合成器端点）——请先启动虚拟桌面"
            )
        })?;
        std::env::set_var("AUTO_VM_RENDER", "desktop");
        std::env::set_var("AUTO_DESKTOP_ENDPOINT", &endpoint);
        println!("  render-queue: desktop compositor endpoint ({endpoint})");
    } else {
        #[cfg(feature = "ui-extra")]
        auto_ui_runtime::ui::desktop_protocol::rqhost::ensure_rqhost_ready().map_err(
            |e| {
                if ai_mode {
                    eprintln!("{}", format_error_json(&AutoError::Msg(e.clone())));
                    std::process::exit(1);
                }
                miette::miette!("{e}")
            },
        )?;
        // Core 形态在上方 rqhost_gate_validate 已具名返回；此块
        // 仅 ui-extra 编译（桌面协议面）。
        #[cfg(feature = "ui-extra")]
        {
            let wellknown = auto_ui_runtime::ui::desktop_protocol::rqhost::wellknown_pipe();
            std::env::set_var("AUTO_RQHOST_PIPE", &wellknown);
            args = cmd_autodesk::rqhost_injected_args(&wellknown, &args);
            // PLAN-683（remote 模式）：pac `desktop_render: remote` /
            // `--render=remote` → vm 轨分岔信号（AUTO_VM_RENDER env——
            // AUTO_RQHOST_PIPE 同位缝；run_vm_rqhost_client 消费）。
            let render_mode =
                auto_ui_runtime::ui::desktop_protocol::coverage::RenderMode::resolve(
                    render.as_deref(),
                    am.pac_desktop_render().as_deref(),
                );
            if render_mode
                == auto_ui_runtime::ui::desktop_protocol::coverage::RenderMode::Remote
            {
                std::env::set_var("AUTO_VM_RENDER", "remote");
                println!(
                    "  render-queue: remote mode (headless iced host, DisplayList v2)"
                );
            }
            println!("  render-queue: rqhost shared compositor ({wellknown})");
        }
    }
}
if !ai_mode {
    info!("Running project ...");
    println!();
    println!("------------ output ------------");
}
am.run(args).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&AutoError::Msg(e.to_string())));
        // PLAN-765 T-03（纪要三 F-7）：进程内唯一未审计的静默
        // exit(1) 臂补挂——与 main_err 挂点（下方 951 行）同册。
        auto_lang_core::vm::ffi::stdlib::exit_audit(1, "main_err_ai_mode");
        std::process::exit(1);
    }
    miette::miette!("{}", e)
})?;
if !ai_mode {
    println!("------------- end --------------");
}
    Ok(())
}
