//! PLAN-982（U0，2026-11-18 用户指令）：**Vue 目标的 `auto run` CLI 分派**。
//!
//! 分工（用户裁定）：980 负责生成产物与 `a2x_rt`（工具侧 lane 接线）；
//! 982 负责启动并传回构建诊断（本文件钉住的 CLI 面）；983 负责产品验收。
//!
//! 用例三条：
//! 1. **非 Vue 目标保持具名拒绝**：候选腿下 `auto run`（默认 vm 腿）仍
//!    `ace.exec/unsupported-command`；
//! 2. **Vue 目标分派接通**：候选腿下 `auto run --render vue` **不再**统一
//!    拒绝——分派进入 `run_vue`（`UiOps::vue_run` → auto-ui-tool），并显式
//!    发射 `AUTO_UI_ACE_VUE=1`（980 对签的生成选择键）。工具缺席
//!    （`AUTO_UI_TOOL_BIN` 指向不存在的路径）⇒ **具名**发现错误；
//! 3. **真实链（round 188 去 ignore）**：真 CLI（候选腿）→ auto-ui-tool
//!    （须 `--features ace-vue` 构建）→ pnpm/vite dev server → 真浏览器
//!    `0 → 1 → 0`，外加磁盘 ACE 覆盖断言（lane `App.vue` ＋官方
//!    `a2x_rt.mjs`）。浏览器腿由 `u0_vue_real_chain.mjs`（node+playwright，
//!    与 980 挂载 rig 同族）驱动。**本用例是有意超过 10s 的端到端链**
//!    （CLI 启动＋pnpm/vite＋浏览器），只在显式 `--features ace-candidate`
//!    套件里运行，实际耗时记录在计划 §9 收据。

#![cfg(feature = "ace-candidate")]

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const EXAMPLE_SUBPATH: &str = "examples/ui/002-counter";

