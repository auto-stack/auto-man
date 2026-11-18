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

    /// 控制面优雅关停 → 子进程退出 0 → 端口重绑。返回已排空的关停收据
    ///（含 `combined_entry_dispatches`，供按服务语义断言）。
    fn shutdown(mut self) -> String {
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
        // r20（982 文本路由）：combined ≤ av 恒成立（GET 是 0 参路由=
        // 文件通道，不计文本）；做过 POST 的服务由调用方再断言 combined ≥ 1。
        let (av, combined) = receipt_counts(&receipt);
        assert!(av >= 1, "AV dispatch receipt: {receipt}");
        assert!(
            combined <= av,
            "text-route dispatches cannot exceed total dispatches: {receipt}"
        );
        receipt
    }
}

fn expected_body(root: &str) -> String {
    serde_json::json!({ "root": root, "wiki_dir": format!("{root}\\wiki") }).to_string()
}
fn slash(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// r19（PLAN-983）：关停回据里解析 `av_dispatches` 与
/// `combined_entry_dispatches`——证明请求经 982 组合文本入口执行
/// （而非 r16–r18 的请求文件暂存通道）。
fn receipt_counts(receipt: &str) -> (u64, u64) {
    fn field(line: &str, name: &str) -> u64 {
        line.split_whitespace()
            .find(|token| token.starts_with(name))
            .and_then(|token| token.split('=').nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    }
    let line = receipt
        .lines()
        .rev()
        .find(|line| line.contains("av_dispatches="))
        .unwrap_or("");
    (
        field(line, "av_dispatches="),
        field(line, "combined_entry_dispatches="),
    )
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

    // 5) 关停重启：持久化值优先于 env（env 仍指向 A）。首个服务做过 POST ⇒
    // 其收据必须含文本路由计数（982 组合入口确实执行）。
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );
    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(
        body_of(&reply),
        b_body,
        "after restart the persisted root wins over the env fallback: {reply}"
    );
    let _ = service.shutdown();

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
/// 当前实现（r18，982 组合入口已落主）：AV 源做 canonical + 索引重建 +
/// 原子提交；本回归常驻（无 ignore）。
#[test]
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
    // r14/r15 基线：C1 的 POST/GET 响应是 Windows canonical（`\\?\`）形态。
    let old_canonical = std::fs::canonicalize(&old)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let target_text = slash(&target);
    let config_text = slash(&config);

    let service = Service::start(&source, project.path(), &config_text, &target_text);
    // 先打开旧工作区（打开即生成旧索引）。
    let reply = service.post_open(&old_text);
    assert!(reply.starts_with("HTTP/1.1 200"), "open old workspace: {reply}");
    assert_eq!(body_of(&reply), expected_body(&old_canonical), "{reply}");
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
    assert_eq!(body_of(&reply), expected_body(&old_canonical), "{reply}");

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
    assert_eq!(body_of(&reply), expected_body(&old_canonical), "{reply}");
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
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );

    // 重启后 GET 仍返回旧工作区（失败不得改动持久配置）。
    let service = Service::start(&source, project.path(), &config_text, &target_text);
    let reply = service.get_workspace();
    assert_eq!(
        body_of(&reply),
        expected_body(&old_canonical),
        "restart keeps the old workspace after a failed index rebuild: {reply}"
    );
    let _ = service.shutdown();
    eprintln!(
        "✓ C1 index-failure receipt: 500 named, old workspace/config/index preserved; target wiki created = {wiki_created}"
    );
}


/// PLAN-983 r15 预置：**Unicode 工作区路径**的 POST/GET/重启同一路径。
/// r18（982 组合入口落主）起常驻：AV 源全量 UTF-8 ↔ UTF-16 转换。
#[test]
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
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );

    let service = Service::start(&source, project.path(), &slash(&config), &a);
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), u_body, "restart keeps the unicode root: {reply}");
    let _ = service.shutdown();
}

/// PLAN-983 r14/r15 预置：**Windows canonical 形态**（`\\?\` 前缀 + 反斜杠）
/// 与 legacy `open_workspace_impl` 一致。r18 起常驻。
#[test]
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
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );

    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), expected, "restart keeps canonical root: {reply}");
    let _ = service.shutdown();
}

/// PLAN-983 r15 预置：**配置提交失败**不得改工作区/配置。
/// 故障夹具：旧工作区打开后把配置文件设为只读，下一次 POST 的配置提交失败
/// （`<config>.tmp` 写入后 MoveFileEx 替换只读目标 ⇒ 拒绝）。
/// 期望：500 且 error 以 `workspace.config-commit-failed:` 开头（r18 冻结）；
/// GET/重启 GET 仍是旧工作区；配置字节不变。
#[test]
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
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );

    let mut perms = std::fs::metadata(&config).unwrap().permissions();
    perms.set_readonly(false);
    std::fs::set_permissions(&config, perms).unwrap();
    let service = Service::start(&source, project.path(), &config_text, &slash(&target));
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), old_body, "restart keeps old workspace: {reply}");
    let _ = service.shutdown();
}

