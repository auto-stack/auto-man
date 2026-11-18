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

fn expected_body(root: &str) -> String {
    serde_json::json!({ "root": root, "wiki_dir": format!("{root}\\wiki") }).to_string()
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

    // 2) POST open B：200 + wiki 创建 + 配置写入。root 形态由实现决定（原样
    //    或 Windows canonical），后续断言统一用响应里的 root，避免锁死形态。
    let reply = service.post_open(&b);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let opened: serde_json::Value = serde_json::from_str(body_of(&reply)).expect("reply JSON");
    let opened_root = opened["root"].as_str().expect("root").to_string();
    let normalized = opened_root
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    assert_eq!(normalized, b, "opened root names workspace-b: {opened_root}");
    assert_eq!(
        opened["wiki_dir"].as_str().unwrap(),
        format!("{opened_root}\\wiki"),
        "wiki_dir derives from the same root: {reply}"
    );
    assert!(dir_b.join("wiki").is_dir(), "<B>/wiki created");
    assert_eq!(
        config_root(&config),
        opened_root,
        "config persists the opened root"
    );
    let b_body = body_of(&reply).to_string();

    // 3) GET 立即读到新目录（同一份显式配置）。
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), b_body, "{reply}");

    // 4) POST open 不存在目录：400 + legacy 同形错误 + 零副作用。
    let reply = service.post_open(&missing);
    assert!(reply.starts_with("HTTP/1.1 400"), "{reply}");
    assert_eq!(
        body_of(&reply),
        format!("{{\"error\":\"Directory does not exist: {missing}\"}}"),
        "{reply}"
    );
    assert!(!dir_missing.exists(), "missing dir must not be created");
    assert_eq!(
        config_root(&config),
        opened_root,
        "failed open leaves config untouched"
    );
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), b_body, "{reply}");

    // 5) 关停重启：持久化值优先于 env（env 仍指向 A）。
    service.shutdown();
    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(
        body_of(&reply),
        b_body,
        "after restart the persisted root wins over the env fallback: {reply}"
    );
    service.shutdown();

    eprintln!(
        "✓ C1 receipt: POST open → GET → restart → GET share one persisted config (env fallback {a}, persisted {b})"
    );
}

