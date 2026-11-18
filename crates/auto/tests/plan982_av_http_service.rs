//! PLAN-982（983 服务链）：**第一条真实项目 HTTP 请求**，由 ACE AV 执行。
//!
//! 链路：真实工程夹具（`pac.at` + `src/back/api.at`）→ `auto service --server av`
//! → 真实 `TcpListener` → 每个请求跑一次 `run_ace(AceAV)` → HTTP 响应。
//!
//! 四类证据（每条都是真跑，不是自报）：
//! 1. **成功**：`fn main() int { return 42 }` ⇒ 200，body 含 `42`，收据
//!    `av_dispatches>=1 legacy_dispatches=0`；
//! 2. **错误**：`fn main() int { return missing }` ⇒ 500 + 具名 code；
//! 3. **超时**：`fn main() int { loop { } }` + `AUTO_AV_WALL_MS=100` ⇒ 504 +
//!    `ace.service/timeout`；
//! 4. **关停**：`AUTO_SERVICE_MAX_REQUESTS=1` ⇒ 服务自行关停，之后的连接被拒。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const PAC: &str = "name: \"avsvc\"\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-avsvc-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("src").join("back")).expect("project dirs");
    std::fs::write(dir.join("pac.at"), PAC).expect("pac.at");
    dir
}

/// 先占一个端口再释放，得到一个大概率可用的端口号（服务需要固定端口才能连）。
fn free_port() -> u16 {
    let probe = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);
    port
}

