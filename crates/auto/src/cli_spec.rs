//! PLAN-014 T-03：CLI 解析声明层（自 main.rs 原样收编）。
//! 职责：clap 面（Cli/Commands/子枚举）、版本串、logo/logger 展示助手。
//! 只声明、不编排；help/flag/默认值/别名/隐藏项零改（合同 §5.3）。
//! Core 形态经 crates/auto-core 的路径目标复用本文件。
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;

use crate::{cmd_docs, cmd_stdlib};

/// Full version: crate version + git stamp injected by build.rs
/// (`git describe --always --dirty`). Two binaries both reporting `0.1.0`
/// were indistinguishable until one misparsed the source (002-counter
/// incident, plan 448); the stamp makes a stale PATH install visible at a
/// glance via `auto --version`.
const FULL_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "+",
    env!("AUTO_BUILD_GIT_VERSION")
);

#[derive(Parser, Debug)]
#[command(
    name = "auto",
    version = FULL_VERSION,
    about = "AutoNexus / Auto CLI\nThe Universal Build Coordinator & Language Environment",
    long_about = None
)]
pub(crate) struct Cli {
    /// Maximum number of errors to display before aborting (default: 20)
    #[arg(short, long, global = true, value_name = "N")]
    pub(crate) error_limit: Option<usize>,

    /// Output format for errors and diagnostics
    #[arg(long, global = true, value_name = "FORMAT")]
    pub(crate) format: Option<OutputFormat>,

    /// AI-friendly output mode: JSON structured output, suppress human-readable info
    /// Equivalent to --format json with additional output suppression
    #[arg(long = "ai", global = true)]
    pub(crate) ai: bool,

    /// Enable VM debug logging (shows task spawning, message handling, replies)
    #[arg(short = 'D', long = "debug", global = true)]
    pub(crate) debug: bool,

    /// Run an Auto script directly via AutoVM
    #[arg(index = 1)]
    pub(crate) file: Option<String>,

    /// Plan 555 T08: dump the s2s-lowered source instead of running
    /// （`auto <file> --dump-lowered`，审查面；W1=passthrough+模式头）
    #[arg(long = "dump-lowered", global = true)]
    pub(crate) dump_lowered: bool,

    /// Plan 524: args passed through to the script (`auto <file> [args...]`) —
    /// visible to the program as `process.args()[1..]`. Positional form
    /// (W0 定案: trailing_var_arg); a bare value that clashes with a subcommand
    /// name can be forced through with `--` (e.g. `auto probe.at -- run`).
    /// Note: known global flags placed AFTER the file are consumed by the flag,
    /// not passed through — put such values after `--`.
    #[arg(
        index = 2,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "ARGS"
    )]
    pub(crate) script_args: Vec<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum OutputFormat {
    /// Human-readable text output with colors (default)
    Text,
    /// Machine-readable JSON output for IDE integration
    Json,
    /// PLAN-676 SD-01: .at static-table emission (`bp list --format at` —
    /// bps-gallery registry artifact; drift-checked in CI)
    At,
}

pub(crate) fn println_logo() {
    println!("{}", "---------------------------".bright_yellow().bold());
    println!("{}", "AutoNexus / Auto CLI".bright_yellow().bold());
    println!("{}", "---------------------------".bright_yellow().bold());
}

pub(crate) fn init_logger() {
    simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )])
    .unwrap();
}