/// PLAN-983 r15 永久回归：**目标索引准备失败**时必须保旧工作区/配置/旧索引。
///
/// 故障夹具：目标目录内预建 `jade-garden-index.json` 为**非空目录**（写索引
/// 必失败）+ 标记文件；目标 `wiki` 尚未创建。期望：POST ⇒ 500 且 error 以
/// `workspace.index-rebuild-failed:` 开头；GET/重启 GET ⇒ 旧工作区；配置字节
/// 与旧索引字节不变；标记文件原样；目标 wiki 已创建（允许的副作用）。
///
/// 当前实现（r14 子集：配置持久化 + POST，尚未生成索引/原子提交）不满足本
/// 回归，先以 `#[ignore]` 固定；C1 完整语义（982 组合入口）落地后去掉 ignore。
#[test]
#[ignore = "PLAN-983 r15: C1 index preparation + atomic config commit pending (982 combined entry)"]
fn c1_index_failure_keeps_the_old_workspace_and_config() {
    let source = consumer_source();
    let base = tempfile::Builder::new().prefix("p983c1i").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1ip").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let old = base.path().join("old-workspace");
    let target = base.path().join("target-workspace");
    std::fs::create_dir_all(old.join("wiki")).unwrap();
    std::fs::write(old.join("wiki/hello.ad"), "# Hello\n").unwrap();
    std::fs::create_dir_all(target.join("jade-garden-index.json")).unwrap();
    std::fs::write(target.join("jade-garden-index.json/keep.txt"), "keep").unwrap();

    let old_text = slash(&old);
    let target_text = slash(&target);
    let config_text = slash(&config);

    let service = Service::start(&source, project.path(), &config_text, &target_text);
    // 先打开旧工作区（最终实现会在此生成旧索引；当前实现仅写配置）。
    let reply = service.post_open(&old_text);
    assert!(reply.starts_with("HTTP/1.1 200"), "open old workspace: {reply}");
    assert_eq!(body_of(&reply), expected_body(&old_text), "{reply}");
    let config_before = std::fs::read(&config).unwrap_or_default();
    let old_index_path = old.join("jade-garden-index.json");
    let old_index_before = std::fs::read(&old_index_path).ok();
    // 旧索引必须真的生成且含 hello.ad（r16：先证明旧状态，再注入目标失败）。
    let old_index: serde_json::Value =
        serde_json::from_slice(old_index_before.as_ref().expect("old index generated by open"))
            .expect("old index JSON");
    let pages = old_index["pages"].as_array().expect("pages array");
    assert!(
        pages
            .iter()
            .any(|page| page["path"].as_str() == Some("hello.ad")),
        "old index contains hello.ad: {old_index}"
    );
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected_body(&old_text), "{reply}");

    // 目标索引写不进去 ⇒ 500 具名；旧工作区/配置/旧索引保持。
    let reply = service.post_open(&target_text);
    assert!(reply.starts_with("HTTP/1.1 500"), "{reply}");
    let body = body_of(&reply);
    let parsed: serde_json::Value = serde_json::from_str(body).expect("error JSON");
    assert!(
        parsed["error"]
            .as_str()
            .unwrap_or_default()
            .starts_with("workspace.index-rebuild-failed:"),
        "named index failure: {body}"
    );
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected_body(&old_text), "{reply}");
    assert_eq!(
        std::fs::read(&config).unwrap_or_default(),
        config_before,
        "config bytes unchanged on index failure"
    );
    assert_eq!(
        std::fs::read(&old_index_path).ok(),
        old_index_before,
        "old index bytes unchanged"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("jade-garden-index.json/keep.txt")).unwrap(),
        "keep",
        "marker file untouched"
    );
    let wiki_created = target.join("wiki").is_dir();
    service.shutdown();

    // 重启后 GET 仍返回旧工作区（失败不得改动持久配置）。
    let service = Service::start(&source, project.path(), &config_text, &target_text);
    let reply = service.get_workspace();
    assert_eq!(
        body_of(&reply),
        expected_body(&old_text),
        "restart keeps the old workspace after a failed index rebuild: {reply}"
    );
    service.shutdown();
    eprintln!(
        "✓ C1 index-failure receipt: 500 named, old workspace/config/index preserved; target wiki created = {wiki_created}"
    );
}


/// PLAN-983 r15 预置：**Unicode 工作区路径**的 POST/GET/重启同一路径。
/// 现有 interim 源只支持 ASCII（具名 400），本用例固定目标行为。
#[test]
#[ignore = "PLAN-983 r15: Unicode workspace paths pending C1 implementation (interim source returns a named 400)"]
fn c1_unicode_workspace_path_roundtrips() {
    let source = consumer_source();
    let base = tempfile::Builder::new().prefix("p983c1u").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1up").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let ascii = base.path().join("ascii-fallback");
    let unicode = base.path().join("工作区-测试");
    std::fs::create_dir_all(&ascii).unwrap();
    std::fs::create_dir_all(&unicode).unwrap();
    let a = slash(&ascii);
    let u = slash(&unicode);
    let config_text = slash(&config);

    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.post_open(&u);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let opened: serde_json::Value = serde_json::from_str(body_of(&reply)).expect("reply JSON");
    let root = opened["root"].as_str().expect("root").to_string();
    assert!(
        root.ends_with("工作区-测试"),
        "Unicode component preserved in the root: {root}"
    );
    assert_eq!(
        opened["wiki_dir"].as_str().unwrap(),
        format!("{root}\\wiki"),
        "{reply}"
    );
    assert!(unicode.join("wiki").is_dir(), "unicode wiki created");
    let u_body = body_of(&reply).to_string();

    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), u_body, "{reply}");
    let config_text = std::fs::read_to_string(&config).expect("config written");
    let config_value: serde_json::Value = serde_json::from_str(&config_text).unwrap();
    assert_eq!(config_value["workspace_root"].as_str().unwrap(), root);
    service.shutdown();

    let service = Service::start(&source, project.path(), &slash(&config), &a);
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), u_body, "restart keeps the unicode root: {reply}");
    service.shutdown();
}

