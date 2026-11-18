//! PLAN-736: 工程级 HTTP 服务独立启动（`auto serve`）。
//!
//! 两条服务轨，同一份解析后的 [HttpServiceConfig]：
//! - **vm**：进程内 VM 网络轨（`auto_lang_core::run_file` + 服务配置 seam），
//!   不启动任何 UI/Vite/桌面。
//! - **rust**：真实生成链（`api_gen::generate_api`，strict 门）→ 子进程
//!   `cargo run`，配置经 `AUTO_HTTP_SERVICE_JSON` 注入，子进程自行
//!   re-resolve（两端 effective hash 必须一致）。
//!
//! 与 legacy `auto run` 的边界：serve 面**不 kill 占端口进程**（端口冲突 =
//! bind 失败 = 非零退出诊断）；无文件 legacy 端口链不受影响。

use auto_web::contract::{resolve_service_config, HttpServiceConfig};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 显式 `--http-config` 的两种载体（文件路径 / 内联 JSON）。
#[derive(Debug, Clone)]
pub enum HttpConfigSource {
    File(PathBuf),
    Inline(String),
}

impl HttpConfigSource {
    fn read_json(&self) -> Result<String, String> {
        match self {
            HttpConfigSource::Inline(s) => Ok(s.clone()),
            HttpConfigSource::File(p) => {
                let mut s = String::new();
                std::fs::File::open(p)
                    .map_err(|e| format!("failed to open http-config {}: {e}", p.display()))?
                    .read_to_string(&mut s)
                    .map_err(|e| format!("failed to read http-config {}: {e}", p.display()))?;
                Ok(s)
            }
        }
    }
}

