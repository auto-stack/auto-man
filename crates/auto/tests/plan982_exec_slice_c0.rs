//! PLAN-982 / EXEC（983 的 C0 切片）：**显式源路径 + ready／shutdown + 坏配置零写**。
//!
//! 三条判据都落在真实 CLI 上（不 mock）：
//! 1. **显式源路径**：`auto service --dir <project>`（显式目录）真的起服务；
//! 2. **ready／shutdown**：先连得上（ready），再由 `AUTO_SERVICE_MAX_REQUESTS=1`
//!    自行关停，退出码 0 且收据行在；
//! 3. **坏配置零写**：`pac.at` 语法坏掉时命令**非零退出**，且工程目录前后
//!    **文件集合完全一致**（零写）。

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-c0-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("src").join("back")).expect("project dirs");
    dir
}

fn write_project(dir: &Path, pac: &str, api: &str) {
    std::fs::write(dir.join("pac.at"), pac).expect("pac.at");
    std::fs::write(dir.join("src").join("back").join("api.at"), api).expect("api.at");
}

fn free_port() -> u16 {
    let probe = TcpListener::bind("127.0.0.1:0").expect("probe");
    let port = probe.local_addr().expect("addr").port();
    drop(probe);
    port
}

fn snapshot(dir: &Path) -> BTreeSet<String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.insert(
                        path.strip_prefix(base)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

/// 目录的**内容**快照（相对路径 -> 文件字节）：证明"已有文件也没被改写"。
fn content_snapshot(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    let key = path
                        .strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace(char::from(92), "/");
                    out.insert(key, std::fs::read(&path).unwrap_or_default());
                }
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// 现状（round 173 实测）：显式 `--dir` + `--server vm` 的最小工程**不会就绪**——
/// 服务打印 `▶ Starting VM HTTP service …` 后 120 s 超时退出（`VM HTTP service did
/// not become ready within 120s`）。这是 C0 需要的**具名缺口**，修好后去掉 `#[ignore]`。
#[ignore = "regression item: VM readiness is reported by the service hook bind address, and this fixture handler never starts an HTTP service; AUTO_SERVICE_MAX_REQUESTS belongs to the AV path, VM shuts down on its own flow (PLAN-982 round 174)"]
#[test]
fn p982_exec_explicit_path_ready_and_shutdown() {
    let dir = scratch("ready");
    write_project(&dir, "name: \"c0\"\n", "fn main() int { return 5 }\n");
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "vm",
            "--http-config-inline",
            &format!("{{\"profile\":\"development\",\"listen\":{{\"addr\":\"127.0.0.1\",\"port\":{port}}}}}"),
        ])
        .env("AUTO_SERVICE_MAX_REQUESTS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn auto service");

    // ---- ready：连得上就是就绪 ----
    let mut stream = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if let Ok(s) = TcpStream::connect(format!("127.0.0.1:{port}")) {
            stream = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut stream = match stream {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let out = child.wait_with_output().expect("collect failed service");
            panic!(
                "the service becomes ready: status={:?} stdout={:?} stderr={:?}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("read timeout");
    stream
        .write_all(b"GET /c0 HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    assert!(
        response.starts_with("HTTP/1.1"),
        "the explicit-path service answers: {response:?}"
    );

    // ---- shutdown：满 1 个请求后自行退出，退出码 0 且收据在 ----
    let out = child.wait_with_output().expect("service exits");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!(
        "p982 c0 ready/shutdown: status={:?} log={log:?}",
        out.status
    );
    assert!(
        out.status.success(),
        "the service shuts down cleanly: {log}"
    );
    assert!(
        log.contains("service receipt") || log.contains("AV service receipt"),
        "the shutdown receipt is printed: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 现状（round 174 实测，`--server vm`）：坏 `pac.at` **不会**让命令快速失败——服务照常
/// 启动并走到 120 s 就绪超时。零写断言本身成立，但"坏配置失败具名"必须在**真正的配置
/// 消费路径**（AV）上验；等 `--server av` 接入后用同一份 `workspace_service.at` 复核。
#[ignore = "on --server vm a broken pac.at does not fail fast (the service starts and hits the 120s readiness timeout); re-verify on the AV path (PLAN-982 round 174)"]
#[test]
fn p982_exec_bad_config_writes_nothing_and_fails_by_name() {
    let dir = scratch("badcfg");
    // 语法坏掉的 pac.at（未闭合的列表）+ 一个合法 api.at。
    write_project(&dir, "name: [\n", "fn main() int { return 5 }\n");
    let before = snapshot(&dir);
    let before_bytes = content_snapshot(&dir);

    let port = free_port();
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "vm",
            "--http-config-inline",
            &format!("{{\"profile\":\"development\",\"listen\":{{\"addr\":\"127.0.0.1\",\"port\":{port}}}}}"),
        ])
        .output()
        .expect("run auto service");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let after = snapshot(&dir);
    let after_bytes = content_snapshot(&dir);
    eprintln!("p982 c0 bad config: status={:?} log={log:?}", out.status);

    assert!(
        !out.status.success(),
        "a bad configuration fails the command: {log}"
    );
    assert_eq!(
        before, after,
        "a bad configuration adds no file to the project"
    );
    assert_eq!(
        before_bytes, after_bytes,
        "a bad configuration rewrites no existing file"
    );
    let lower = log.to_lowercase();
    assert!(
        lower.contains("pac")
            || lower.contains("config")
            || lower.contains("syntax")
            || lower.contains("parse"),
        "the failure names a configuration/parse diagnostic: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
/// PLAN-982（C0）：`--stack web` 是 `--server av --api <source>` 的**别名**——
/// 缺 `--api` 时具名拒绝（不让栈去猜业务源），未知栈同样具名。
#[test]
fn p982_service_stack_web_is_an_alias_that_requires_an_explicit_source() {
    let dir = scratch("stack");
    write_project(&dir, "name: \"c0\"\n", "fn main() int { return 5 }\n");
    let out = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "av",
            "--stack",
            "web",
        ])
        .output()
        .expect("run auto service --stack web");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("p982 stack web: status={:?} log={log:?}", out.status);
    assert!(
        !out.status.success(),
        "`--stack web` without --api is refused"
    );
    assert!(
        log.contains("ace.service/stack-web-needs-api"),
        "the refusal names the missing explicit source: {log}"
    );

    // 未知 stack 也是具名拒绝（不是静默落到默认车道）。
    let bogus = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "av",
            "--stack",
            "bogus",
        ])
        .output()
        .expect("run auto service --stack bogus");
    let bogus_log = format!(
        "{}{}",
        String::from_utf8_lossy(&bogus.stdout),
        String::from_utf8_lossy(&bogus.stderr)
    );
    assert!(!bogus.status.success(), "an unknown stack is refused");
    assert!(
        bogus_log.contains("ace.service/stack-unknown"),
        "the unknown stack is named: {bogus_log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PLAN-982（C0 · AV CLI 负例）：坏 **HTTP 配置**——CLI 实际消费的那份
/// （`--http-config-inline` / `--http-config`）——必须**具名失败**：诊断点名
/// `http-service config`，非零退出，**不进入服务循环**（无启动横幅、无
/// ready 行），工程目录**字节级零写**。VM pac.at 路径的旧用例保持 ignore，
/// 两条路径各自记账（用户裁定 round 180）。
#[test]
fn p982_av_bad_http_config_fails_by_name_with_zero_writes() {
    let dir = scratch("badhttp");
    write_project(&dir, "name: \"c0\"\n", "fn main() int { return 5 }\n");
    let api = dir.join("src").join("back").join("api.at");
    let before = snapshot(&dir);
    let before_bytes = content_snapshot(&dir);

    // (1) 坏 JSON：inline 配置（CLI 直接读取的那份）。
    let inline = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "av",
            "--api",
            api.to_str().unwrap(),
            "--http-config-inline",
            "{not json",
        ])
        .output()
        .expect("run auto service with a broken inline config");
    let inline_log = format!(
        "{}{}",
        String::from_utf8_lossy(&inline.stdout),
        String::from_utf8_lossy(&inline.stderr)
    );
    eprintln!(
        "p982 bad-http-config inline: status={:?} log={inline_log:?}",
        inline.status
    );
    assert!(
        !inline.status.success(),
        "a broken inline config fails the command"
    );
    assert!(
        inline_log.contains("http-service config error"),
        "the failure names the HTTP config: {inline_log}"
    );
    assert!(
        !inline_log.contains("AUTO_SERVICE_READY"),
        "no readiness is reported: {inline_log}"
    );
    assert!(
        !inline_log.contains("auto serve:"),
        "the failure happens before the serve banner: {inline_log}"
    );
    assert_eq!(
        before,
        snapshot(&dir),
        "a broken inline config adds no file to the project"
    );
    assert_eq!(
        before_bytes,
        content_snapshot(&dir),
        "a broken inline config rewrites no existing file"
    );

    // (2) 坏值：`--http-config` 文件变体（未知 profile 具名）。配置文件放在
    // 工程目录**之外**，保持工程快照纯净。
    let cfg_dir = scratch("badhttp-cfg");
    let bad_json = cfg_dir.join("bad.json");
    std::fs::write(&bad_json, "{\"profile\":\"nope\"}").expect("write bad.json");
    let file_leg = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            dir.to_str().unwrap(),
            "--server",
            "av",
            "--api",
            api.to_str().unwrap(),
            "--http-config",
            bad_json.to_str().unwrap(),
        ])
        .output()
        .expect("run auto service with a broken config file");
    let file_log = format!(
        "{}{}",
        String::from_utf8_lossy(&file_leg.stdout),
        String::from_utf8_lossy(&file_leg.stderr)
    );
    eprintln!(
        "p982 bad-http-config file: status={:?} log={file_log:?}",
        file_leg.status
    );
    assert!(
        !file_leg.status.success(),
        "an unknown profile fails the command"
    );
    assert!(
        file_log.contains("http-service config error") && file_log.contains("profile"),
        "the failure names the profile field: {file_log}"
    );
    assert!(
        !file_log.contains("AUTO_SERVICE_READY"),
        "no readiness is reported: {file_log}"
    );
    assert_eq!(
        before,
        snapshot(&dir),
        "a broken config file adds no file to the project"
    );
    assert_eq!(
        before_bytes,
        content_snapshot(&dir),
        "a broken config file rewrites no existing file"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&cfg_dir);
}
