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
    serve_project_with_api(project_dir, server, config, None)
}

/// PLAN-983 C0：带**显式 AV 源路径**的服务入口（`auto service --server av --api <path>`）。
///
/// 只影响 AV 轨；其它轨忽略该参数。`--server av` = auto-web AV 服务宿主
/// （983/AVSERVICE）；`--server av-direct` = pre-C0 的 man 自带 TCP 宿主（显式
/// 保留给其文本入口/注入常量面，不是 fallback）。
pub fn serve_project_with_api(
    project_dir: &Path,
    server: &str,
    config: HttpServiceConfig,
    api: Option<&Path>,
) -> Result<i32, String> {
    match server {
        "vm" => serve_vm(project_dir, config),
        "rust" => serve_rust(project_dir, config),
        // PLAN-983 C0（用户裁定）：`av` 轨=**auto-web AV 服务宿主**，同一份
        // `#[api]` AV 源经文件通道引擎执行；`--api` 可显式指定源。
        "av" | "ace" => serve_av(project_dir, config, api),
        // pre-C0 的 man 自带 TCP 宿主（文本入口/注入常量面），显式命名保留。
        "av-direct" => serve_av_direct(project_dir, config),
        other => Err(format!(
            "unknown --server `{other}` (expected `vm`, `rust`, `av` or `av-direct`)"
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
fn serve_av_direct(project_dir: &Path, config: HttpServiceConfig) -> Result<i32, String> {
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

    // 路由模式：默认（未设 `AUTO_AV_ROUTES`）走固定入口 `AUTO_AV_ENTRY`（默认
    // `d_main`）；`AUTO_AV_ROUTES=derive` 时按 **(method, path) ⇒ def 名** 派生——
    // `GET /hello` ⇒ `d_hello`，`POST /hello` ⇒ `d_hello_post`，`GET /` ⇒ `d_main`。
    // 派生不到 def 时**不是**静默回落到 `d_main`，而是 404 `ace.service/no-route`。
    let fixed_entry = std::env::var("AUTO_AV_ENTRY").unwrap_or_else(|_| "d_main".to_string());
    let derive_routes = std::env::var("AUTO_AV_ROUTES")
        .map(|v| matches!(v.trim(), "derive" | "1" | "true" | "yes"))
        .unwrap_or(false);
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
        "▶ Starting AV HTTP service on {bound} (entry {}, routes {}, profile {})",
        if derive_routes {
            "<derived>"
        } else {
            fixed_entry.as_str()
        },
        if derive_routes { "derive" } else { "fixed" },
        config.profile.as_str()
    );

    // 并发：**每连接一个线程**（服务器本身仍是单 accept 循环）；计数用原子量共享。
    let served = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let av_dispatches = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let declared = std::sync::Arc::new(declared_fns(&source));
    // PLAN-982（983 文本面）：`AUTO_AV_TEXT=1` 时走**受检文本入口**——查询文本
    // 作为运行时数据交给 handler 的借用 `str` 形参（同一编译产物、逐请求数据）。
    let text_entry = std::env::var("AUTO_AV_TEXT")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    let max_body = std::env::var("AUTO_SERVICE_MAX_BODY")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(AV_MAX_BODY);
    let max_header = std::env::var("AUTO_SERVICE_MAX_HEADER")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(AV_MAX_HEADER);
    let max_line = std::env::var("AUTO_SERVICE_MAX_REQUEST_LINE")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(AV_MAX_REQUEST_LINE);
    let max_conn = std::env::var("AUTO_SERVICE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(AV_MAX_CONNECTIONS);
    let active = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // 非阻塞 accept：这样"已服务的请求数达到上限"能在**没有新连接**时被观察到，
    // 进程才能按上限自然关停（否则会一直阻塞在 accept 上）。
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("ace.service/accept: {e}"))?;
    let mut workers = Vec::new();
    loop {
        if max_requests.is_some_and(|max| served.load(std::sync::atomic::Ordering::SeqCst) >= max) {
            break;
        }
        let (stream, _peer) = match listener.accept() {
            Ok(pair) => {
                let _ = pair.0.set_nonblocking(false);
                pair
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            Err(e) => return Err(format!("ace.service/accept: {e}")),
        };
        if max_requests.is_some_and(|max| served.load(std::sync::atomic::Ordering::SeqCst) >= max) {
            // 超过上限后不再接新连接（已接的连接照常服务完）。
            drop(stream);
            break;
        }
        // 连接上限：超限时**不派发**，直接 503 并关闭连接。
        if active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= max_conn {
            active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            let body = "ace.service/overloaded: the connection cap is reached\n";
            let response = format!(
                "HTTP/1.1 503 Service Unavailable\r\ncontent-type: text/plain; charset=utf-8\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut stream = stream;
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
            continue;
        }
        let source = source.clone();
        let fixed_entry = fixed_entry.clone();
        let served = std::sync::Arc::clone(&served);
        let av_dispatches = std::sync::Arc::clone(&av_dispatches);
        let active_worker = std::sync::Arc::clone(&active);
        let declared_worker = std::sync::Arc::clone(&declared);
        workers.push(std::thread::spawn(move || {
            serve_connection(
                stream,
                &source,
                &fixed_entry,
                derive_routes,
                wall_ms,
                max_requests,
                max_body,
                max_header,
                max_line,
                text_entry,
                &declared_worker,
                &served,
                &av_dispatches,
            );
            active_worker.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }));
    }
    drop(listener);
    for worker in workers {
        let _ = worker.join();
    }

    let served = served.load(std::sync::atomic::Ordering::SeqCst);
    let av_dispatches = av_dispatches.load(std::sync::atomic::Ordering::SeqCst);

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

/// PLAN-983 C0 CLI 切片：`--server av` 的**auto-web 宿主**。
///
/// 形态：`auto_web::http_server::serve_av` + `av_service::AvServiceEngine`
/// （983/AVSERVICE 文件通道引擎）——真实 axum/hyper transport、配置/排水/
/// 控制面（`/__auto/shutdown`）与 legacy serve 同源；AV 源按 `#[api]` 抽取
/// 路由、**编译一次驻留**，每请求 `run_platform`（grants 可配），响应字节由
/// AV 程序产出。
///
/// `--api <path>` 显式指定 AV 源（默认仍发现 `src/back/api.at`）。无源/无路由/
/// 编译失败/坏配置 = 具名失败，**绝不回退** `vm`/`av-direct`。
///
/// pre-C0 的 man 自带 TCP 宿主保留为显式 `--server av-direct`（其文本入口与
/// 注入常量面由 `plan982_av_http_service` 钉住），不是本轨的 fallback。
fn serve_av(
    project_dir: &Path,
    config: HttpServiceConfig,
    api_override: Option<&Path>,
) -> Result<i32, String> {
    let api_path = match api_override {
        Some(path) => path.to_path_buf(),
        None => auto_lang_core::config::resolve_back_api(project_dir).ok_or_else(|| {
            format!(
                "ace.service/no-api: no back API found under {} (expected src/back/api.at); \
                 `auto service --server av` never falls back to the legacy VM track",
                project_dir.display()
            )
        })?,
    };
    let source = std::fs::read_to_string(&api_path)
        .map_err(|e| format!("ace.service/api-unreadable: {}: {e}", api_path.display()))?;
    if source.trim().is_empty() {
        return Err(format!(
            "ace.service/api-empty: {} is empty",
            api_path.display()
        ));
    }

    // 文件通道 scratch：ASCII（w16 staging 约束）；服务结束删除。
    let scratch = std::env::temp_dir().join(format!("auto-av-service-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("ace.service/scratch: {e}"))?;
    let scratch_text = scratch
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    if !scratch_text.is_ascii() {
        return Err(
            "ace.service/scratch-ascii: the AV file channel requires an ASCII scratch path".into(),
        );
    }
    let grants: Vec<String> = std::env::var("AUTO_AV_GRANTS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|grant| grant.trim().to_string())
                .filter(|grant| !grant.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|grants: &Vec<String>| !grants.is_empty())
        .unwrap_or_else(|| {
            vec![
                "platform.file".to_string(),
                "platform.file.path".to_string(),
                "platform.mem".to_string(),
                "platform.env".to_string(),
            ]
        });
    let engine = auto_web::av_service::AvServiceEngine::new_with_grants(&source, &scratch, grants)
        .map_err(|e| format!("ace.service/source: {e}"))?;
    let receipt = engine.receipt();
    let route_count = engine.routes().len();
    let addr = format!("{}:{}", config.listen_addr, config.listen_port);
    println!(
        "▶ Starting AV HTTP service on {addr} (engine auto-web, {route_count} route(s), profile {})",
        config.profile.as_str()
    );

    // 真实的 `HttpServiceConfig` → transport 映射（auto-web 公开单一来源：
    // 预算/超时/排水取自配置；策略面 ServiceRuntime 一并装配——982 `--stack
    // web` 的同一入口）。
    let transport = auto_web::http_transport::TransportConfig::from_service_config_with_policy(
        std::sync::Arc::new(config.clone()),
    );
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    if let Some(service) = &transport.service {
        if !service.bind_shutdown(shutdown_tx.clone()) {
            return Err(
                "ace.service/shutdown-bound: the service runtime shutdown channel is already bound"
                    .into(),
            );
        }
    }
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .name("auto-av-http-service".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("ace.service/runtime: {e}"))?;
            let local = tokio::task::LocalSet::new();
            // 控制面（loopback `/__auto/shutdown`）与 Ctrl+C 驱动同一关停通道
            // （legacy `serve_async_with` 同款接线；`serve_av` 是公共入口，自身
            // 不安装信号面）。
            auto_web::http_server::bind_control_plane_shutdown(shutdown_tx.clone());
            let ctrl_tx = shutdown_tx.clone();
            local.spawn_local(async move {
                let _ = tokio::signal::ctrl_c().await;
                eprintln!("[HTTP] Ctrl+C — shutting down gracefully");
                let _ = ctrl_tx.send(true);
            });
            local
                .block_on(
                    &rt,
                    auto_web::http_server::serve_av(engine, &addr, transport, shutdown_rx),
                )
                .map_err(|e| format!("ace.service/serve: {e}"))
        })
        .map_err(|e| format!("ace.service/thread: {e}"))?;

    // bind 判定同 serve_vm（port=0 → 内核分配）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut bound = None;
    while std::time::Instant::now() < deadline {
        if let Some(addr) = auto_web::http_server::bound_addr_snapshot() {
            bound = Some(addr);
            break;
        }
        if handle.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    match bound {
        Some(addr) => {
            println!("  ✓ AV HTTP service ready (bound {addr})");
            println!(
                "  AUTO_SERVICE_READY {{\"bound\":\"{addr}\",\"profile\":\"{}\",\"config_hash\":\"{:016x}\"}}",
                config.profile.as_str(),
                config.effective_config_hash()
            );
        }
        None => {
            let join = handle.join();
            let fatal = auto_web::http_server::take_serve_fatal_error();
            if let Some(f) = fatal {
                return Err(f);
            }
            if let Ok(Err(e)) = join {
                return Err(e);
            }
            return Err("AV HTTP service did not become ready within 120s".to_string());
        }
    }

    let join = handle.join();
    println!(
        "✓ AV service receipt: served={} av_dispatches={} legacy_dispatches={} \
         combined_entry_dispatches={} same_sdk=true",
        receipt.av_dispatches(),
        receipt.av_dispatches(),
        receipt.legacy_dispatches(),
        receipt.combined_entry_dispatches()
    );
    let _ = auto_web::http_server::take_serve_fatal_error();
    let _ = std::fs::remove_dir_all(&scratch);
    match join {
        Ok(Ok(())) => Ok(0),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("AV service thread failed".to_string()),
    }
}

/// 服务端请求体上限的默认值（1 MiB）；`AUTO_SERVICE_MAX_BODY` 可覆盖。
const AV_MAX_BODY: usize = 1024 * 1024;
/// 请求头总量上限（8 KiB）；`AUTO_SERVICE_MAX_HEADER` 可覆盖。
const AV_MAX_HEADER: usize = 8 * 1024;
/// 并发连接上限（64）；`AUTO_SERVICE_MAX_CONNECTIONS` 可覆盖。
const AV_MAX_CONNECTIONS: usize = 64;
/// 请求行上限（8 KiB）；`AUTO_SERVICE_MAX_REQUEST_LINE` 可覆盖。
const AV_MAX_REQUEST_LINE: usize = 8 * 1024;

/// 一条连接的完整服务过程（**每连接一个线程**）：keep-alive + 逐请求派发 AV。
#[allow(clippy::too_many_arguments)]
fn serve_connection(
    stream: std::net::TcpStream,
    source: &str,
    fixed_entry: &str,
    derive_routes: bool,
    wall_ms: Option<u64>,
    max_requests: Option<u64>,
    max_body: usize,
    max_header: usize,
    max_line: usize,
    text_entry: bool,
    declared: &std::collections::BTreeSet<String>,
    served: &std::sync::atomic::AtomicU64,
    av_dispatches: &std::sync::atomic::AtomicU64,
) {
    use std::io::Write;
    use std::sync::atomic::Ordering;

    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(60)));
    let mut reader = std::io::BufReader::new(stream);
    let mut stream = None;
    loop {
        let request = match read_request(&mut reader, max_body, max_header, max_line) {
            Ok(Some(request)) => request,
            // 就绪探针（只连不发）或对端提前关闭：本连接结束。
            Ok(None) => break,
            Err(_) => break,
        };
        // 超长请求行：直接 414（不派发）。
        if request.request_line_too_long {
            let body = "ace.service/uri-too-long: the request line exceeds the cap\n";
            let response = format!(
                "HTTP/1.1 414 URI Too Long\r\ncontent-type: text/plain; charset=utf-8\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut io = request.stream;
            let _ = io.write_all(response.as_bytes());
            let _ = io.flush();
            served.fetch_add(1, Ordering::SeqCst);
            break;
        }
        // 超限头：直接 431（不派发）。
        if request.headers_too_large {
            let body = "ace.service/headers-too-large: the request headers exceed the cap\n";
            let response = format!(
                "HTTP/1.1 431 Request Header Fields Too Large\r\n\
                 content-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            let mut io = request.stream;
            let _ = io.write_all(response.as_bytes());
            let _ = io.flush();
            served.fetch_add(1, Ordering::SeqCst);
            break;
        }
        // 超限体：直接 413，不派发（也绝不读体）。
        if request.oversize {
            let body = "ace.service/body-too-large: content-length exceeds the configured cap\n";
            let response = format!(
                "HTTP/1.1 413 Payload Too Large\r\ncontent-type: text/plain; charset=utf-8\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let io = request.stream;
            let mut io = io;
            let _ = io.write_all(response.as_bytes());
            let _ = io.flush();
            served.fetch_add(1, Ordering::SeqCst);
            break;
        }
        let method = request.method.clone();
        let path = request.path.clone();
        let program = inject_request_context(&method, &path, source, &request);
        let entry = if derive_routes {
            entry_for(&method, &path)
        } else {
            fixed_entry.to_string()
        };
        // HTTP 语义：路径存在但方法不对 ⇒ **405 + allow**（不派发）；方法与路径都没有
        // 对应的 def 才走 404（由 lower 的 `ace.entry/missing` 给出）。
        if derive_routes && !declared.contains(&entry) {
            let allowed = allowed_methods(declared, &path);
            if !allowed.is_empty() {
                let body = format!(
                    "ace.service/method-not-allowed: allow {}\n",
                    allowed.join(", ")
                );
                let response = format!(
                    "HTTP/1.1 405 Method Not Allowed\r\ncontent-type: text/plain; charset=utf-8\r\n\
                     allow: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    allowed.join(", "),
                    body.len()
                );
                let mut io = request.stream;
                let _ = io.write_all(response.as_bytes());
                let _ = io.flush();
                served.fetch_add(1, Ordering::SeqCst);
                break;
            }
        }
        let (status, body, dispatched) = if text_entry {
            av_handle_text(
                source,
                &entry,
                wall_ms,
                &path,
                &request.query,
                &request.body,
            )
        } else {
            av_handle(&program, &entry, wall_ms, &path)
        };
        if dispatched {
            av_dispatches.fetch_add(1, Ordering::SeqCst);
        }
        let done = served.fetch_add(1, Ordering::SeqCst) + 1;
        let keep_alive = request.keep_alive && !max_requests.is_some_and(|max| done >= max);
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\n\
             content-length: {}\r\nconnection: {}\r\n\r\n{body}",
            body.len(),
            if keep_alive { "keep-alive" } else { "close" }
        );
        let io = stream.get_or_insert(request.stream);
        let _ = io.write_all(response.as_bytes());
        let _ = io.flush();
        if !keep_alive {
            break;
        }
    }
}

/// 一个已解析的请求（最小 HTTP/1.1 面：请求行 + 头 + 按 `content-length` 的体）。
struct AvRequest {
    method: String,
    /// 请求行里的原始目标（含查询串）。
    target: String,
    path: String,
    query: String,
    body: String,
    keep_alive: bool,
    /// `content-length` 超过服务端上限：不读体、不派发，直接 413。
    oversize: bool,
    /// 请求头总量超过上限：直接 431。
    headers_too_large: bool,
    /// 请求行超过上限：直接 414。
    request_line_too_long: bool,
    stream: std::net::TcpStream,
}

/// 从连接里读一个完整请求；`Ok(None)` 表示对端没说话就关了（就绪探针）。
fn read_request(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    max_body: usize,
    max_header: usize,
    max_line: usize,
) -> Result<Option<AvRequest>, String> {
    use std::io::BufRead;

    let mut line = String::new();
    if reader
        .read_line(&mut line)
        .map_err(|e| format!("ace.service/read: {e}"))?
        == 0
    {
        return Ok(None);
    }
    if line.len() > max_line {
        let stream = reader.get_ref().try_clone().map_err(|e| e.to_string())?;
        return Ok(Some(AvRequest {
            method: String::new(),
            target: String::new(),
            path: String::new(),
            query: String::new(),
            body: String::new(),
            keep_alive: false,
            oversize: false,
            headers_too_large: false,
            request_line_too_long: true,
            stream,
        }));
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (target.clone(), String::new()),
    };

    let mut content_length = 0usize;
    let mut keep_alive = true;
    let mut header_bytes = 0usize;
    loop {
        let mut header = String::new();
        if reader
            .read_line(&mut header)
            .map_err(|e| format!("ace.service/read: {e}"))?
            == 0
        {
            break;
        }
        header_bytes += header.len();
        let trimmed = header.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => content_length = value.parse().unwrap_or(0),
                "connection" => {
                    keep_alive = !value.eq_ignore_ascii_case("close");
                }
                _ => {}
            }
        }
    }

    let headers_too_large = header_bytes > max_header;
    let oversize = !headers_too_large && content_length > max_body;
    if headers_too_large {
        let stream = reader.get_ref().try_clone().map_err(|e| e.to_string())?;
        return Ok(Some(AvRequest {
            method,
            target,
            path,
            query,
            body: String::new(),
            keep_alive: false,
            oversize: false,
            headers_too_large: true,
            request_line_too_long: false,
            stream,
        }));
    }
    let mut body = String::new();
    if oversize {
        // 超限：不读体（避免被一个声称超大的 content-length 拖住内存）。
        let stream = reader.get_ref().try_clone().map_err(|e| e.to_string())?;
        return Ok(Some(AvRequest {
            method,
            target,
            path,
            query,
            body: String::new(),
            keep_alive: false,
            oversize: true,
            headers_too_large: false,
            request_line_too_long: false,
            stream,
        }));
    }
    if content_length > 0 {
        let mut rest = vec![0u8; content_length];
        reader
            .read_exact(&mut rest)
            .map_err(|e| format!("ace.service/read: {e}"))?;
        body = String::from_utf8_lossy(&rest).to_string();
    }
    let stream = reader.get_ref().try_clone().map_err(|e| e.to_string())?;
    Ok(Some(AvRequest {
        method,
        target,
        path,
        query,
        body,
        keep_alive,
        oversize: false,
        headers_too_large: false,
        request_line_too_long: false,
        stream,
    }))
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
fn inject_request_context(method: &str, path: &str, source: &str, request: &AvRequest) -> String {
    let mut injected = format!(
        "fn __av_request_path_len() int {{ return {} }}\n\
         fn __av_request_method_len() int {{ return {} }}\n\
         fn __av_request_query_len() int {{ return {} }}\n\
         fn __av_request_body_len() int {{ return {} }}\n\
         fn __av_request_target_len() int {{ return {} }}\n",
        path.len(),
        method.len(),
        request.query.len(),
        request.body.len(),
        request.target.len()
    );
    // 查询参数按**键**注入：`?n=5&m=7` ⇒ `__av_query_n()`=5、`__av_query_m()`=7；
    // 每个键另有 `__av_query_<key>_present()`=1。
    // **多值**：`?t=a&t=b` ⇒ `__av_query_t_len()`=2、`__av_query_t_0()`/`__av_query_t_1()`
    // （仅当值是十进制整数时给值函数）；单值函数取**第一个**值。
    // 值不是十进制整数时只注入 `_present()`——**原文面还没开**（实测：AV 源面
    // 连 `str` 类型都不是该 profile 的一部分）。
    for (key, values) in query_groups(&request.query) {
        let key = sanitize_key(&key);
        if key.is_empty() {
            continue;
        }
        injected.push_str(&format!(
            "fn __av_query_{key}_present() int {{ return 1 }}\n\
             fn __av_query_{key}_len() int {{ return {} }}\n",
            values.len()
        ));
        if let Some(first) = values.first() {
            if let Ok(int) = first.parse::<i64>() {
                injected.push_str(&format!("fn __av_query_{key}() int {{ return {int} }}\n"));
            }
        }
        for (index, value) in values.iter().enumerate() {
            if let Ok(int) = value.parse::<i64>() {
                injected.push_str(&format!(
                    "fn __av_query_{key}_{index}() int {{ return {int} }}\n"
                ));
            }
        }
    }
    // 请求体的**数值字段**按名注入：`{"n":5,"m":7}` ⇒ `__av_body_n()`=5、`__av_body_m()`=7
    // （另有 `_present()`）。这是**最小扫描**：只认扁平对象里的十进制/负整数
    // 字面量，不解析嵌套、字符串、布尔与小数——文本面仍等语言决策。
    for (key, value) in body_number_fields(&request.body) {
        let key = sanitize_key(&key);
        if key.is_empty() {
            continue;
        }
        injected.push_str(&format!(
            "fn __av_body_{key}_present() int {{ return 1 }}\n\
             fn __av_body_{key}() int {{ return {value} }}\n"
        ));
    }
    injected.push_str(source);
    injected
}

/// 从请求体里扫出**数值字段**：`{"n":5,"m":-7}` ⇒ `[("n",5),("m",-7)]`。
///
/// 只处理扁平对象：`"key"` `:` 十进制或负十进制（可带空白），值后面必须是
/// `,` / `}` / 空白 / 结束。其余形状一律跳过（不是解析器，也不会报错）。
fn body_number_fields(body: &str) -> Vec<(String, i64)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        // 键
        let key_start = i + 1;
        let mut j = key_start;
        while j < bytes.len() && bytes[j] != b'"' {
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }
        let key = &body[key_start..j];
        let mut k = j + 1;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if k >= bytes.len() || bytes[k] != b':' {
            i = j + 1;
            continue;
        }
        k += 1;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        let value_start = k;
        if k < bytes.len() && bytes[k] == b'-' {
            k += 1;
        }
        let digits_start = k;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k > digits_start {
            let raw = &body[value_start..k];
            if let Ok(value) = raw.parse::<i64>() {
                out.push((key.to_string(), value));
            }
            i = k;
        } else {
            i = j + 1;
        }
    }
    out
}

/// `a=1&b=2&a=3` ⇒ `[("a", ["1","3"]), ("b", ["2"])]`（百分号解码，保持首次出现顺序）。
fn query_groups(query: &str) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (percent_decode(key), percent_decode(value)),
            None => (percent_decode(pair), String::new()),
        };
        match groups.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, values)) => values.push(value),
            None => groups.push((key, vec![value])),
        }
    }
    groups
}