/// 解析最终服务配置。
///
/// - `source`：None = 隐式 development 缺省（决策 §3.1：loopback:8080）。
/// - `cli_port`：`-B` 显式旗标（覆盖 listen.port）；与配置 listen 的并存
///   语义在 resolve 内实现。
pub fn resolve_config(
    source: Option<&HttpConfigSource>,
    cli_port: Option<u16>,
) -> Result<HttpServiceConfig, String> {
    match source {
        Some(src) => {
            let json = src.read_json()?;
            resolve_service_config(&json, cli_port).map_err(|e| e.to_string())
        }
        None => resolve_service_config(r#"{"profile":"development"}"#, cli_port)
            .map_err(|e| e.to_string()),
    }
}

/// 服务入口（`auto serve` 实现）。返回进程退出码。
///
/// 运行失败（生成失败/bind 冲突/VM 编译失败）返回非零码；调用方据此退出。
pub fn serve_project(
    project_dir: &Path,
    server: &str,
    config: HttpServiceConfig,
) -> Result<i32, String> {
    match server {
        "vm" => serve_vm(project_dir, config),
        "rust" => serve_rust(project_dir, config),
        // PLAN-982（形状 A，用户裁定）：`av` 轨＝Rust HTTP 宿主 + **ACE AV** 执行
        // handler。宿主能力（`platform.http-service/1`）尚在实现中，因此这里先给
        // **具名拒绝**：让 983 能显式选中该轨并拿到可编程判断的错误，而不是
        // "unknown --server" 这种配置错误。
        "av" | "ace" => serve_av(project_dir, config),
        other => Err(format!(
            "unknown --server `{other}` (expected `vm`, `rust` or `av`)"
        )),
    }
}

/// PLAN-982：`av` 轨——**真实 HTTP 宿主（Rust）＋ ACE AV 执行 handler**。
///
/// 第一条真实请求的最小完整链路（round 149 起）：
/// 1. 工程发现：`src/back/api.at`（产品自己的 project-discovery 规则）；
/// 2. 真实监听：`TcpListener` 绑 `config.listen`，每连接解析请求行；
/// 3. **AV 执行**：每个请求跑一次 `run_ace(AceAV)`，入口由 `AUTO_AV_ENTRY`
///    指定（默认 `d_main`），预算由 `AUTO_AV_WALL_MS` 指定；
/// 4. 响应：成功 ⇒ 200 + 值文本；诊断/宿主失败 ⇒ 500 + 具名 code；预算耗尽 ⇒
///    504 + `ace.service/timeout`；
/// 5. 收据：`served` / `av_dispatches` / `legacy_dispatches`（后者必须为 0）。
///
/// 测试用阀门：`AUTO_SERVICE_MAX_REQUESTS=N` ⇒ 服务满 N 个请求后自行关停（关停后
/// 监听关闭，连接被拒）。**绝不回退**到 `vm` 轨。
fn serve_av(project_dir: &Path, config: HttpServiceConfig) -> Result<i32, String> {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // 桥必须**由产品装配**（`auto` 启动时已 `install()`；本 crate 不依赖宿主桥，
    // 分层同 `rust_engine`/`script_engine`）。无桥 ⇒ 同一具名拒绝，绝不静默回退。
    if !auto_lang_core::ace_executor_installed() {
        return Err(
            "ace.service/bridge-unavailable: the ACE bridge could not be installed in this build"
                .to_string(),
        );
    }
    let api_path = auto_lang_core::config::resolve_back_api(project_dir).ok_or_else(|| {
        format!(
            "ace.service/no-api: no back API found under {} (expected src/back/api.at); \
             `auto service --server av` never falls back to the legacy VM track",
            project_dir.display()
        )
    })?;
    let source = std::fs::read_to_string(&api_path)
        .map_err(|e| format!("ace.service/api-unreadable: {}: {e}", api_path.display()))?;

    let entry = std::env::var("AUTO_AV_ENTRY").unwrap_or_else(|_| "d_main".to_string());
    let wall_ms = std::env::var("AUTO_AV_WALL_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok());
    let max_requests = std::env::var("AUTO_SERVICE_MAX_REQUESTS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok());

    let addr = format!("{}:{}", config.listen_addr, config.listen_port);
    let listener = TcpListener::bind(&addr)
        .map_err(|e| format!("ace.service/bind: cannot bind {addr}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("ace.service/bind: {e}"))?;
    println!(
        "▶ Starting AV HTTP service on {bound} (entry {entry}, profile {})",
        config.profile.as_str()
    );

    let mut served = 0u64;
    let mut av_dispatches = 0u64;
    loop {
        let (mut stream, _peer) = match listener.accept() {
            Ok(pair) => pair,
            Err(e) => return Err(format!("ace.service/accept: {e}")),
        };
        let mut buf = [0u8; 4096];
        let n = stream
            .read(&mut buf)
            .map_err(|e| format!("ace.service/read: {e}"))?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        // 就绪探针（只连不发）不消耗请求、也不派发——真实 HTTP 客户端总会先说话。
        if n == 0 {
            continue;
        }
        let (method, path) = request
            .lines()
            .next()
            .map(|line| {
                let mut parts = line.split_whitespace();
                (
                    parts.next().unwrap_or("GET").to_string(),
                    parts.next().unwrap_or("/").to_string(),
                )
            })
            .unwrap_or_else(|| ("GET".to_string(), "/".to_string()));

        // 每个请求**重新合成**源码：请求上下文以常量函数注入（见 inject_request_context）。
        let program = inject_request_context(&method, &path, &source);
        let (status, body, dispatched) = av_handle(&program, &entry, wall_ms, &path);
        if dispatched {
            av_dispatches += 1;
        }
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        served += 1;
        if max_requests.is_some_and(|max| served >= max) {
            break;
        }
    }
    drop(listener);

    let legacy = crate::config_engine::observation().legacy_dispatches;
    let same = legacy == 0 && av_dispatches >= 1;
    println!(
        "✓ AV service receipt: served={served} av_dispatches={av_dispatches} \
         legacy_dispatches={legacy} same_sdk={same}"
    );
    if !same {
        return Err(format!(
            "ace.service/receipt: the AV track must dispatch (av={av_dispatches}) and never \
             fall back (legacy={legacy})"
        ));
    }
    Ok(0)
}

/// 把一个请求的上下文注入到 AV 程序：**源码常量**（第一阶段的契约）。
///
/// 现状：AV 车道还没有把请求内容送进程序的宿主面（`AceRequest` 的 args/input 在
/// AV 侧是具名拒绝，`platform.input/1` 尚未实现）。第一阶段用**每次请求重新合成的
/// 源码**承载请求上下文：程序可以调用
///
/// - `__av_request_path_len() int` —— 请求路径的字节长度；
/// - `__av_request_method_len() int` —— 方法名的字节长度。
///
/// 这两个名字是**服务契约的一部分**（不是内部细节）：api.at 可以按它分支。下一阶段
/// 用 `platform.input/1` 把整段请求文本送进程序后，这两个常量函数继续保留为
/// 轻量便捷面。
fn inject_request_context(method: &str, path: &str, source: &str) -> String {
    format!(
        "fn __av_request_path_len() int {{ return {} }}\n\
         fn __av_request_method_len() int {{ return {} }}\n\
         {source}",
        path.len(),
        method.len()
    )
}
fn av_handle(
    source: &str,
    entry: &str,
    wall_ms: Option<u64>,
    path: &str,
) -> (&'static str, String, bool) {
    use auto_lang_core::{run_ace, AceEngineChoice, AceOutcome, AceRequest};
    let wall = wall_ms.map(std::time::Duration::from_millis);
    let request = AceRequest {
        entry_def_id: entry,
        wall,
        ..AceRequest::simple(source)
    };
    let report = run_ace(AceEngineChoice::AceAV, &request);
    match &report.outcome {
        AceOutcome::Success { value, steps } => {
            let rendered = render_ace_value(value);
            let body = format!("{rendered}\npath={path} steps={steps}");
            ("200 OK", body, true)
        }
        AceOutcome::Cancelled => (
            "504 Gateway Timeout",
            "ace.service/timeout: the AV budget was exhausted\n".to_string(),
            true,
        ),
        AceOutcome::Diagnostic { code, .. } => (
            "500 Internal Server Error",
            format!("ace.service/handler-diagnostic: {code}\n"),
            true,
        ),
        AceOutcome::HostFailure { code, .. } => (
            "500 Internal Server Error",
            format!("ace.service/handler-host-failure: {code}\n"),
            true,
        ),
        AceOutcome::Denied { code, .. } => (
            "500 Internal Server Error",
            format!("ace.service/handler-denied: {code}\n"),
            true,
        ),
        // 预算回收（步数/字节/栈）与墙钟超时同类：请求被回收，回 504。
        AceOutcome::ResourceDenied { kind, .. } => (
            "504 Gateway Timeout",
            format!("ace.service/budget-exhausted: {kind}\n"),
            true,
        ),
    }
}

/// `AceValue` → 响应体文本（只做最小渲染：数值与文本；其它给 debug 形）。
fn render_ace_value(value: &auto_lang_core::AceValue) -> String {
    use auto_lang_core::AceValue as V;
    match value {
        V::Unit => String::new(),
        V::Bool(b) => b.to_string(),
        V::I8(v) => v.to_string(),
        V::U8(v) => v.to_string(),
        V::I32(v) => v.to_string(),
        V::U32(v) => v.to_string(),
        V::I64(v) => v.to_string(),
        V::U64(v) => v.to_string(),
        V::F32(v) => v.to_string(),
        V::F64(v) => v.to_string(),
        other => format!("{other:?}"),
    }
}

// ============================================================================
// VM 轨：进程内 VM 网络服务
// ============================================================================

fn serve_vm(project_dir: &Path, config: HttpServiceConfig) -> Result<i32, String> {
    let api_path = auto_lang_core::config::resolve_back_api(project_dir).ok_or_else(|| {
        format!(
            "no back API found under {} (expected src/back/api.at)",
            project_dir.display()
        )
    })?;
    let api_str = api_path.to_string_lossy().to_string();

    // 模块搜索目录注入（与 rust_ui::start_vm_server 同一语义：split 模式
    // api.at 的 `use types` 需要解析 src/front）。
    let src_front = project_dir.join("src").join("front");
    if src_front.exists() {
        let joined = src_front.to_string_lossy().to_string();
        let existing = std::env::var("AUTO_SOURCE_DIRS").unwrap_or_default();
        let combined = if existing.is_empty() {
            joined
        } else {
            format!("{existing};{joined}")
        };
        std::env::set_var("AUTO_SOURCE_DIRS", &combined);
    }

    auto_web::http_server::reset_serve_state();
    auto_web::install_http_service_host_if_absent();
    let launch = auto_web::service_launch(Arc::new(config.clone()));
    let display_addr = format!("{}:{}", config.listen_addr, config.listen_port);
    println!(
        "▶ Starting VM HTTP service on {display_addr} (profile: {})",
        config.profile.as_str()
    );

    // 与 start_vm_server 同款大栈线程（flattened api.at 解析超 1MB 默认栈）。
    let handle = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .name("auto-vm-http-service".into())
        .spawn(move || {
            auto_lang_core::vm::host_capabilities::http_service::with_http_service_launch(
                Some(launch),
                || crate::language::run_language_file(&api_str),
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
        .map_err(|e| format!("failed to spawn VM service thread: {e}"))?;

    // bind 判定：等待 serve hook 报告真实 bound 地址（port=0 → 内核分配）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut bound = None;
    while std::time::Instant::now() < deadline {
        if let Some(addr) = auto_web::http_server::bound_addr_snapshot() {
            bound = Some(addr);
            break;
        }
        if handle.is_finished() {
            break; // 编译失败/启动失败：join 后按 fatal 报告
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    match bound {
        Some(addr) => {
            println!("  ✓ VM HTTP service ready (bound {})", addr);
            println!(
                "  AUTO_SERVICE_READY {{\"bound\":\"{addr}\",\"profile\":\"{}\",\"config_hash\":\"{:016x}\"}}",
                config.profile.as_str(),
                config.effective_config_hash()
            );
        }
        None => {
            // 未在期限内 bind：join 收尸并按 fatal 非零退出（消息三源取一：
            // serve fatal > run_file 的 Err > 兜底超时文案）。
            let join_result = handle.join();
            let fatal = auto_web::http_server::take_serve_fatal_error();
            if let Some(f) = fatal {
                return Err(f);
            }
            if let Ok(Err(run_err)) = join_result {
                return Err(format!("VM service run failed: {run_err}"));
            }
            return Err("VM HTTP service did not become ready within 120s".to_string());
        }
    }

    // Ctrl+C 由 serve_async 内注册的处理器驱动优雅关闭；此处 join 至线程结束。
    let run_result = handle.join();
    let fatal = auto_web::http_server::take_serve_fatal_error();
    match (run_result, fatal) {
        (Ok(Ok(_)), None) => {
            println!("  ✓ VM HTTP service stopped cleanly");
            Ok(0)
        }
        (Ok(_), Some(f)) | (Err(_), Some(f)) => Err(format!("http service failed: {f}")),
        (Ok(Err(error)), None) => Err(format!("VM service run failed: {error}")),
        (Err(_), None) => Err("VM service thread failed".to_string()),
    }
}

// ============================================================================
// Rust 轨：生成 + 子进程承载
// ============================================================================

fn serve_rust(project_dir: &Path, config: HttpServiceConfig) -> Result<i32, String> {
    // PLAN-910：workspace 落点由共享单源 `resolve_rust_workspace_dir` 以
    // **项目目录**为锚解析（CWD 无关）——本处原有的重复推断器与
    // AUTO_RUST_WORKSPACE 强制设置已退役，ensure/generate/cargo run 三方
    // 天然一致。
    // 共享 workspace 根确保（成员表+相对路径锚点按实际落点重算；serve 面无
    // generate_rust_ui 前置，必须自行 ensure——陈旧根的坏 path dep 会毒化
    // 整个 workspace 的 cargo 解析；写前比较，未变不重写）。
    let ws_dir = auto_web::rust_workspace::try_ensure_shared_workspace(project_dir)
        .map_err(|e| format!("shared workspace producer 预检失败: {e}"))?;
    // 真实生成链（734 strict 门；失败=Err 硬传播，不落旧产物）。
    auto_web::generate_api(project_dir, "rust")
        .map_err(|e| format!("Failed to generate Rust backend: {e}"))?;
    let back_name = auto_web::rust_workspace::generated_build::member_dir_name(
        project_dir,
        auto_web::rust_workspace::generated_build::MemberRole::Back,
    );
    let backend_dir = ws_dir.join(&back_name);
    // PLAN-910：受管生成构建上下文——target 根 + flags 经 env 钉住，与 UI
    // 构建路径同源（同 producer 兼容工程共用受管缓存根，另一 worktree 隔离）。
    let build_context = auto_web::rust_workspace::generated_build::GeneratedBuildContext::resolve(
        project_dir,
        &ws_dir,
        auto_web::rust_workspace::WorkspaceRequirements::Service,
    )
    .map_err(|e| format!("generated build context 解析失败: {e}"))?;
    build_context
        .write_context_file()
        .map_err(|e| format!("generated build context 记录失败: {e}"))?;
    // PLAN-910：服务运行期间持有 target 根 lease（清理器拒绝活 pid 持有的根；
    // 服务退出/被杀 → lease 释放或留 dead-pid 过期件，清理器报告并忽略）。
    let _target_lease = auto_web::rust_workspace::generated_build::TargetLease::acquire(
        &build_context.target_root,
        "serve_rust",
    )
    .map_err(|e| format!("target lease 获取失败: {e}"))?;
    let cargo_toml = backend_dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(format!(
            "backend Cargo.toml missing under {} (generate_api reported success?)",
            backend_dir.display()
        ));
    }

    let display_addr = format!("{}:{}", config.listen_addr, config.listen_port);
    println!(
        "▶ Starting generated Rust HTTP service on {display_addr} (profile: {})",
        config.profile.as_str()
    );
    println!("  cargo run --manifest-path {}", cargo_toml.display());

    // PLAN-816 T-06：cargo run 子进程经 owned 执行设施承载（契约基线 §3
    // serve_rust 行——"ready/健康轮询归 T-06，用设施重写"）。树归属=
    // ProcessTree（cargo run 派生的真实服务二进制一并 owned；Job 挂起
    // 启动保证早派生后代入树）；stdout 管道化由 reader 线程逐行转发并
    // 观测子进程自打的 `AUTO_SERVICE_READY`（不假 ready：未见行=未就绪，
    // spec §5"旧实例/生成失败不得 ready"）；stderr 继承（cargo 构建进度
    // 语义保持）。配置注入：AUTO_HTTP_SERVICE_JSON（决策 §3.1：env 注入）；
    // cwd=workspace 落点（cargo 从 CWD 发现 .cargo/config.toml——shared
    // target-dir+MSVC 栈 rustflags，冷编指纹才一致）。
    let service_json = config.to_json();
    let mut spec = auto_tool_execution::ExecutionSpec::new([
        "cargo".into(),
        "run".into(),
        "--release".into(),
        "--manifest-path".into(),
        cargo_toml.clone().into_os_string(),
    ]);
    spec.cwd = Some(ws_dir.clone());
    spec.env = vec![("AUTO_HTTP_SERVICE_JSON".into(), service_json.into())];
    for (key, value) in build_context.cargo_env() {
        spec.env.push((key.into(), value.into()));
    }
    spec.stdin = auto_tool_execution::StdioPolicy::Null;
    spec.stdout = auto_tool_execution::StdioPolicy::Piped;
    spec.stderr = auto_tool_execution::StdioPolicy::Inherit;
    spec.tree_scope = auto_tool_execution::TreeScope::ProcessTree;
    let mut proc = auto_tool_execution::spawn(&spec)
        .map_err(|e| format!("failed to spawn backend cargo run: {e}"))?;
    println!(
        "  ✓ backend spawning (PID: {}); Ctrl+C stops the service",
        proc.id()
    );

    // READY 观测：reader 线程逐行转发 stdout 并捕获 AUTO_SERVICE_READY
    // 行（生成服务 bind+初始化后自打，api_gen——就绪只认真实上报；
    // 窗口尽未见行=具名失败并取消树，不把"还在构建"误报为就绪也不无限
    // 等）。窗口默认 600s：rust 轨的窗口覆盖 cargo build（冷 release 构建
    // 是真实常态，与 vm 轨 120s 纯启动窗不同源）；AUTO_SERVICE_READY_
    // TIMEOUT_SECS 显式覆盖（预算正反控）。
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
    let stdout = proc
        .take_stdout()
        .ok_or_else(|| "backend stdout was not piped".to_string())?;
    // F-816-B-R1-02：reader 的首个 I/O 错误具名留存（旧 lines() Err=
    // 静默 break，被当普通 EOF 吞掉）。
    let reader_err: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let err_slot = reader_err.clone();
    let reader = std::thread::Builder::new()
        .name("service-rust-stdout".into())
        .spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);
            let mut ready_sent = false;
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        println!("{line}");
                        if !ready_sent && line.starts_with("AUTO_SERVICE_READY ") {
                            ready_sent = true;
                            let _ = ready_tx.send(line);
                        }
                    }
                    Err(e) => {
                        *err_slot.lock().unwrap() = Some(format!("backend stdout reader: {e}"));
                        break;
                    }
                }
            }
        })
        .map_err(|e| format!("failed to spawn backend stdout reader: {e}"))?;

    // F-816-B-R1-02：reader 有界回收——调用前**先终止 owned 树**（根已
    // 退出时，仍持 stdout 写端的后代在被终止前不会给 EOF；A 阶段"根
    // 退出≠管道 EOF"的教训在新 service adapter 保持）。宽限尽=具名失败，
    // panic=具名失败，读错=具名传播；不无限 join、不吞 panic。
    let reclaim_reader =
        |reader: std::thread::JoinHandle<()>, grace: std::time::Duration| -> Result<(), String> {
            let deadline = std::time::Instant::now() + grace;
            while !reader.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if !reader.is_finished() {
                return Err(format!(
                "backend stdout reader still running after {grace:?} (pipe holder in owned tree?)"
            ));
            }
            match reader.join() {
                Ok(()) => {}
                Err(_) => return Err("backend stdout reader panicked".to_string()),
            }
            if let Some(e) = reader_err.lock().unwrap().take() {
                return Err(e);
            }
            Ok(())
        };
    let tree_grace = std::time::Duration::from_secs(5);
    // F-816-B-R2-02 残项：失败出口不覆盖确已发生的 reader/回收错误——
    // 后端退出信息保持优先呈现，读错/回收错**追加**具名（不是吞掉、也不
    // 把读错解释成 bind/startup 失败）。
    let compose_failure = |lead: String,
                           cancel: &Result<(), String>,
                           reader_reclaim: &Result<(), String>|
     -> String {
        let mut msg = lead;
        if let Err(e) = cancel {
            msg.push_str(&format!("; {e}"));
        }
        if let Err(e) = reader_reclaim {
            msg.push_str(&format!("; {e}"));
        }
        msg
    };

    // F-816-B-R1-07（budget 语义明确）：单位=秒；非法值不静默采用默认
    // ——stderr 具名诊断后按默认 600s 继续（显式拒绝会让拼写错误把长
    // 冷构建误杀；诊断让人可见）。
    let budget_raw = std::env::var("AUTO_SERVICE_READY_TIMEOUT_SECS").ok();
    let budget_secs = budget_raw
        .as_deref()
        .and_then(|v| v.trim().parse::<u64>().ok());
    if budget_raw.is_some() && budget_secs.is_none() {
        eprintln!(
            "warning: AUTO_SERVICE_READY_TIMEOUT_SECS={:?} is not a non-negative integer of seconds; using default 600s",
            budget_raw.unwrap_or_default()
        );
    }
    let ready_window = std::time::Duration::from_secs(budget_secs.unwrap_or(600));
    let ready = ready_rx.recv_timeout(ready_window);
    match ready {
        Ok(line) => println!("  ✓ Rust HTTP service ready ({line})"),
        Err(_) => {
            // 未在窗口内就绪：先取消 owned 树（终止仍持 stdout 的后代→
            // EOF 确定性到达），再有界回收 reader（F-816-B-R1-02），随后
            // 按失败源报告——不假 ready。
            let cancel = proc
                .cancel_within(tree_grace)
                .map(|_| ())
                .map_err(|e| format!("cancel after ready window: {e}"));
            let reader_reclaim = reclaim_reader(reader, tree_grace);
            let waited = proc.wait();
            return match (&waited, &cancel, &reader_reclaim) {
                (Ok(status), _, _) if !status.success() => Err(compose_failure(
                    format!(
                        "backend exited with {} before ready (bind conflict or startup failure)",
                        status.code().unwrap_or(1)
                    ),
                    &cancel,
                    &reader_reclaim,
                )),
                (Ok(_), Err(_), _) | (Ok(_), _, Err(_)) => Err(compose_failure(
                    format!(
                        "Rust HTTP service did not report AUTO_SERVICE_READY within {ready_window:?}"
                    ),
                    &cancel,
                    &reader_reclaim,
                )),
                (Ok(_), Ok(()), Ok(())) => Err(format!(
                    "Rust HTTP service did not report AUTO_SERVICE_READY within {ready_window:?}"
                )),
                (Err(_), _, _) => Err(compose_failure(
                    "backend wait failed after ready window".to_string(),
                    &cancel,
                    &reader_reclaim,
                )),
            };
        }
    }

    // Ctrl+C（spec §7 受控关闭）：子进程与 CLI 同控制台、自行收到事件并
    // 优雅排空（drain 默认 10s）；本进程先等待自然退出（有界排空窗），
    // 超窗或有界等待失败则 cancel owned 树（terminate+收尸）——"rust 子
    // 进程有界排空后 parent 退出；端口与自己创建的子进程零残留"。
    let terminate_installed = auto_tool_execution::terminate::install();
    let drain_window = std::time::Duration::from_secs(15);
    // F-816-B-R2-01：Unix 下 ProcessTree 子进程在新进程组——终端 Ctrl+C
    // 不直达；父进程观察到终止意图后**先向 owned 组转发 SIGINT**
    //（子进程汇入自身优雅关闭，spec §7/plan736 关闭 watch），再等排空，
    // 超窗才 cancel 兜底。Windows 控制台事件本就独立送达，interrupt 为
    // no-op。
    let mut interrupt_sent = false;
    let status = loop {
        if terminate_installed && auto_tool_execution::terminate::requested() {
            if !interrupt_sent {
                let _ = proc.interrupt();
                interrupt_sent = true;
            }
            match proc.wait_deadline(drain_window) {
                Ok(Some(status)) => break status,
                _ => {
                    let status = proc
                        .cancel_within(std::time::Duration::from_secs(5))
                        .map_err(|e| format!("cancel after drain window: {e}"))?;
                    break status;
                }
            }
        }
        match proc.wait_deadline(std::time::Duration::from_millis(200)) {
            Ok(Some(status)) => break status,
            Ok(None) => continue,
            Err(e) => return Err(format!("backend wait failed: {e}")),
        }
    };
    // F-816-B-R1-02：根已退出≠树已空——先终止 owned 树内仍持 stdout
    // 写端的后代（EOF 确定性到达），再有界回收 reader（panic/读错具名）。
    let tree_reclaim = proc
        .cancel_within(tree_grace)
        .map(|_| ())
        .map_err(|e| format!("cancel after backend exit: {e}"));
    let reader_reclaim = reclaim_reader(reader, tree_grace);
    if status.success() {
        tree_reclaim?;
        reader_reclaim?;
        println!("  ✓ Rust HTTP service stopped cleanly");
        return Ok(0);
    }
    if terminate_installed && auto_tool_execution::terminate::requested() {
        // 操作员终止路径的树取消：退出码来自我方终止，不计为服务失败。
        tree_reclaim?;
        reader_reclaim?;
        println!("  ✓ Rust HTTP service stopped (terminate)");
        return Ok(0);
    }
    // 失败路径：后端退出码优先呈现，确已发生的回收/读错追加具名
    //（F-816-B-R2-02 残项：原实现在此丢弃 tree/reader 回收错误）。
    let code = status.code().unwrap_or(1);
    Err(compose_failure(
        format!("backend exited with {code} — bind conflict or startup failure (see output above)"),
        &tree_reclaim,
        &reader_reclaim,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_config_none_is_development_loopback() {
        let c = resolve_config(None, None).unwrap();
        assert_eq!(
            c.profile,
            auto_lang_core::http_service_config::ServiceProfile::Development
        );
        assert_eq!(c.listen_port, 8080);
    }

    #[test]
    fn resolve_config_cli_port_overrides() {
        let c = resolve_config(None, Some(9123)).unwrap();
        assert_eq!(c.listen_port, 9123);
    }

    #[test]
    fn resolve_config_bad_file_errors() {
        let e = resolve_config(
            Some(&HttpConfigSource::File(PathBuf::from(
                "Z:/definitely/missing/service.json",
            ))),
            None,
        )
        .unwrap_err();
        assert!(e.contains("failed to open"), "{e}");
    }

    #[test]
    fn resolve_config_inline_bad_json_errors_named() {
        let e = resolve_config(
            Some(&HttpConfigSource::Inline(
                r#"{"profile":"development","limits":{"max_connections":0}}"#.into(),
            )),
            None,
        )
        .unwrap_err();
        assert!(e.contains("max_connections"), "{e}");
    }

    #[test]
    fn serve_project_rejects_unknown_server() {
        let c = resolve_config(None, None).unwrap();
        let err = serve_project(Path::new("."), "vue", c).unwrap_err();
        assert!(err.contains("unknown --server"), "{err}");
    }
}