struct Service {
    child: Child,
    port: u16,
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(project: &PathBuf, api: &str, envs: &[(&str, &str)]) -> Service {
    std::fs::write(project.join("src").join("back").join("api.at"), api).expect("api.at");
    let port = free_port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_auto"));
    command
        .args([
            "service",
            "--dir",
            project.to_str().unwrap(),
            "--server",
            "av",
            "--http-config-inline",
            &format!(
                "{{\"profile\":\"development\",\"listen\":{{\"addr\":\"127.0.0.1\",\"port\":{port}}}}}"
            ),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let child = command.spawn().expect("spawn auto service");
    Service { child, port }
}

fn wait_ready(service: &Service) -> std::net::SocketAddr {
    let addr = format!("127.0.0.1:{}", service.port);
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if let Ok(stream) = TcpStream::connect(&addr) {
            drop(stream);
            return addr.parse().expect("addr");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the AV service never accepted a connection on {addr}");
}

fn request(addr: std::net::SocketAddr, path: &str) -> (String, String) {
    request_method(addr, "GET", path)
}

fn request_method(addr: std::net::SocketAddr, method: &str, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .expect("read timeout");
    let request =
        format!("{method} {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut text = String::new();
    stream.read_to_string(&mut text).expect("read response");
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    (
        head.lines().next().unwrap_or("").to_string(),
        body.to_string(),
    )
}

fn finish(service: &mut Service) -> String {
    // 先等进程自己退出（`AUTO_SERVICE_MAX_REQUESTS` 触发关停），超时才强杀。
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if service.child.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = service.child.kill();
    let _ = service.child.wait();

    let mut text = String::new();
    if let Some(mut out) = service.child.stdout.take() {
        let _ = out.read_to_string(&mut text);
    }
    if let Some(mut err) = service.child.stderr.take() {
        let mut buf = String::new();
        let _ = err.read_to_string(&mut buf);
        text.push('\n');
        text.push_str(&buf);
    }
    text
}

#[test]
fn p982_av_service_serves_one_real_request_with_av_dispatch() {
    let project = scratch("ok");
    let mut service = start(
        &project,
        "fn main() int { return 42 }\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr = wait_ready(&service);
    let (status, body) = request(addr, "/hello");
    let receipt = finish(&mut service);

    eprintln!("p982: status={status} body={body:?}");
    eprintln!("p982: receipt={receipt}");
    assert_eq!(
        status, "HTTP/1.1 200 OK",
        "the request is served: {receipt}"
    );
    assert!(
        body.contains("42"),
        "the body carries the AV result: {body}"
    );
    assert!(
        receipt.contains("av_dispatches=1") || receipt.contains("av_dispatches=2"),
        "the AV lane really dispatched: {receipt}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn p982_av_service_reports_handler_errors_by_name() {
    let project = scratch("err");
    let mut service = start(
        &project,
        "fn main() int { return missing_name }\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr = wait_ready(&service);
    let (status, body) = request(addr, "/boom");
    let receipt = finish(&mut service);

    eprintln!("p982: error status={status} body={body:?}");
    assert!(
        status.contains("500"),
        "a failing handler is a 500: {status} / {receipt}"
    );
    assert!(
        body.contains("ace.service/handler-") && body.contains("source."),
        "the error is named with its code: {body}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn p982_av_service_times_out_a_runaway_handler() {
    let project = scratch("slow");
    let mut service = start(
        &project,
        "fn main() int {\n  var i = 0\n  loop { i = i }\n  return i\n}\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "1"),
            ("AUTO_AV_WALL_MS", "150"),
        ],
    );
    let addr = wait_ready(&service);
    let (status, body) = request(addr, "/slow");
    let receipt = finish(&mut service);

    eprintln!("p982: timeout status={status} body={body:?}");
    assert!(
        status.contains("504")
            || body.contains("ace.service/timeout")
            || body.contains("ace.service/budget-exhausted"),
        "a runaway handler is reclaimed as a timeout: {status} / {body} / {receipt}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

#[test]
fn p982_av_service_closes_its_listener_after_shutdown() {
    let project = scratch("stop");
    let mut service = start(
        &project,
        "fn main() int { return 7 }\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr = wait_ready(&service);
    let (status, _) = request(addr, "/once");
    assert_eq!(status, "HTTP/1.1 200 OK", "the single request is served");
    let receipt = finish(&mut service);
    assert!(
        receipt.contains("av_dispatches=1"),
        "one dispatch happened before shutdown: {receipt}"
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let mut refused = false;
    while std::time::Instant::now() < deadline {
        if TcpStream::connect(addr).is_err() {
            refused = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(refused, "the listener is closed after shutdown: {receipt}");
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 150）：**请求上下文真的进了 AV 程序**——每个请求重新合成的源码
/// 带 `__av_request_path_len()` / `__av_request_method_len()`，handler 直接返回它。
/// 同一个进程内两个不同路径 ⇒ 两个不同结果，证明注入是**逐请求**的，不是常量。
#[test]
fn p982_av_service_hands_request_context_to_the_program() {
    let project = scratch("ctx");
    let mut service = start(
        &project,
        "fn main() int { return __av_request_path_len() + __av_request_method_len() }\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "2")],
    );
    let addr = wait_ready(&service);

    // GET (3) + "/hello" (6) = 9；GET + "/x" (2) = 5。
    let (first_status, first_body) = request(addr, "/hello");
    let (second_status, second_body) = request(addr, "/x");
    let receipt = finish(&mut service);

    eprintln!("p982: ctx1={first_status} {first_body:?}");
    eprintln!("p982: ctx2={second_status} {second_body:?}");
    assert_eq!(first_status, "HTTP/1.1 200 OK", "{receipt}");
    assert!(
        first_body.starts_with("9"),
        "the first request carries its own path length: {first_body}"
    );
    assert!(
        second_body.starts_with("5"),
        "the second request carries a different one: {second_body}"
    );
    assert!(
        receipt.contains("av_dispatches=2") && receipt.contains("legacy_dispatches=0"),
        "both requests dispatched to AV and never to the VM: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 151）：**路由表**——`AUTO_AV_ROUTES=derive` 时 `(method, path)`
/// 派生 def 名：`GET /hello` ⇒ `d_hello`、`GET /other` ⇒ `d_other`、
/// `POST /hello` ⇒ `d_hello_post`；派生不到 ⇒ **404**，绝不静默回落 `d_main`。
#[test]
fn p982_av_service_routes_by_method_and_path() {
    let project = scratch("routes");
    let mut service = start(
        &project,
        "fn hello() int { return 11 }\n\
         fn other() int { return 22 }\n\
         fn hello_post() int { return 33 }\n\
         fn main() int { return 99 }\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "4"),
            ("AUTO_AV_ROUTES", "derive"),
        ],
    );
    let addr = wait_ready(&service);

    let (get_hello, body_hello) = request(addr, "/hello");
    let (get_other, body_other) = request(addr, "/other");
    let (post_hello, body_post) = request_method(addr, "POST", "/hello");
    let (get_missing, body_missing) = request(addr, "/missing");
    let receipt = finish(&mut service);

    eprintln!("p982: routes GET /hello = {body_hello:?} GET /other = {body_other:?}");
    eprintln!("p982: routes POST /hello = {body_post:?} GET /missing = {body_missing:?}");
    assert_eq!(get_hello, "HTTP/1.1 200 OK", "{receipt}");
    assert!(body_hello.starts_with("11"), "{body_hello}");
    assert!(
        body_other.starts_with("22"),
        "a different handler ran: {body_other}"
    );
    assert_eq!(post_hello, "HTTP/1.1 200 OK", "{receipt}");
    assert!(
        body_post.starts_with("33"),
        "the method suffix selected its handler: {body_post}"
    );
    assert_eq!(get_missing, "HTTP/1.1 404 Not Found", "{body_missing}");
    assert!(
        body_missing.contains("ace.service/no-route"),
        "no silent fallback to d_main: {body_missing}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
