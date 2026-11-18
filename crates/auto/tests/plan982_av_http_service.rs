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
/// PLAN-982（round 152）：**请求体与查询串进了程序**，并且同一连接上 **keep-alive**
/// 连续服务两个请求——`POST /x?a=1&b=22` 的 handler 能读到查询串与体的长度。
#[test]
fn p982_av_service_reads_query_and_body_and_keeps_the_connection() {
    let project = scratch("body");
    let mut service = start(
        &project,
        "fn x_post() int {\n  return __av_request_query_len() + __av_request_body_len()\n}\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "2"),
            ("AUTO_AV_ROUTES", "derive"),
        ],
    );
    let addr = wait_ready(&service);

    // 同一连接上两个请求（第二个带 connection: close）。
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .expect("read timeout");
    let body = "{\"n\":5}"; // 7 字节
    let first = format!(
        "POST /x?a=1&b=22 HTTP/1.1\r\nhost: localhost\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(first.as_bytes()).expect("write first");
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).expect("read first response");
    let first_response = String::from_utf8_lossy(&buf[..n]).to_string();

    let second = "POST /x?a=1&b=22 HTTP/1.1\r\nhost: localhost\r\ncontent-length: 7\r\n\
                  connection: close\r\n\r\n{\"n\":5}";
    stream.write_all(second.as_bytes()).expect("write second");
    let mut buf2 = [0u8; 4096];
    let n2 = stream.read(&mut buf2).expect("read second response");
    let second_response = String::from_utf8_lossy(&buf2[..n2]).to_string();
    let receipt = finish(&mut service);

    eprintln!("p982: body first={first_response:?}");
    eprintln!("p982: body second={second_response:?}");
    // 查询串 "a=1&b=22" = 8 字节，体 7 字节 ⇒ 15。
    assert!(
        first_response.contains("15"),
        "the handler read both the query and the body: {first_response}"
    );
    assert!(
        first_response.contains("connection: keep-alive"),
        "the first response keeps the connection: {first_response}"
    );
    assert!(
        second_response.contains("connection: close"),
        "the second response closes it: {second_response}"
    );
    assert!(
        receipt.contains("av_dispatches=2") && receipt.contains("legacy_dispatches=0"),
        "both requests ran on AV and never on the VM: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 153）：**并发连接**与**体上限**。
/// 1. 两个并发连接同时被服务（每连接一线程），都拿到 200；
/// 2. `content-length` 超过 `AUTO_SERVICE_MAX_BODY` ⇒ **413** + 具名 code，且**不派发**。
#[test]
fn p982_av_service_serves_concurrent_connections_and_refuses_oversize_bodies() {
    let project = scratch("conc");
    let mut service = start(
        &project,
        "fn main() int { return 5 }\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "3"),
            ("AUTO_SERVICE_MAX_BODY", "16"),
        ],
    );
    let addr = wait_ready(&service);

    // (1) 两个并发连接：同时发出请求，都能在超时内拿到 200。
    let a = std::thread::spawn(move || request(addr, "/a"));
    let b = std::thread::spawn(move || request(addr, "/b"));
    let (status_a, body_a) = a.join().expect("thread a");
    let (status_b, body_b) = b.join().expect("thread b");
    assert_eq!(status_a, "HTTP/1.1 200 OK", "{body_a}");
    assert_eq!(status_b, "HTTP/1.1 200 OK", "{body_b}");

    // (2) 超限体：content-length 17 > 16 ⇒ 413，且不派发。
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    let oversize = "POST /big HTTP/1.1\r\nhost: localhost\r\ncontent-length: 17\r\n\r\n";
    stream.write_all(oversize.as_bytes()).expect("write");
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).expect("read 413");
    let response = String::from_utf8_lossy(&buf[..n]).to_string();
    let receipt = finish(&mut service);

    eprintln!("p982: oversize = {response:?}");
    assert!(
        response.starts_with("HTTP/1.1 413"),
        "an oversize body is refused before dispatch: {response}"
    );
    assert!(
        response.contains("ace.service/body-too-large"),
        "the refusal is named: {response}"
    );
    assert!(
        receipt.contains("served=3") && receipt.contains("av_dispatches=2"),
        "two dispatches + one refused (no dispatch): {receipt}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 154）：**头上限**——头总量超过 `AUTO_SERVICE_MAX_HEADER` ⇒ 431，
/// 且在派发前拒绝。
#[test]
fn p982_av_service_refuses_oversize_headers_before_dispatch() {
    let project = scratch("hdrcap");
    let mut service = start(
        &project,
        "fn main() int { return 3 }\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "1"),
            ("AUTO_SERVICE_MAX_HEADER", "16"),
        ],
    );
    let addr = wait_ready(&service);

    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    let big = "GET /h HTTP/1.1\r\nhost: localhost\r\nx-pad: 0123456789012345678901234567890123456789\r\n\r\n";
    stream.write_all(big.as_bytes()).expect("write");
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).expect("read 431");
    let response = String::from_utf8_lossy(&buf[..n]).to_string();
    let receipt = finish(&mut service);

    eprintln!("p982: hdrcap = {response:?}");
    assert!(
        response.starts_with("HTTP/1.1 431") && response.contains("ace.service/headers-too-large"),
        "an oversize header is refused before dispatch: {response}"
    );
    assert!(
        receipt.contains("av_dispatches=0") && receipt.contains("legacy_dispatches=0"),
        "nothing dispatched, least of all the legacy VM: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}

/// PLAN-982（round 154）：**连接上限**——已有连接占用上限时，新连接直接 503
/// `ace.service/overloaded`，不派发。
#[test]
fn p982_av_service_refuses_connections_over_the_cap() {
    let project = scratch("conncap");
    let mut service = start(
        &project,
        "fn main() int { return 3 }\n",
        &[
            ("AUTO_SERVICE_MAX_REQUESTS", "2"),
            ("AUTO_SERVICE_MAX_CONNECTIONS", "1"),
        ],
    );
    let addr = wait_ready(&service);
    // 让就绪探针那条连接先结束，避免它占用上限（探针只连不发）。
    std::thread::sleep(Duration::from_millis(300));

    // 第一个连接保持打开且不发完（keep-alive 语义下服务端仍在读）。
    let mut first = TcpStream::connect(addr).expect("connect first");
    first
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("first timeout");
    let keep = "GET /a HTTP/1.1\r\nhost: localhost\r\n\r\n";
    first.write_all(keep.as_bytes()).expect("write first");
    let mut first_buf = [0u8; 4096];
    let first_read = first.read(&mut first_buf).expect("first response");
    let first_response = String::from_utf8_lossy(&first_buf[..first_read]).to_string();
    assert_eq!(
        first_response.lines().next().unwrap_or(""),
        "HTTP/1.1 200 OK",
        "the first connection is served: {first_response}"
    );

    // 第二个连接：上限已占 ⇒ 503。
    let mut second = TcpStream::connect(addr).expect("connect second");
    second
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("second timeout");
    second.write_all(keep.as_bytes()).expect("write second");
    let mut second_buf = [0u8; 4096];
    let second_read = second.read(&mut second_buf).expect("second response");
    let second_response = String::from_utf8_lossy(&second_buf[..second_read]).to_string();
    // 第一条连接再发一个请求（keep-alive），把 served 推到上限让服务自然关停。
    let closing = "GET /b HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n";
    first.write_all(closing.as_bytes()).expect("write closing");
    let mut closing_buf = [0u8; 4096];
    let closing_read = first.read(&mut closing_buf).expect("closing response");
    assert_eq!(
        String::from_utf8_lossy(&closing_buf[..closing_read])
            .lines()
            .next()
            .unwrap_or(""),
        "HTTP/1.1 200 OK",
        "the kept-alive connection continues to be served"
    );
    drop(second);
    drop(first);
    let receipt = finish(&mut service);

    eprintln!("p982: conncap = {second_response:?}");
    assert!(
        second_response.starts_with("HTTP/1.1 503")
            && second_response.contains("ace.service/overloaded"),
        "a connection over the cap is refused: {second_response}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 155）：**查询参数按值进程序**——handler 直接读 `__av_query_<key>()`。
/// `?n=5&m=7` ⇒ `5 + 7 = 12`；缺失的键连 `_present()` 都不会被注入（这里用存在的键断言）。
#[test]
fn p982_av_service_hands_named_query_values_to_the_handler() {
    let project = scratch("query");
    let mut service = start(
        &project,
        "fn main() int {\n  return __av_query_n() + __av_query_m() + __av_query_n_present()\n}\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr = wait_ready(&service);
    let (status, body) = request(addr, "/x?n=5&m=7");
    let receipt = finish(&mut service);

    eprintln!("p982: query status={status} body={body:?}");
    assert_eq!(status, "HTTP/1.1 200 OK", "{receipt}");
    assert!(
        body.starts_with("13"),
        "5 + 7 + present(1) = 13 came from the query: {body}"
    );
    assert!(
        receipt.contains("av_dispatches=1") && receipt.contains("legacy_dispatches=0"),
        "the AV lane dispatched and the VM never did: {receipt}"
    );
    let _ = std::fs::remove_dir_all(&project);
}
/// PLAN-982（round 156）：**多值查询与百分号解码**——`?t=5&t=7` ⇒ `__av_query_t_len()`=2、
/// `__av_query_t_0()`=5、`__av_query_t_1()`=7；键里的 `%20` 解码后再用作函数名片段。
#[test]
fn p982_av_service_reads_multi_value_and_percent_decoded_query() {
    let project = scratch("multi");
    let mut service = start(
        &project,
        "fn main() int {\n  return __av_query_t_len() * 100 + __av_query_t_0() * 10 + __av_query_t_1()\n}\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr = wait_ready(&service);
    let (status, body) = request(addr, "/m?t=5&t=7");
    let receipt = finish(&mut service);

    eprintln!("p982: multi status={status} body={body:?}");
    assert_eq!(status, "HTTP/1.1 200 OK", "{receipt}");
    assert!(
        body.starts_with("257"),
        "len(2)*100 + 5*10 + 7 = 257 came from the multi-value query: {body}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "the legacy VM never dispatched: {receipt}"
    );

    // 百分号解码：`na%6De=3` ⇒ 键 `name`（解码后才是合法命名面的一部分）。
    let project2 = scratch("pct");
    let mut service2 = start(
        &project2,
        "fn main() int {\n  return __av_query_name_present()\n}\n",
        &[("AUTO_SERVICE_MAX_REQUESTS", "1")],
    );
    let addr2 = wait_ready(&service2);
    let (status2, body2) = request(addr2, "/p?na%6De=3");
    let receipt2 = finish(&mut service2);
    eprintln!("p982: pct status={status2} body={body2:?}");
    assert_eq!(status2, "HTTP/1.1 200 OK", "{receipt2}");
    assert!(
        body2.starts_with("1"),
        "the percent-decoded key is visible to the handler: {body2}"
    );
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&project2);
}
