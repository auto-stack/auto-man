//! PLAN-982（983 最小服务链，形状 A）：`auto service --server av` 的**显式选择与
//! 具名拒绝**。
//!
//! 用户裁定形状 A：HTTP 服务器用 `auto_web`（Rust），handler 由 **ACE AV** 执行。
//! 宿主能力 `platform.http-service/1` 仍在 982 内实现中，因此本轮先把 **CLI 合同**
//! 钉住：
//! 1. `--server av` 与 `--server ace` 都被**识别**（不是 "unknown --server"）；
//! 2. **PLAN-982 round 149 起**：`av` 轨已经是真实服务；没有 `src/back/api.at`
//!    的工程以具名错误 `ace.service/no-api` 拒绝（仍然不回退 `vm` 轨）；
//! 3. 真实请求/错误/超时/关停的证据见 `plan982_av_http_service`。

use std::path::PathBuf;
use std::process::Command;

const PAC: &str = "name: \"svc\"\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-svc-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    std::fs::write(dir.join("pac.at"), PAC).expect("pac.at");
    dir
}

fn run_service(project: &PathBuf, server: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "service",
            "--dir",
            project.to_str().unwrap(),
            "--server",
            server,
            "--http-config-inline",
            "{\"profile\":\"development\",\"listen\":{\"addr\":\"127.0.0.1\",\"port\":0}}",
        ])
        .output()
        .expect("spawn auto service")
}

#[test]
fn p982_service_av_track_is_recognised_and_refuses_by_name() {
    let dir = scratch("av");
    let out = run_service(&dir, "av");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        !out.status.success(),
        "the av track has no host yet, so it must exit non-zero: {combined}"
    );
    assert!(
        !combined.contains("unknown --server"),
        "`av` must be a recognised track, not an unknown server: {combined}"
    );
    assert!(
        combined.contains("ace.service/no-api")
            || combined.contains("ace.service/bridge-unavailable")
            || combined.contains("ace.service/host-unavailable"),
        "the refusal is named `ace.service/*`: {combined}"
    );
    // miette 会把长消息按行折行并插入 `│` 边框字符，比较前先剥掉再归一化空白。
    let normalized = combined
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        normalized.contains("never falls back to the legacy VM track"),
        "the refusal states that it never falls back to the legacy VM track: {normalized}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn p982_service_ace_alias_behaves_the_same() {
    let dir = scratch("ace");
    let out = run_service(&dir, "ace");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "the ace alias must also be refused: {combined}"
    );
    assert!(
        !combined.contains("unknown --server"),
        "`ace` is an alias of `av`, not an unknown server: {combined}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
