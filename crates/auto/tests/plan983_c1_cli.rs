//! PLAN-983 C1 永久验收：`POST /api/workspace/open → GET → 重启 → GET`。
//!
//! 目标（用户 2026-11-18 冻结）：GET 与 POST 接到**同一份显式工作区配置**
//! （`JADE_GARDEN_CONFIG`；持久化值优先、env 只作初始回退），从而验出
//! "POST 返回 200，但页面/重启后仍看到旧目录" 的缺陷。
//!
//! 覆盖：
//! 1. 无持久化值 ⇒ GET 回 env 默认（初始回退）；
//! 2. POST open 已存在目录 ⇒ 200 + root/wiki_dir、`<root>/wiki` 创建、
//!    配置文件写入 `workspace_root`；
//! 3. GET 立即读到新目录（同一份配置）；
//! 4. POST open 不存在目录 ⇒ 400 + legacy 同形错误、**零副作用**
//!    （不建目录、不改配置、GET 仍为新目录）；
//! 5. 关停重启后 GET 仍读到持久化目录（env 仍指向旧目录也不影响）。
//!
//! 源 = 消费者仓唯一副本（auto-down `back/auto/av/workspace_service.at`）。

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

fn body_of(reply: &str) -> &str {
    reply.split_once("\r\n\r\n").map(|(_, body)| body).unwrap_or("")
}

struct Service {
    child: OwnedChild,
    port: u16,
    lines: mpsc::Receiver<String>,
    stdout_reader: std::thread::JoinHandle<()>,
}

impl Service {
    /// 启动 `auto service --server av --api <consumer source>`；`config`/
    /// `env_default` 都是转发 slash 路径。
    fn start(source: &std::path::Path, project: &std::path::Path, config: &str, env_default: &str) -> Self {
        let mut child = OwnedChild(
            Command::new(env!("CARGO_BIN_EXE_auto"))
                .args([
                    "service",
                    "--server",
                    "av",
                    "--dir",
                    project.to_str().unwrap(),
                    "--api",
                    source.to_str().unwrap(),
                    "--http-config-inline",
                    "{\"profile\":\"development\",\"listen\":{\"addr\":\"127.0.0.1\",\"port\":0}}",
                ])
                .env("JADE_GARDEN_CONFIG", config)
                .env("JADE_GARDEN_DEFAULT_WORKSPACE", env_default)
                .env(
                    "AUTO_AV_GRANTS",
                    "platform.file,platform.file.path,platform.mem,platform.env",
                )
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn auto service --server av"),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(120);
        let ready = loop {
            let line = lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("CLI did not report ready");
            if let Some((_, json)) = line.split_once("AUTO_SERVICE_READY ") {
                break serde_json::from_str::<serde_json::Value>(json).unwrap();
            }
        };
        let addr: SocketAddr = ready["bound"].as_str().unwrap().parse().unwrap();
        Self {
            child,
            port: addr.port(),
            lines,
            stdout_reader,
        }
    }

    fn get_workspace(&self) -> String {
        request(
            self.port,
            b"GET /api/workspace HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
    }

    fn post_open(&self, root: &str) -> String {
        let body = format!("{{\"root\":\"{root}\"}}");
        let wire = format!(
            "POST /api/workspace/open HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        request(self.port, wire.as_bytes())
    }

    /// 控制面优雅关停 → 子进程退出 0 → 端口重绑。
    fn shutdown(mut self) {
        let reply = request(
            self.port,
            b"POST /__auto/shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert!(reply.starts_with("HTTP/1.1 202"), "{reply}");
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.0.try_wait().unwrap() {
                assert!(status.success(), "CLI exited non-zero: {status}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "CLI failed to join after shutdown"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        self.stdout_reader.join().unwrap();
        let port = self.port;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "port never released");
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut receipt = String::new();
        while let Ok(line) = self.lines.try_recv() {
            receipt.push_str(&line);
            receipt.push('\n');
        }
        assert!(
            receipt.contains("legacy_dispatches=0"),
            "AV dispatch receipt: {receipt}"
        );
    }
}

fn slash(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn c1_open_get_restart_share_one_persisted_workspace_config() {
    let source = consumer_source();
    assert!(
        source.is_file(),
        "consumer AV source missing at {} (set AUTO_DOWN_ROOT)",
        source.display()
    );
    let base = tempfile::Builder::new().prefix("p983c1").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1p").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let dir_a = base.path().join("workspace-a");
    let dir_b = base.path().join("workspace-b");
    let dir_missing = base.path().join("does-not-exist");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let a = slash(&dir_a);
    let b = slash(&dir_b);
    let missing = slash(&dir_missing);
    let config_text = slash(&config);

    let expected = |root: &str| {
        serde_json::json!({ "root": root, "wiki_dir": format!("{root}\\wiki") }).to_string()
    };
    let config_root = |path: &std::path::Path| -> String {
        let text = std::fs::read_to_string(path).expect("workspace config exists");
        let value: serde_json::Value = serde_json::from_str(&text).expect("config JSON");
        value["workspace_root"].as_str().expect("workspace_root").to_string()
    };

    // 1) 无持久化值：GET 回 env 初始回退。
    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(body_of(&reply), expected(&a), "{reply}");
    assert!(!config.exists(), "GET must not create the config file");

    // 2) POST open B：200 + wiki 创建 + 配置写入。
    let reply = service.post_open(&b);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(body_of(&reply), expected(&b), "{reply}");
    assert!(dir_b.join("wiki").is_dir(), "<B>/wiki created");
    assert_eq!(config_root(&config), b, "config persists the opened root");

    // 3) GET 立即读到新目录（同一份显式配置）。
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected(&b), "{reply}");

    // 4) POST open 不存在目录：400 + legacy 同形错误 + 零副作用。
    let reply = service.post_open(&missing);
    assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
    assert_eq!(
        body_of(&reply),
        format!("{{\"error\":\"Directory does not exist: {missing}\"}}"),
        "{reply}"
    );
    assert!(!dir_missing.exists(), "missing dir must not be created");
    assert_eq!(config_root(&config), b, "failed open leaves config untouched");
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected(&b), "{reply}");

    // 5) 关停重启：持久化值优先于 env（env 仍指向 A）。
    service.shutdown();
    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(
        body_of(&reply),
        expected(&b),
        "after restart the persisted root wins over the env fallback: {reply}"
    );
    service.shutdown();

    eprintln!(
        "✓ C1 receipt: POST open → GET → restart → GET share one persisted config (env fallback {a}, persisted {b})"
    );
}
