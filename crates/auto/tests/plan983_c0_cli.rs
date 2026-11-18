//! PLAN-983 C0 CLI 验收：`auto service --server av --api <AV 源>` 用**同一份**
//! jade-garden `workspace_service.at` 起真实服务——真实 bind、`GET /api/workspace`
//! 字节级 JSON、AV 派发收据、`/__auto/shutdown` 优雅关停与端口重绑。
//!
//! 源=消费者仓唯一副本（auto-down `back/auto/av/workspace_service.at`），不复制；
//! `AUTO_DOWN_ROOT` 或组内相对路径解析，缺失时具名失败。
//!
//! 982 CLI 侧（本用例覆盖）：`--api` 显式源、`--server av` 走 auto-web 宿主
//! （983/AVSERVICE 引擎）、`AUTO_SERVICE_READY` 就绪行、收据
//! `av_dispatches=N legacy_dispatches=0`、控制面关停。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn consumer_source() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = std::env::var("AUTO_DOWN_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../../auto-down"));
    root.join("jade-garden/back/auto/av/workspace_service.at")
}

fn request(port: u16, wire: &[u8]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(wire).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    reply
}

#[test]
fn c0_cli_serves_workspace_from_the_same_av_source() {
    let source = consumer_source();
    let source_text = std::fs::read_to_string(&source).unwrap_or_else(|e| {
        panic!(
            "consumer AV source missing at {} (set AUTO_DOWN_ROOT to the auto-down checkout): {e}",
            source.display()
        )
    });
    assert!(
        source_text.contains("#[api(method = \"GET\", path = \"/api/workspace\")]"),
        "the consumer source declares the /api/workspace route"
    );

    // 工程目录只需存在：`--api` 显式源，不依赖 `src/back/api.at` 发现规则。
    let project = tempfile::Builder::new().prefix("p983c0").tempdir().unwrap();
    let workspace = tempfile::Builder::new().prefix("p983ws").tempdir().unwrap();
    let root = workspace.path().to_string_lossy().replace('\\', "/");
    let wiki_dir = format!("{root}\\wiki");
    let expected = serde_json::json!({ "root": root, "wiki_dir": wiki_dir }).to_string();
    let config = r#"{"profile":"development","listen":{"addr":"127.0.0.1","port":0},"limits":{"drain_timeout_ms":100}}"#;

    let mut child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_auto"))
            .args([
                "service",
                "--server",
                "av",
                "--dir",
                project.path().to_str().unwrap(),
                "--api",
                source.to_str().unwrap(),
                "--http-config-inline",
                config,
            ])
            .env("JADE_GARDEN_DEFAULT_WORKSPACE", &root)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn auto service --server av"),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + Duration::from_secs(120);
    let ready = loop {
        let line = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("CLI did not report ready");
        if let Some((_, json)) = line.split_once("AUTO_SERVICE_READY ") {
            break serde_json::from_str::<serde_json::Value>(json).unwrap();
        }
    };
    let addr: SocketAddr = ready["bound"].as_str().unwrap().parse().unwrap();
    assert_ne!(addr.port(), 0, "kernel-assigned port");
    assert_eq!(ready["profile"], "development");

    // 同一份 AV 源、同一字节级 wire：GET /api/workspace。
    let reply = request(
        addr.port(),
        b"GET /api/workspace HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(
        reply
            .to_ascii_lowercase()
            .contains("content-type: application/json"),
        "{reply}"
    );
    assert_eq!(reply.split_once("\r\n\r\n").unwrap().1, expected, "{reply}");

    // 重复请求稳定（同一编译产物逐请求运行）。
    let reply = request(
        addr.port(),
        b"GET /api/workspace HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(reply.split_once("\r\n\r\n").unwrap().1, expected, "{reply}");

    // 未声明路由：404 空体（派发前拒绝，不派生/不回落）。
    let reply = request(
        addr.port(),
        b"GET /api/nope HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(reply.starts_with("HTTP/1.1 404"), "{reply}");
    assert_eq!(reply.split_once("\r\n\r\n").unwrap().1, "");

    // 控制面优雅关停（loopback 202）→ 子进程退出 0 → 端口重绑。
    let reply = request(
        addr.port(),
        b"POST /__auto/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert!(reply.starts_with("HTTP/1.1 202"), "{reply}");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "CLI exited non-zero: {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "CLI failed to join after shutdown"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    reader.join().unwrap();
    assert!(TcpListener::bind(addr).is_ok(), "listener leaked");

    // 收据：两笔业务请求派发到 AV；legacy 派发 0。
    let mut receipt = String::new();
    while let Ok(line) = rx.try_recv() {
        receipt.push_str(&line);
        receipt.push('\n');
    }
    assert!(
        receipt.contains("av_dispatches=2"),
        "AV dispatch receipt: {receipt}"
    );
    assert!(
        receipt.contains("legacy_dispatches=0"),
        "legacy dispatch receipt: {receipt}"
    );
}
