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
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .expect("read timeout");
    let request = format!("GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n");
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