/// PLAN-983 r19（用户指令 2）：**并发提交合同**（§5 r15——"并发切换须串行
/// 提交，GET 在切换期间读到一套一致的状态；重启读到最后一次成功提交"）。
/// 结构依据：AV 引擎单 owner 循环顺序派发（`!Send`，dispatch 同步完成）+
/// `<config>.tmp` + MoveFileEx 原子替换 ⇒ 任何 GET 读到的配置都是某一笔
/// 完整提交。本用例把该合同固化为永久回归（并发 POST A/B + 穿插 GET；
/// `Service` 含 `!Sync` 的 stdout 管道，并发腿直接用端口发裸请求）。
#[test]
fn c1_concurrent_switches_stay_consistent_and_last_commit_wins() {
    let source = consumer_source();
    let base = tempfile::Builder::new().prefix("p983c1x").tempdir().unwrap();
    let project = tempfile::Builder::new().prefix("p983c1xp").tempdir().unwrap();
    let config = base.path().join("workspace-config.json");
    let ws_a = base.path().join("ws-a");
    let ws_b = base.path().join("ws-b");
    std::fs::create_dir_all(ws_a.join("wiki")).unwrap();
    std::fs::write(ws_a.join("wiki/a.ad"), "# A
").unwrap();
    std::fs::create_dir_all(ws_b.join("wiki")).unwrap();
    std::fs::write(ws_b.join("wiki/b.ad"), "# B
").unwrap();
    let a = slash(&ws_a);
    let b = slash(&ws_b);
    let canonical = |path: &std::path::Path| {
        std::fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .to_string()
    };
    let expected_a = expected_body(&canonical(&ws_a));
    let expected_b = expected_body(&canonical(&ws_b));
    let config_text = slash(&config);

    let service = Service::start(&source, project.path(), &config_text, &a);
    let port = service.port;
    // 预热：先顺序打开 A（此后 GET 只有 canonical A/B 两种一致形态，
    // 不混入 env 原样回退形态）。
    let reply = service.post_open(&a);
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(body_of(&reply), expected_a, "{reply}");

    // 并发两路切换 + 穿插 GET（hammer 断言每次都是完整一致的状态）。
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let getter = {
        let stop = std::sync::Arc::clone(&stop);
        let expected_a = expected_a.clone();
        let expected_b = expected_b.clone();
        std::thread::spawn(move || {
            let mut seen = 0u64;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let reply = request(
                    port,
                    b"GET /api/workspace HTTP/1.1
Host: 127.0.0.1
Connection: close

",
                );
                let body = body_of(&reply).to_string();
                assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
                assert!(
                    body == expected_a || body == expected_b,
                    "inconsistent GET during concurrent switches: {body}"
                );
                seen += 1;
            }
            seen
        })
    };
    let arrival = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let poster = |root: String, pause_ms: u64| {
        let arrival = std::sync::Arc::clone(&arrival);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(pause_ms));
            let body = format!("{{\"root\":\"{root}\"}}");
            let wire = format!(
                "POST /api/workspace/open HTTP/1.1
Host: 127.0.0.1
Content-Type: application/json
Content-Length: {}
Connection: close

{}",
                body.len(),
                body
            );
            let reply = request(port, wire.as_bytes());
            // 到达序（观测证据）：单 owner 顺序派发下响应写出序=提交序；
            // 仅记入收据，不作断言（客户端线程调度理论上可换到达序）。
            let seq = arrival.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            (reply, root, seq)
        })
    };
    let post_a = poster(a.clone(), 0);
    let post_b = poster(b.clone(), 5);
    let (reply_a, root_a, seq_a) = post_a.join().unwrap();
    let (reply_b, root_b, seq_b) = post_b.join().unwrap();
    assert_eq!(seq_a + seq_b, 3, "two distinct arrival slots");
    let _ = (root_a.as_str(), root_b.as_str());
    assert!(reply_a.starts_with("HTTP/1.1 200"), "{reply_a}");
    assert!(reply_b.starts_with("HTTP/1.1 200"), "{reply_b}");
    assert_eq!(body_of(&reply_a), expected_a, "{reply_a}");
    assert_eq!(body_of(&reply_b), expected_b, "{reply_b}");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let seen = getter.join().unwrap();

    // 最终配置 = 两笔 canonical 提交之一；当前 GET 与之逐字节一致。
    let config_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
    let final_root = config_value["workspace_root"]
        .as_str()
        .expect("workspace_root")
        .to_string();
    let final_expected = expected_body(&final_root);
    assert!(
        final_expected == expected_a || final_expected == expected_b,
        "final config must be one of the committed workspaces: {final_root}"
    );
    let reply = service.get_workspace();
    assert_eq!(body_of(&reply), final_expected, "{reply}");
    let first_receipt = service.shutdown();
    let (_av_first, combined_first) = receipt_counts(&first_receipt);
    assert!(
        combined_first >= 1,
        "POST dispatches must run through the combined text entry: {first_receipt}"
    );

    // 重启：持久配置优先 ⇒ 最后一次成功提交胜出。
    let service = Service::start(&source, project.path(), &config_text, &a);
    let reply = service.get_workspace();
    assert_eq!(
        body_of(&reply),
        final_expected,
        "restart keeps the last committed workspace: {reply}"
    );
    let _ = service.shutdown();
    let order = if seq_a < seq_b { "A->B" } else { "B->A" };
    eprintln!(
        "✓ C1 concurrency receipt: {seen} consistent GETs (config always one complete commit) during concurrent A/B switches; observed POST arrival order {order}; final={final_root} (root_a={root_a}, root_b={root_b}); restart keeps the last commit"
    );
}
