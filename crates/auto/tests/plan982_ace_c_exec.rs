//! PLAN-982 / V-B-C：**真实 A2C 生成物经 MSVC 编译、链接、运行**。
//!
//! 链条：Auto 源 → `auto_lang_core::trans::c::transpile_c`（产品 `Target` 走的就是
//! 这一条）→ `.c`/`.h` 写盘 → `cl`（经 vcvars64 加载 MSVC 环境）编译链接 →
//! 运行 exe → 断言源语义。
//!
//! 环境门槛：找不到 `cl`（vcvars64.bat 不存在或不可用）时**具名跳过**，
//! 不把"这台机器没有 C 工具链"记成失败。

use std::path::{Path, PathBuf};
use std::process::Command;

const VCVARS: &str =
    r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat";

const SRC: &str = "fn main() int {\n  print(\"c-leg:hello\")\n  return 0\n}\n";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-c-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// 通过一个**批处理**加载 vcvars 再跑命令：`cmd` 的 `&&` 链在 Rust 里传参容易被
/// 再引用，写 .bat 最稳；输出重定向进文件，避免依赖管道。
fn run_in_vs(dir: &Path, bat_body: &str) -> (bool, String) {
    let bat = dir.join("step.bat");
    let out = dir.join("step.out");
    let script = format!(
        "@echo off\r\ncall \"{VCVARS}\" >nul 2>&1\r\n{bat_body} > \"{}\" 2>&1\r\n",
        out.display()
    );
    std::fs::write(&bat, script).expect("write step.bat");
    let status = Command::new("cmd")
        .args(["/c", bat.to_str().unwrap()])
        .current_dir(dir)
        .status()
        .expect("spawn cmd");
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    (status.success(), text)
}

/// 工具链是否可用：`cl` 打印版本横幅即视为可用。
fn cl_available(dir: &Path) -> bool {
    if !Path::new(VCVARS).exists() {
        return false;
    }
    // `cl` 无参数时以非零退出（用法），因此只看输出内容。
    let (_ok, text) = run_in_vs(dir, "cl");
    eprintln!("p982 c-leg probe: {text:?}");
    text.contains("Microsoft") && text.contains("C/C++")
}
#[test]
fn p982_c_transpiled_program_compiles_and_runs_with_msvc() {
    let probe_dir = scratch("probe");
    if !cl_available(&probe_dir) {
        eprintln!("p982 c-leg: skipped (no MSVC `cl` via vcvars64)");
        return;
    }
    let dir = scratch("leg");

    // 1) 真实 A2C 生成物（与产品 `Target::transpile_auto` 同一 API）。
    let mut code = auto_lang_core::trans::c::transpile_c("main", SRC).expect("A2C transpiles");
    let c_text = code.done().expect("the C body renders");
    assert!(!c_text.is_empty(), "the transpiler produced a C body");
    std::fs::write(dir.join("main.c"), &c_text).expect("write main.c");
    if !code.header.is_empty() {
        std::fs::write(dir.join("main.h"), &code.header).expect("write main.h");
    }

    // 2) `cl` 编译链接（同一 cmd 里 call vcvars）。
    let (ok, build_log) = run_in_vs(&dir, "cl /nologo /W0 /Fe:prog.exe main.c");
    assert!(
        ok && dir.join("prog.exe").exists(),
        "MSVC compiles and links the A2C output:\n{build_log}"
    );

    // 3) 运行：源语义 `print("c-leg:hello")` 必须出现。
    let run = Command::new(dir.join("prog.exe"))
        .current_dir(&dir)
        .output()
        .expect("run prog.exe");
    let out = String::from_utf8_lossy(&run.stdout).to_string();
    eprintln!("p982 c-leg: status={:?} stdout={out:?}", run.status);
    assert!(
        out.contains("c-leg:hello"),
        "the compiled C program prints the source semantics: {out:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
/// PLAN-982 / V-B-C（多模块）：两个 Auto 模块各自生成 C，由 `cl` **一起**编译链接，
/// 跨模块调用在真实可执行文件里生效。
/// 现状（PLAN-982 round 169 实测）：A2C **不降级跨模块限定调用**——`util.add(2, 3)`
/// 被原样写成 C（`error C2224: ".add"的左侧必须具有结构/联合类型`）。这条用例保留为
/// **具名缺口**，修好后去掉 `#[ignore]` 即为验收。
#[ignore = "A2C does not lower qualified cross-module calls (util.add); see PLAN-982 round 169"]
#[test]
fn p982_c_multi_module_program_links_and_runs() {
    let probe_dir = scratch("probe2");
    if !cl_available(&probe_dir) {
        eprintln!("p982 c-leg(multi): skipped (no MSVC `cl` via vcvars64)");
        let _ = std::fs::remove_dir_all(&probe_dir);
        return;
    }
    let _ = std::fs::remove_dir_all(&probe_dir);

    let dir = scratch("multi");
    // 两个模块：`util` 提供 add；`main` 调它并按结果打印。
    let util_src = "pub fn add(a int, b int) int {\n  return a + b\n}\n";
    let main_src = "use util\nfn main() int {\n  var r int = util.add(2, 3)\n  print(f\"multi:${r}\")\n  return 0\n}\n";

    for (module, src) in [("util", util_src), ("main", main_src)] {
        let mut code = auto_lang_core::trans::c::transpile_c(module, src)
            .unwrap_or_else(|e| panic!("A2C transpiles `{module}`: {e}"));
        let body = code.done().expect("the C body renders");
        assert!(!body.is_empty(), "`{module}` produced a C body");
        std::fs::write(dir.join(format!("{module}.c")), &body).expect("write .c");
        if !code.header.is_empty() {
            std::fs::write(dir.join(format!("{module}.h")), &code.header).expect("write .h");
        }
    }

    let (ok, build_log) = run_in_vs(&dir, "cl /nologo /W0 /Fe:prog.exe main.c util.c");
    assert!(
        ok && dir.join("prog.exe").exists(),
        "MSVC compiles and links both modules:\n{build_log}"
    );
    let run = Command::new(dir.join("prog.exe"))
        .current_dir(&dir)
        .output()
        .expect("run prog.exe");
    let out = String::from_utf8_lossy(&run.stdout).to_string();
    eprintln!("p982 c-leg(multi): status={:?} stdout={out:?}", run.status);
    assert!(
        out.contains("multi:5"),
        "the cross-module call works in the compiled binary: {out:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