/// `%XX` 与 `+` 的最小解码（非法转义原样保留）。
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 查询键 ⇒ 合法标识符片段（非字母数字折 `_`）。
fn sanitize_key(key: &str) -> String {
    key.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
/// 路由派生：`(method, path)` ⇒ def 名（`AUTO_AV_ROUTES=derive` 时启用）。
///
/// - `GET /` ⇒ `d_main`（根入口沿用惯例名）；
/// - `GET /hello` ⇒ `d_hello`；`GET /a/b` ⇒ `d_a_b`；
/// - 非 GET 追加方法后缀：`POST /hello` ⇒ `d_hello_post`。
///
/// 派生是**确定**的：路径里非字母数字一律折成 `_`，大小写保留；查询串不参与。
fn entry_for(method: &str, path: &str) -> String {
    let method = method.trim().to_ascii_uppercase();
    let trimmed = path.split('?').next().unwrap_or("").trim_matches('/');
    if trimmed.is_empty() {
        return if method == "GET" {
            "d_main".to_string()
        } else {
            format!("d_{}", method.to_ascii_lowercase())
        };
    }
    let mut name = String::from("d_");
    for ch in trimmed.chars() {
        name.push(if ch.is_ascii_alphanumeric() { ch } else { '_' });
    }
    if method != "GET" {
        name.push('_');
        name.push_str(&method.to_ascii_lowercase());
    }
    name
}

/// 源码里的 `fn <name>` 声明集合（只做标识符扫描；用于 **405 vs 404** 的判定）。
fn declared_fns(source: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if &bytes[i..i + 2] == b"fn" && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 2;
            if j < bytes.len() && bytes[j].is_ascii_whitespace() {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j > start {
                    // 源码函数名 ⇒ **def id**（语言侧惯例：`fn hello` ⇒ `d_hello`）。
                    out.insert(format!("d_{}", &source[start..j]));
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 同一个 path 上**已声明**的方法（用于 405 的 `allow` 头）。
fn allowed_methods(declared: &std::collections::BTreeSet<String>, path: &str) -> Vec<&'static str> {
    ["GET", "POST", "PUT", "DELETE", "PATCH"]
        .into_iter()
        .filter(|method| declared.contains(&entry_for(method, path)))
        .collect()
}

/// PLAN-982（983 文本面）：**受检文本入口**的一次请求——查询文本作为运行时数据
/// 交给 handler 的借用 `str` 形参（`run_ace_text` ⇒ Text profile ⇒
/// `execute_entry_text`）。返回 `(状态行, 响应体, 是否真的派发到 AV)`。
fn av_handle_text(
    source: &str,
    entry: &str,
    wall_ms: Option<u64>,
    path: &str,
    query: &str,
    body: &str,
) -> (&'static str, String, bool) {
    use auto_lang_core::{run_ace_text, AceEngineChoice, AceOutcome, AceRequest};
    let wall = wall_ms.map(std::time::Duration::from_millis);
    let request = AceRequest {
        entry_def_id: entry,
        wall,
        ..AceRequest::simple(source)
    };
    // 同一编译产物、逐请求文本数据：`texts[0]` = 查询串，`texts[1]` = 请求体。
    // handler 按签名位置接收：`fn main(q str, body str) int {…}`。
    let texts: [&[u8]; 2] = [query.as_bytes(), body.as_bytes()];
    let report = run_ace_text(AceEngineChoice::AceAV, &request, &texts);
    match &report.outcome {
        AceOutcome::Success { value, steps } => {
            let rendered = render_ace_value(value);
            let body = format!("{rendered}\npath={path} steps={steps} mode=text");
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
        AceOutcome::ResourceDenied { kind, .. } => (
            "504 Gateway Timeout",
            format!("ace.service/budget-exhausted: {kind}\n"),
            true,
        ),
    }
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
        // 派生入口不存在 ⇒ **404**（绝不静默回落到 `d_main`）。
        // 实测：未知入口的具名 code 是 `ace.entry/missing`（lower 阶段给出 `lower.entry`）。
        AceOutcome::Diagnostic { code, .. }
            if code == "ace.entry/missing" || code == "lower.entry" =>
        {
            (
                "404 Not Found",
                "ace.service/no-route: no handler is defined for this (method, path)\n".to_string(),
                true,
            )
        }
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