/// PLAN-983 r14/r15 预置：**Windows canonical 形态**（`\\?\` 前缀 + 反斜杠）
/// 与 legacy `open_workspace_impl` 一致；现有 interim 源返回原样路径。
#[test]
#[ignore = "PLAN-983 r14/r15: Windows canonical (\\\\?\\...) root parity pending; interim source returns the given path"]
fn c1_windows_canonical_paths_match_legacy() {
    let source = consumer_source();
    let base = tempfile::Builder::new().prefix("p983c1c").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1cp").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let ascii = base.path().join("ascii-fallback");
    let target = base.path().join("canonical-target");
    std::fs::create_dir_all(&ascii).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let a = slash(&ascii);
    let config_text = slash(&config);
    let canonical = std::fs::canonicalize(&target)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let expected = serde_json::json!({
        "root": canonical,
        "wiki_dir": format!("{canonical}\\wiki"),
    })
    .to_string();

    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.post_open(&slash(&target));
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(body_of(&reply), expected, "{reply}");
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected, "{reply}");
    let config_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(config_value["workspace_root"].as_str().unwrap(), canonical);
    service.shutdown();

    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected, "restart keeps canonical root: {reply}");
    service.shutdown();
}

/// PLAN-983 r15 预置：**配置提交失败**不得改工作区/配置。
/// 故障夹具：旧工作区打开后把配置文件设为只读，下一次 POST 的配置提交失败。
/// 期望：500 且 error 以 `workspace.config-commit-failed:` 开头（建议前缀，
/// 待实现冻结）；GET/重启 GET 仍是旧工作区；配置字节不变。
#[test]
#[ignore = "PLAN-983 r15: config commit failure semantics pending (proposed prefix workspace.config-commit-failed:)"]
fn c1_config_commit_failure_keeps_the_old_workspace() {
    let source = consumer_source();
    let base = tempfile::Builder::new().prefix("p983c1f").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1fp").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let old = base.path().join("old-workspace");
    let target = base.path().join("target-workspace");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let old_text = slash(&old);
    let target_text = slash(&target);
    let config_text = slash(&config);

    let service = Service::start(&source, project.path(), &config_text, &old_text);
    let reply = service.post_open(&old_text);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let old_body = body_of(&reply).to_string();
    let config_before = std::fs::read(&config).expect("config written");

    let mut perms = std::fs::metadata(&config).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&config, perms).unwrap();

    let reply = service.post_open(&target_text);
    assert!(reply.starts_with("HTTP/1.1 500"), "{reply}");
    let parsed: serde_json::Value = serde_json::from_str(body_of(&reply)).expect("error JSON");
    assert!(
        parsed["error"]
            .as_str()
            .unwrap_or_default()
            .starts_with("workspace.config-commit-failed:"),
        "named config-commit failure: {reply}"
    );
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), old_body, "{reply}");
    assert_eq!(
        std::fs::read(&config).unwrap_or_default(),
        config_before,
        "config bytes unchanged on commit failure"
    );
    service.shutdown();

    let mut perms = std::fs::metadata(&config).unwrap().permissions();
    perms.set_readonly(false);
    std::fs::set_permissions(&config, perms).unwrap();
    let service = Service::start(&source, project.path(), &config_text, &slash(&target));
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), old_body, "restart keeps old workspace: {reply}");
    service.shutdown();
}