#[derive(Subcommand, Debug)]
pub(crate) enum CacheCommands {
    #[command(about = "Show cache statistics")]
    Stats,
    #[command(about = "List all cached artifacts")]
    List {
        #[arg(long)]
        type_: Option<String>,
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    #[command(about = "Run garbage collection")]
    Prune,
    #[command(about = "Clear all cached artifacts")]
    Clear,
    #[command(about = "Inspect a cache entry")]
    Inspect { name: String },
    #[command(about = "Verify cache integrity")]
    Verify,
}

#[derive(Subcommand, Debug)]
pub(crate) enum DeviceAction {
    #[command(about = "List connected hardware devices and ports")]
    List,
    #[command(about = "Select a specific port for deployment")]
    Select { port: String },
}

#[derive(Subcommand, Debug)]
pub(crate) enum EnvAction {
    #[command(about = "Reset AutoMan configuration and index")]
    Reset,
    #[command(about = "Install a custom am.at configuration file")]
    Install { file: String },
    #[command(about = "Manage AutoCache")]
    Cache {
        #[command(subcommand)]
        command: CacheCommands,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum TransTarget {
    Ts {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .ts extension)"
        )]
        output: Option<String>,
    },
    C {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .c extension)"
        )]
        output: Option<String>,
    },
    Rust {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .rs extension)"
        )]
        output: Option<String>,
        #[arg(
            short,
            long,
            help = "Merge all discovered modules into a single .rs file"
        )]
        merge: bool,
    },
    Python {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .py extension)"
        )]
        output: Option<String>,
    },
    Js {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .js extension)"
        )]
        output: Option<String>,
    },
    Gd {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .gd extension)"
        )]
        output: Option<String>,
    },
    Tscn {
        #[arg(
            short,
            long,
            help = "Output file path (default: same name with .tscn extension)"
        )]
        output: Option<String>,
    },
    /// Emit both .tscn (from any `scene`) and .gd (from functions) for one .at file.
    Godot {
        #[arg(
            short,
            long,
            help = "Output base name (default: source name; writes <base>.tscn + <base>.gd)"
        )]
        output: Option<String>,
    },
    /// Plan 555 T07: Auto → Auto（脚本糖 → 正常模式桥，s2s 改写器；
    /// W1 规则表空置=identity；--dump-lowered 的产物源）
    Auto {
        #[arg(short, long, help = "Output file path (default: stdout)")]
        output: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum UiAction {
    /// Generate self-contained widget SFCs into an output directory
    Build {
        #[arg(long, help = "Target framework (default: vue)", default_value = "vue")]
        target: String,
        #[arg(
            long,
            help = "Output directory (default: packages/widgets/registry)",
            default_value = "packages/widgets/registry"
        )]
        out: String,
        #[arg(
            long,
            value_delimiter = ',',
            help = "Comma-separated widget names (default: all registered)"
        )]
        widgets: Vec<String>,
    },
    /// List registered library widgets
    List,
    /// Inspect an .at file — show widget structure and validation warnings (Plan 362 Phase 5)
    Inspect {
        /// Path to the .at file
        file: String,
    },
    /// List AURA widgets not yet covered by @auto-ui/widgets (Plan 337)
    Backlog,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WizardAction {
    /// Create a new AutoUI project from template
    New {
        /// Project name
        name: String,
    },
    /// Add a new widget to an existing project
    Add {
        /// Widget name
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum BpAction {
    /// List blueprint packages (grouped by kind); `--format at` (global flag)
    /// emits the PLAN-676 SD-01 .at static table for gallery consumption
    List,
    /// Print a blueprint's spec + variants + gotchas (the agent skill interface)
    Show {
        /// `kind/name` of the blueprint (e.g. `form/login`)
        key: String,
    },
    /// Copy a reference implementation into a consumer project (adopt-and-edit)
    Add {
        /// `kind/name` of the blueprint (e.g. `form/login`)
        key: String,
        /// Variant to copy (default: the package's first variant)
        #[arg(long)]
        reference: Option<String>,
        /// Output directory (default: src/front/bps)
        #[arg(long, default_value = "src/front/bps")]
        out: String,
        /// PLAN-639 L1: emit a declarative bind artifact (zero-copy import)
        /// instead of copying the reference implementation
        #[arg(long)]
        bind: bool,
        /// pac.at dep name pointing at the blueprints package (with --bind)
        #[arg(long, default_value = "bps")]
        dep: String,
    },
    /// PLAN-639 L1 import/bind: emit a declarative bind artifact that imports
    /// the blueprint from the declared package (zero-copy consumption)
    Bind {
        /// `kind/name` of the blueprint (e.g. `form/login`)
        key: String,
        /// Variant to import (default: the package's first variant)
        #[arg(long)]
        reference: Option<String>,
        /// Output directory (default: src/front/bps)
        #[arg(long, default_value = "src/front/bps")]
        out: String,
        /// pac.at dep name pointing at the blueprints package
        #[arg(long, default_value = "bps")]
        dep: String,
    },
    /// Static acceptance check on a generated/copied .at (agent repair-loop gate)
    Check {
        /// Path to the .at file to check
        file: String,
        /// Optional `kind/name` spec to check extension-point EDIT markers against
        #[arg(long)]
        spec: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    // ========== Project Creation ==========
    #[command(about = "Create a new Auto project (app, lib, gear, gadget)")]
    New {
        name: String,
        #[arg(short, long, help = "Project template (app, jet, capp, lib, clib)")]
        template: Option<String>,
    },
    #[command(about = "Initialize an Auto project in the current directory")]
    Init {
        #[arg(short, long, help = "Project template (e.g. app, jet)")]
        template: Option<String>,
    },

    // ========== Build & Run ==========
    #[command(
        about = "Compile the project based on pac.at render target",
        alias = "b"
    )]
    Build {
        #[arg(short, long)]
        dir: Option<String>,
        #[arg(short, long)]
        port: Option<String>,
        #[arg(
            short = 'B',
            long = "back-port",
            help = "Backend HTTP API server port (default 8080)"
        )]
        back_port: Option<String>,
        #[arg(
            short = 'F',
            long = "front-port",
            help = "Frontend dev server port (default 3000)"
        )]
        front_port: Option<String>,
        #[arg(
            short,
            long,
            help = "Render target to use (vue, rust, vm, jet, arkts, tauri)"
        )]
        render: Option<String>,
        #[arg(
            long,
            help = "Parser scenario: core, ui, or shell (overrides pac.at scene)"
        )]
        scene: Option<String>,
        #[arg(
            long,
            help = "Stop after code generation; skip npm/gradle install and build (vue backend)"
        )]
        gen_only: bool,
        #[arg(
            long,
            help = "Escalate codegen validation warnings (vue backend) to build failure (default since plan 015 P0#1; kept for compatibility)"
        )]
        strict: bool,
        #[arg(
            long,
            help = "Keep building despite codegen validation warnings (vue backend); restores the pre-015 default"
        )]
        lenient: bool,
    },
    /// PLAN-031：rqhost 共享合成器 daemon——`auto run -q` 客户端的宿主 OS
    /// 原生窗承载（多 app 共享单实例；末窗自退）。
    #[command(
        about = "PLAN-031: rqhost shared compositor daemon (native OS windows for `run -q` clients)"
    )]
    Rqhost {
        #[arg(
            long,
            help = "Well-known pipe override (default: autodesk-rqhost; $AUTO_RQHOST_WELLKNOWN also honored)"
        )]
        pipe: Option<String>,
        #[arg(
            long,
            help = "PLAN-102 D-1: windowless bridge host — serve adopted apps' frames/InputMsg over a loopback WebSocket (with --bridge-token) instead of native OS windows"
        )]
        bridge_ws: Option<u16>,
        #[arg(
            long,
            requires = "bridge_ws",
            help = "Static token guarding the bridge WS endpoint (loopback + token; no token = no listener)"
        )]
        bridge_token: Option<String>,
    },
    #[command(about = "Build and run the executable/dev-server", alias = "r")]
    Run {
        #[arg(short, long)]
        dir: Option<String>,
        #[arg(short, long)]
        port: Option<String>,
        #[arg(
            short = 'B',
            long = "back-port",
            help = "Backend HTTP API server port (default 8080)"
        )]
        back_port: Option<String>,
        #[arg(
            short = 'F',
            long = "front-port",
            help = "Frontend dev server port (default 3000)"
        )]
        front_port: Option<String>,
        #[arg(
            short,
            long,
            help = "Render target to use (vue, rust, vm, jet, arkts, tauri)"
        )]
        render: Option<String>,
        #[arg(short = 'q', long = "render-queue", action = clap::ArgAction::SetTrue, help = "PLAN-031: render via shared rqhost compositor (native OS window; vm/rust tracks)")]
        render_queue: bool,
        #[arg(
            long,
            help = "PLAN-063: desktop compositor endpoint pipe for `--render=desktop` (queue family; endpoint must already be running)"
        )]
        desktop_endpoint: Option<String>,
        #[arg(
            long,
            help = "Backend server mode: vm (AutoVM HTTP) or rust (a2r, default)"
        )]
        server: Option<String>,
        #[arg(
            long,
            help = "Plan 340: merge frontend+backend VM in-process (default true). --no-merge uses HTTP between VMs"
        )]
        #[arg(long = "no-merge", action = clap::ArgAction::SetTrue, help = "Plan 340: use HTTP between VMs (split mode)")]
        no_merge: bool,
        #[arg(
            long,
            help = "Parser scenario: core, ui, or shell (overrides pac.at scene)"
        )]
        scene: Option<String>,
        #[arg(
            long,
            help = "Plan 458: UI theme preset (dark, light). Overrides pac.at `theme:`; default dark"
        )]
        theme: Option<String>,
        #[arg(
            long,
            help = "Plan 458: UI accent preset (indigo, coral, ocean, sage, amber). Overrides pac.at `accent:`; default indigo"
        )]
        accent: Option<String>,
        #[arg(
            long,
            help = "Plan 465: desktop host mode — vue scaffold becomes the virtual-desktop shell consuming an apps registry"
        )]
        desktop: bool,
        #[arg(
            long,
            help = "Plan 549: UI gallery host mode — harvest examples/ui into demos-registry.ts"
        )]
        gallery: bool,
        #[arg(
            long,
            help = "Plan 465: apps directory for the desktop registry (default <workspace>/examples/ui; corpus owner auto-ui since PLAN-007)"
        )]
        apps: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Plan 547: explicitly select Rust merged in-process mode (already the default for --render=rust)")]
        merged: bool,
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// PLAN-736: HTTP 服务独立启动（server-only；不启 UI/Vite/桌面）。
    /// 命名注记：`auto serve` 已被 Plan 269 AutoVM daemon 占用，故为 `auto service`。
    #[command(
        about = "Start the project's HTTP service standalone (no UI): --server vm|rust [--http-config <json file>]"
    )]
    Service {
        #[arg(short, long)]
        dir: Option<String>,
        #[arg(
            long,
            help = "Service track: vm (in-process AutoVM HTTP) or rust (generated axum backend)"
        )]
        server: String,
        #[arg(
            long,
            help = "HTTP service config JSON file (profile/listen/limits/cors/proxy); omit = development defaults (127.0.0.1:8080)"
        )]
        http_config: Option<String>,
        #[arg(
            long,
            help = "HTTP service config as inline JSON (mutually exclusive with --http-config)"
        )]
        http_config_inline: Option<String>,
        #[arg(
            short = 'B',
            long = "back-port",
            help = "Explicit override for the configured listen port"
        )]
        back_port: Option<String>,
    },
    /// PLAN-1006: 导出 Rust 服务成员为可迁移交付包（export-only；不启动监听、
    /// 不复用 UI gen 路由）。薄接线到 auto-web 公开导出库接口。
    #[command(
        about = "Export the project's generated Rust service member into a relocatable delivery bundle (PLAN-1006)"
    )]
    ServiceExport {
        #[arg(short, long)]
        dir: Option<String>,
        #[arg(
            long,
            help = "Delivery bundle output root (created and managed by the exporter)"
        )]
        delivery_root: String,
        #[arg(
            long,
            help = "Project-relative plain file to include in the bundle resources/ (repeatable)"
        )]
        resource: Vec<String>,
    },
    #[command(about = "Run all #[test] functions in the project", alias = "t")]
    Test {
        #[arg(short, long, help = "Source file or directory to test")]
        dir: Option<String>,
        #[arg(short, long, help = "Run only tests matching this filter")]
        filter: Option<String>,
        #[arg(short = 'v', long, help = "Show test output (print statements)")]
        verbose: bool,
    },
    #[command(
        name = "test:ui",
        about = "Run Playwright UI tests in <dir>/tests (start the app first: auto run <dir>)"
    )]
    TestUi {
        #[arg(
            short,
            long,
            help = "Project directory containing tests/ (default: current dir)"
        )]
        dir: Option<String>,
        #[arg(
            short = 'H',
            long,
            help = "Run Playwright with a visible browser window"
        )]
        headed: bool,
        #[arg(
            short,
            long,
            help = "Only run tests whose title contains this substring"
        )]
        filter: Option<String>,
        #[arg(
            long,
            help = "Open the HTML report from the last run instead of testing"
        )]
        report: bool,
    },
    #[command(about = "Remove the .auto/build directory and artifacts")]
    Clean {
        #[arg(short, long)]
        dir: Option<String>,
    },
    #[command(about = "Watch .at files and auto-regenerate SFCs (Plan 362)")]
    Watch {
        #[arg(short, long)]
        dir: Option<String>,
        #[arg(
            short = 'B',
            long = "back-port",
            help = "Backend HTTP API server port (default 8080)"
        )]
        back_port: Option<String>,
        #[arg(
            short = 'F',
            long = "front-port",
            help = "Frontend dev server port (default 3000)"
        )]
        front_port: Option<String>,
    },
    #[command(about = "Scaffold a new project or widget from templates (Plan 363)")]
    Wizard {
        #[command(subcommand)]
        action: WizardAction,
    },

    // ========== Dependencies ==========
    #[command(about = "Add a dependency to pac.at")]
    Add { package: String },
    #[command(about = "Fetch and resolve all dependencies (Replaces scan/pull)")]
    Fetch,
    #[command(about = "Show the dependency graph")]
    Deps,

    // ========== Hardware & Embedded ==========
    #[command(about = "Manage connected hardware devices and ports")]
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },
    #[command(about = "Export the project to a specific format (cmake, iar, ghs)")]
    Export {
        #[arg(short, long, help = "Name of the port to export")]
        port: String,
        // NOTE: named `fmt` (not `format`) to avoid clashing with the global
        // `--format <OutputFormat>` error-diagnostic flag, which caused a clap
        // downcast panic whenever `export --format ...` was used.
        #[arg(
            short = 'f',
            long = "fmt",
            help = "Format to export to (cmake, iar, ghs)"
        )]
        fmt: String,
    },

    // ========== Project Utils ==========
    #[command(about = "Show package, backend, and target information", alias = "i")]
    Info {
        #[arg(short, long)]
        target: Option<String>,
    },
    #[command(about = "Open the current project in the default IDE", alias = "o")]
    Open,

    // ========== Environment ==========
    // #[command(about = "Upgrade auto.exe toolchain to the latest version")]
    // Upgrade,  // NOTE: disabled — zip dependency removed
    #[command(about = "Manage global AutoMan configurations and cache")]
    Env {
        #[command(subcommand)]
        action: EnvAction,
    },

    // ========== Code Generation ==========
    #[command(about = "Generate code from .at files (kotlin for jet backend)")]
    Gen {
        #[arg(
            short,
            long,
            help = "Output directory (default: dist for vue, current dir for jet)"
        )]
        output: Option<String>,
        #[arg(short, long, help = "Generate full project structure")]
        project: bool,
        #[arg(
            long,
            help = "Parser scenario: core, ui, or shell (overrides pac.at scene)"
        )]
        scene: Option<String>,
        #[arg(
            long,
            help = "PLAN-792: rust backend explicit relocatable delivery bundle root \
                    (managed source/resource bundle + consumer-root patch; dev mode unchanged)"
        )]
        delivery_root: Option<String>,
    },

    // ========== AutoUI Widget Library (Plan 331) ==========
    #[command(about = "AutoUI widget library commands")]
    Ui {
        #[command(subcommand)]
        action: UiAction,
    },

    // ========== AutoUI Blueprints (Plan 343, Design 17; PLAN-639 rename) ==========
    #[command(about = "AutoUI blueprint catalog commands (Skill-tier)")]
    Bp {
        #[command(subcommand)]
        action: BpAction,
    },
    /// Deprecated alias of `auto bp` (PLAN-639 rename) — prints a warning and forwards
    #[command(hide = true)]
    Block {
        #[command(subcommand)]
        action: BpAction,
    },

    // ========== Legacy / Dev Tools ==========
    #[command(
        about = "AutoLang REPL (deprecated - uses TreeWalker Interpreter)",
        hide = true
    )]
    OldRepl,
    #[command(about = "Parse Auto to JSON", hide = true)]
    Parse { code: String },
    #[command(about = "Evaluate Auto expression", hide = true)]
    Eval { code: String },
    #[command(about = "Treat File as AutoConfig", hide = true)]
    Config { path: String },
    // ========== Transpile (single file) ==========
    #[command(about = "Transpile a single .at file to a target language")]
    Trans {
        /// Input .at file to transpile
        #[arg(short, long, help = "Input .at file to transpile")]
        path: String,
        #[command(subcommand)]
        target: TransTarget,
    },
    #[command(about = "Transpile Auto to C", hide = true)]
    C {
        path: String,
        #[arg(short, long, help = "Compilation target", global = false)]
        target: Option<String>,
    },
    #[command(about = "Transpile Auto to Rust", hide = true)]
    Rust { path: String },
    #[command(about = "Transpile Rust to AutoLang (r2a)", hide = true)]
    R2a {
        path: String,
        #[arg(short, long, help = "Output file path")]
        output: Option<String>,
    },
    #[command(about = "Transpile Auto to Python", hide = true)]
    Python { path: String },
    #[command(about = "Transpile Auto to JavaScript", hide = true)]
    JavaScript { path: String },
    #[command(about = "Transpile Auto to GDScript (Godot 4.x)", hide = true)]
    GDScript { path: String },
    #[command(about = "Transpile stdlib to C", hide = true)]
    A2cStdlib,

    // ========== Debug (Plan 199) ==========
    #[command(
        about = "Debug an Auto program with interactive debugger",
        alias = "dbg"
    )]
    Debug {
        /// Path to the .at file to debug
        file: String,
        /// Agent-friendly JSON mode: each pause emits JSON state, read commands from stdin
        #[arg(long = "agent", short = 'a')]
        agent: bool,
    },

    // ========== C FFI Bindgen (Plan 216) ==========
    #[command(about = "List available C FFI bindings from manifests")]
    Cffi {
        /// Show functions for a specific header (e.g., "string.h", "math.h")
        #[arg(long)]
        header: Option<String>,
    },

    // ========== MCP Server (Plan 265) ==========
    #[command(about = "Start AutoVM MCP server (stdio transport for AI agents)")]
    Mcp,

    // ========== Daemon + CLI (Plan 269) ==========
    #[command(about = "Start AutoVM daemon server (stateful VM, persistent sessions)")]
    Serve {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
        /// Run in stdio mode (used internally by auto req)
        #[arg(long)]
        stdio: bool,
        /// Named pipe name (Windows) or socket name (Unix)
        #[arg(short, long, default_value = "autovm")]
        pipe_name: String,
        /// Maximum number of concurrent sessions
        #[arg(long, default_value_t = 20)]
        max_sessions: usize,
        /// Session idle timeout in seconds (0 = no timeout)
        #[arg(long, default_value_t = 1800)]
        timeout: u64,
    },

    #[command(about = "Send request to AutoVM daemon (eval code, inspect sessions)")]
    Req {
        /// Session ID to use
        #[arg(short, long)]
        session: Option<String>,
        /// Named pipe name (must match daemon)
        #[arg(short = 'p', long, default_value = "autovm")]
        pipe_name: String,
        /// Create a new session and print its ID
        #[arg(long)]
        new_session: bool,
        /// Inspect session state
        #[arg(long)]
        inspect: bool,
        /// Reset session
        #[arg(long)]
        reset: bool,
        /// Delete session
        #[arg(long)]
        delete: bool,
        /// Export session as .at source
        #[arg(long)]
        snapshot: bool,
        /// List active sessions
        #[arg(long)]
        list: bool,
        /// Output as JSON (machine-readable)
        #[arg(long)]
        json: bool,
        /// Auto code to evaluate
        code: Option<String>,
    },

    // ========== Schema Docs (Plan 435 P8-1) ==========
    #[command(about = "Schema-driven docs generation (core.md / kitchen-sink.at)")]
    Docs {
        #[command(subcommand)]
        action: cmd_docs::DocsAction,
    },

    // ========== Stdlib Assembly (PLAN-738) ==========
    #[command(about = "Stdlib assembly inventory / manifest / core gate (inspect)")]
    Stdlib {
        #[command(subcommand)]
        action: cmd_stdlib::StdlibAction,
    },
}
