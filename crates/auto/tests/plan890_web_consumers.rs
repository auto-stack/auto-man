//! Actual CLI composition: configured Web service, graceful exit, and bind failure.
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
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
fn cli_vm_service_uses_explicit_config_and_joins_after_shutdown() {
    cli_service("vm");
}

#[test]
#[ignore = "heavy: actual release generation/build; run serially with --ignored"]
fn cli_rust_service_generates_uses_explicit_config_and_joins_after_shutdown() {
    cli_service("rust");
}

fn cli_service(server: &str) {
    let project = tempfile::Builder::new()
        .prefix("cliweb890")
        .tempdir()
        .unwrap();
    std::fs::write(project.path().join("pac.at"), "name: \"cliweb890\"\n").unwrap();
    std::fs::create_dir_all(project.path().join("src/back")).unwrap();
    std::fs::write(
        project.path().join("src/back/api.at"),
        "#[api(method = \"GET\", path = \"/nonce\")]\npub fn nonce() str { \"cli-web-890\" }\n",
    )
    .unwrap();
    let config =
        r#"{"profile":"development","listen":{"port":0},"limits":{"drain_timeout_ms":100}}"#;
    let mut child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_auto"))
            .args([
                "service",
                "--server",
                server,
                "--dir",
                project.path().to_str().unwrap(),
                "--http-config-inline",
                config,
            ])
            .env("AUTO_EXECUTION_ENGINE", "autovm")
            .env("AUTO_RUST_WORKSPACE", project.path().join("rust-workspace"))
            .env(
                "CARGO_TARGET_DIR",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/plan890-service"),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
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
    let deadline = Instant::now() + Duration::from_secs(600);
    let ready = loop {
        let line = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("CLI did not report ready");
        if let Some((_, json)) = line.split_once("AUTO_SERVICE_READY ") {
            break serde_json::from_str::<serde_json::Value>(json).unwrap();
        }
    };
    let addr: std::net::SocketAddr = ready["bound"].as_str().unwrap().parse().unwrap();
    assert_ne!(addr.port(), 0);
    assert_eq!(ready["profile"], "development");
    let expected = auto_man_runtime::http_service::resolve_config(
        Some(&auto_man_runtime::http_service::HttpConfigSource::Inline(
            config.into(),
        )),
        None,
    )
    .unwrap();
    assert_eq!(
        ready["config_hash"],
        format!("{:016x}", expected.effective_config_hash())
    );
    let reply = request(
        addr.port(),
        b"GET /nonce HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(reply.split_once("\r\n\r\n").unwrap().1, "\"cli-web-890\"");
    let reply = request(addr.port(), b"POST /__auto/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert!(reply.starts_with("HTTP/1.1 202"), "{reply}");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{status}");
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
}