fn counter_example() -> Option<PathBuf> {
    // 原样 002-counter（不改一字）：env 显式 → 组内 sibling → 主检出。
    if let Some(root) = std::env::var_os("AUTO_UI_ROOT") {
        let p = PathBuf::from(root).join(EXAMPLE_SUBPATH);
        if p.is_dir() {
            return Some(p);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for base in [
        manifest.join("../../../auto-ui"),
        PathBuf::from("D:/autostack/auto-ui"),
    ] {
        let p = base.join(EXAMPLE_SUBPATH);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-u0-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// 原样复制 002-counter（整树拷贝，零改写）。
fn copy_example(dst: &Path) -> PathBuf {
    let src = counter_example().expect(
        "the unmodified 002-counter example (set AUTO_UI_ROOT or keep the \
         auto-ui sibling/main checkout present)",
    );
    fn walk(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap().flatten() {
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                walk(&from, &to);
            } else {
                std::fs::copy(&from, &to).unwrap();
            }
        }
    }
    walk(&src, dst);
    dst.to_path_buf()
}

fn run_cli(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.args(args)
        .current_dir(dir)
        .env_remove("AUTO_ACE_CANDIDATE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn auto");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// (1) 非 Vue 目标：候选腿仍具名拒绝（vm 腿未迁移）。
#[test]
fn p982_u0_non_vue_run_keeps_the_named_refusal() {
    let dir = scratch("refusal");
    let (ok, log) = run_cli(&dir, &["run"], &[("AUTO_ACE_CANDIDATE", "1")]);
    eprintln!(
        "p982 u0 refusal: ok={ok} log-tail={}",
        log.lines().rev().take(4).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        !ok,
        "the default (vm) run leg stays refused under the candidate"
    );
    assert!(
        log.contains("ace.exec/unsupported-command"),
        "the refusal is named: {log}"
    );
    assert!(
        log.contains("`auto run`"),
        "the refusal names the command: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (2) Vue 目标：分派接通 + 生成选择键发射 + 工具缺席具名错误。
#[test]
fn p982_u0_vue_run_dispatches_and_names_the_missing_provider() {
    let dir = scratch("vue-missing");
    copy_example(&dir);
    // 工具缺席：显式 env 键指向不存在的路径 ⇒ 具名 `EnvPathMissing`。
    let missing_tool = dir.join("no-such-auto-ui-tool.exe");
    let (ok, log) = run_cli(
        &dir,
        &["run", "--render", "vue"],
        &[
            ("AUTO_ACE_CANDIDATE", "1"),
            ("AUTO_UI_TOOL_BIN", missing_tool.to_str().unwrap()),
        ],
    );
    eprintln!(
        "p982 u0 vue-missing: ok={ok} log-tail={}",
        log.lines().rev().take(6).collect::<Vec<_>>().join(" | ")
    );
    assert!(
        !ok,
        "the missing provider fails the command (no silent legacy launch)"
    );
    // 分派证据：候选闸放行 + 生成选择键发射。
    assert!(
        !log.contains("ace.exec/unsupported-command"),
        "the vue target is NOT blanket-refused: {log}"
    );
    assert!(
        log.contains("AUTO_UI_ACE_VUE=1"),
        "the ACE vue lane selection is emitted: {log}"
    );
    // 工具缺席：具名发现错误（不静默、不猜路径；诊断换行/装饰轨归一后比对）。
    let flat: String = log
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        flat.contains("auto-ui-tool discovery failed")
            && flat.contains("AUTO_UI_TOOL_BIN points to missing path"),
        "the missing provider is a named discovery error: {log}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (3) 真实链（round 188 去 ignore）：真 CLI（候选腿）`auto run --render
/// vue` 经 auto-ui-tool（ace-vue 构建）起 pnpm/vite dev server，真浏览器
/// `0 → 1 → 0`；外加磁盘 ACE 覆盖断言。工具定位：`AUTO_UI_TOOL_BIN` →
/// 主检出 target → 组内 sibling target；缺失＝具名失败（带构建指引）。
#[test]
fn p982_u0_vue_run_serves_the_counter_page_with_ace_coverage() {
    let tool = locate_ace_vue_tool();
    let dir = scratch("vue-page");
    copy_example(&dir);
    // 唯一前端口：重跑/并发不撞 3000。
    let port = 3000 + (std::process::id() % 2000) as u16;
    let port_arg = port.to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_auto"))
        .args(["run", "--render", "vue", "--front-port", &port_arg])
        .current_dir(&dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_UI_TOOL_BIN", &tool)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn auto run --render vue");
    // 浏览器腿前先钉住清理路径：即使断言失败也回收整棵 CLI→pnpm→vite 树。
    struct TreeGuard(Child);
    impl Drop for TreeGuard {
        fn drop(&mut self) {
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &self.0.id().to_string()])
                .output();
            let _ = self.0.wait();
        }
    }
    let mut guard = TreeGuard(child);

    // URL 就绪信号可能在**任一**管道：工具把 pnpm/vite 的转发日志写在
    // stderr（实测），CLI 自己的输出在 stdout。两根管道各起一个读线程
    //（防 64KB 写满阻塞），行去掉 ANSI 装饰后匹配本端口 URL；命中即
    // 通知主线程，读线程继续泄流。
    let url_mark = format!("http://127.0.0.1:{port}");
    let log = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    let mut readers = Vec::new();
    let streams: Vec<Box<dyn std::io::Read + Send>> = vec![
        Box::new(guard.0.stdout.take().expect("piped stdout")),
        Box::new(guard.0.stderr.take().expect("piped stderr")),
    ];
    for stream in streams {
        let log = log.clone();
        let ready_tx = ready_tx.clone();
        let url_mark = url_mark.clone();
        readers.push(std::thread::spawn(move || {
            for line in std::io::BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let plain = strip_ansi(&line);
                if let Ok(mut buf) = log.lock() {
                    buf.push_str(&line);
                    buf.push('\n');
                }
                if plain.contains(&url_mark) {
                    let _ = ready_tx.send(());
                }
            }
        }));
    }

    // 轮询等待：URL 命中 / CLI 早退（失败）/ 180s 截止。
    let log_snapshot = |log: &std::sync::Arc<std::sync::Mutex<String>>| {
        log.lock().unwrap_or_else(|p| p.into_inner()).clone()
    };
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut ready = false;
    while !ready {
        if ready_rx.try_recv().is_ok() {
            ready = true;
            break;
        }
        if let Ok(Some(code)) = guard.0.try_wait() {
            // CLI 已退出：给泄流线程一拍收敛，仍未命中 URL 即失败。
            std::thread::sleep(Duration::from_millis(500));
            if ready_rx.try_recv().is_ok() {
                ready = true;
                break;
            }
            panic!(
                "the CLI exited (code {code}) before announcing http://127.0.0.1:{port}; log:\n{}",
                log_snapshot(&log)
            );
        }
        assert!(
            Instant::now() < deadline,
            "no http://127.0.0.1:{port} line within 180s; log:\n{}",
            log_snapshot(&log)
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let log = log_snapshot(&log);
    assert!(
        log.contains("AUTO_UI_ACE_VUE=1"),
        "the ACE vue lane selection is emitted: {log}"
    );

    // 磁盘 ACE 覆盖：App.vue 为 lane 产物（显式 a2x_rt import + a2x_add 体），
    // rt 为官方 AUTO-GENERATED 运行时（980 ④-A #5/#7 的落盘约定）。
    let app_vue = std::fs::read_to_string(dir.join("gen/front/vue/src/App.vue"))
        .expect("gen/front/vue/src/App.vue exists after the run");
    assert!(
        app_vue.contains("from \"./a2x_rt.mjs\"") || app_vue.contains("from './a2x_rt.mjs'"),
        "App.vue imports the lane runtime explicitly:\n{app_vue}"
    );
    assert!(
        app_vue.contains("a2x_add(count.value, 1)"),
        "the + handler goes through the ACE lane helper:\n{app_vue}"
    );
    let rt = std::fs::read_to_string(dir.join("gen/front/vue/src/a2x_rt.mjs"))
        .expect("gen/front/vue/src/a2x_rt.mjs exists next to App.vue");
    assert!(
        rt.contains("AUTO-GENERATED"),
        "the runtime is the official generated artifact:\n{}",
        &rt[..rt.len().min(200)]
    );

    // 浏览器腿：真 vite 页面 0→1→0（+ 再两次 →2）、零 page error。
    let rig = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/u0_vue_real_chain.mjs");
    let started = Instant::now();
    let browser = Command::new("node")
        .arg(&rig)
        .arg(format!("http://127.0.0.1:{port}/"))
        .output()
        .expect("spawn the browser rig");
    let browser_log = format!(
        "{}{}",
        String::from_utf8_lossy(&browser.stdout),
        String::from_utf8_lossy(&browser.stderr)
    );
    assert!(
        browser.status.success(),
        "the served counter page runs 0 -> 1 -> 0 with zero page errors:\n{browser_log}"
    );
    assert!(browser_log.contains("U0-BROWSER-OK"), "{browser_log}");
    eprintln!(
        "p982 u0 real chain: url-ready + artifacts + browser ok (browser leg {:.2}s)",
        started.elapsed().as_secs_f32()
    );

    drop(guard);
    for r in readers {
        let _ = r.join();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// 定位按 `--features ace-vue` 构建的 auto-ui-tool：显式 env → 主检出
/// target → 组内 sibling target；缺失＝具名失败并给出构建指引。
fn locate_ace_vue_tool() -> PathBuf {
    if let Some(path) = std::env::var_os("AUTO_UI_TOOL_BIN") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return p;
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bases = vec![
        manifest.join("../../../auto-ui"),
        PathBuf::from("D:/autostack/auto-ui"),
    ];
    if let Some(root) = std::env::var_os("AUTO_UI_ROOT") {
        bases.insert(0, PathBuf::from(root).join(".."));
    }
    for base in bases {
        let p = base.join("target/debug/auto-ui-tool.exe");
        if p.is_file() {
            return p;
        }
    }
    panic!(
        "auto-ui-tool built with --features ace-vue was not found; build it first: \
         cd auto-ui && cargo build -p auto-ui-tools --features ace-vue \
         (or point AUTO_UI_TOOL_BIN at the executable)"
    );
}

/// 去掉 ANSI 转义序列（vite 的 ready 行把端口号包在加粗装饰里）。
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
