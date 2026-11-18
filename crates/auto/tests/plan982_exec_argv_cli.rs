//! PLAN-982（Q-982-2 裁定 (a) 第一步／B）：**sys-stdlib 包车道的 argv 真 CLI
//! 验收**——`AUTO_ACE_SCRIPT_SYS=1` 显式候选入口，最小 argv 装配（窄
//! `process` 面＋syscommon/bytes 编解码；无 sysplat、无 spawn 族），授权＝
//! `platform.mem`＋`platform.process.argv`（按最终产物导入表，前门逐项）。
//!
//! 腿：①无参仍有 `argv[0]`（len==1 且非空）；②六形逐项保真（空串/空格/
//! 引号/Unicode/`--` 旗标形/`--` 本体）；③两次执行无串漏（各查各的）；
//! ④零旧 VM 回退（sys 车道标记＋值 oracle）；⑤spawn 负控（窄面不可名
//! spawn ⇒ 具名编译拒，且车道授权无 spawn 族——腿①②已在无 spawn 授权下
//! 跑通即为结构性证据）。

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-argv-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn run_sys(dir: &PathBuf, script: &str, args: &[&str]) -> (bool, String) {
    std::fs::write(dir.join("s.at"), script).expect("write script");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_auto"));
    cmd.arg("s.at")
        .args(args)
        .current_dir(dir)
        .env("AUTO_ACE_CANDIDATE", "1")
        .env("AUTO_ACE_SCRIPT_SYS", "1");
    let out = cmd.output().expect("spawn auto");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), log)
}

/// 断言脚本生成器：期望逐项内嵌（`remove(0)` 取项——1064 同款取值形状）。
fn checker(expect: &[&str]) -> String {
    let mut body = String::new();
    body.push_str("use std.process.args\n\nfn main() int {\n    var argv = args()\n");
    body.push_str(&format!(
        "    if (argv.len() != {}) {{ return 100 }}\n",
        expect.len() + 1
    ));
    body.push_str("    let a0 = argv.remove(0)\n    if (a0.len() == 0) { return 101 }\n");
    for (i, e) in expect.iter().enumerate() {
        let lit = e.replace('\\', "\\\\").replace('"', "\\\"");
        body.push_str(&format!(
            "    let a{} = argv.remove(0)\n    if (a{} != \"{}\") {{ return 1{}0 }}\n",
            i + 1,
            i + 1,
            lit,
            i + 1
        ));
    }
    body.push_str("    return 0\n}\n");
    body
}

#[test]
fn p982_argv_sys_lane_serves_all_shapes_by_real_cli() {
    let dir = scratch("shapes");

    // (1) 无参：仍有 argv[0]（len==1 且非空）。
    let (ok, log) = run_sys(&dir, &checker(&[]), &[]);
    eprintln!("p982 argv no-args: ok={ok} log={log:?}");
    assert!(ok, "no-args still carries argv[0]: {log}");
    assert!(log.contains("ACE sys lane"), "the sys lane marker: {log}");
    // 值 oracle：checker 成功返回 0（len/内容断言都在脚本内）。
    assert!(
        log.trim_end().ends_with("0"),
        "the checker returns 0: {log}"
    );

    // (2) 六形逐项：空串、含空格、含引号、Unicode、--flag=值、`--` 本体。
    let expect = ["", "b c", "say \"hi\"", "你好-λ", "--flag=ón", "--"];
    let (ok2, log2) = run_sys(&dir, &checker(&expect), &expect);
    eprintln!("p982 argv six-shapes: ok={ok2} log={log2:?}");
    assert!(ok2, "all six shapes survive item-by-item: {log2}");
    assert!(
        log2.trim_end().ends_with("0"),
        "the six-shape checker returns 0: {log2}"
    );

    // (3) 两次执行无串漏：各自期望各自成立（第二次不得看见第一次的参数）。
    let (ok3, _log3) = run_sys(&dir, &checker(&["alpha"]), &["alpha"]);
    let (ok4, log4) = run_sys(&dir, &checker(&["beta", "γ"]), &["beta", "γ"]);
    assert!(ok3, "first execution sees its own args");
    assert!(ok4, "second execution sees only its own args: {log4}");

    // (4) spawn 负控：窄面没有 spawn——`use std.process.spawn_with_output`
    //     在 sys 车道＝具名绑定/编译拒，非零退出（授权面也不含 spawn 族）。
    let spawn_script = "use std.process.spawn_with_output\n\nfn main() int { return 0 }\n";
    let (ok5, log5) = run_sys(&dir, spawn_script, &[]);
    eprintln!("p982 argv spawn-neg: ok={ok5} log={log5:?}");
    assert!(!ok5, "spawn is unnameable in the argv lane: {log5}");

    let _ = std::fs::remove_dir_all(&dir);
}
