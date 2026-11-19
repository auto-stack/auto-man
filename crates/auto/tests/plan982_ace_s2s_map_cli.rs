//! PLAN-982（982/REVERSE · round 202，排程令并行项，消费 977 已交面）：
//! `auto trans --target auto --map <sidecar>` ⇒ **ACE 规范方言**改写产物＋
//! 行级 source map sidecar。
//!
//! 1. **实际改写＋可执行**：带历史 `->` 签名的源经改写（箭头消除＝规范形
//!    重写）后在**默认 sys 车道**运行（值 oracle 42）；
//! 2. **诊断映回原源**：坏源（int 函数返回字符串，原行已知）⇒ 改写产物
//!    运行失败（具名诊断＋产物 span）⇒ span→产物行→`source_line_for_output`
//!    ⇒ **映回原源行**；
//! 3. **py 规则实改写**：`use.py` 源经规则链真实改写（`py_call_may(…).?`
//!    命中）＋map 齐（py 产物执行属 py 运行时面，不在本刀）。

#![cfg(feature = "ace-candidate")]

use std::path::PathBuf;
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p982-s2s-map-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

fn trans_map(dir: &PathBuf, name: &str, out: &str, map: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auto"))
        .args([
            "trans", "--path", name, "auto", "--output", out, "--map", map,
        ])
        .current_dir(dir)
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS")
        .output()
        .expect("run auto trans --map")
}

fn run_default(dir: &PathBuf, name: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auto"))
        .arg(name)
        .current_dir(dir)
        .env_remove("AUTO_ACE_CANDIDATE")
        .env_remove("AUTO_ACE_SCRIPT_SYS")
        .output()
        .expect("run auto <canonical product>")
}

/// sidecar JSON → 行级映射函数（消费公开缝 `source_line_for_output`）。
fn line_mapper(map_text: &str) -> impl Fn(usize) -> Option<usize> {
    let value: serde_json::Value = serde_json::from_str(map_text).expect("map sidecar json");
    let entries: Vec<auto_lang_core::trans::SourceMapEntry> = value
        .as_array()
        .expect("map sidecar is an array")
        .iter()
        .map(|e| auto_lang_core::trans::SourceMapEntry {
            output_line: e["output_line"].as_u64().expect("output_line") as usize,
            source_line: e["source_line"].as_u64().expect("source_line") as usize,
            source_file: None,
        })
        .collect();
    move |output_line| auto_lang_core::trans::emit::source_line_for_output(&entries, output_line)
}

/// (1) 实际改写（规范形）＋可执行（默认 sys 车道）＋值 oracle。
#[test]
fn p982_s2s_map_canonical_rewrite_runs_on_the_sys_lane() {
    let dir = scratch("run");
    // 历史 `->` 签名：规范形改写会真实消除箭头。单 fn 形＝sys 车道入口
    // 模块的冻结面（第二 fn 具名拒——多 fn 产物属包车道面，不在本刀）。
    std::fs::write(
        dir.join("in.at"),
        "fn main() -> int {\n    var n = 21\n    return n * 2\n}\n",
    )
    .expect("in.at");

    let out = trans_map(&dir, "in.at", "out.at", "out.map");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "trans --map succeeds: {log}");

    let product = std::fs::read_to_string(dir.join("out.at")).expect("product");
    eprintln!("p982 s2s-map product:\n{product}");
    assert!(
        !product.contains("->"),
        "the canonical rewrite removed the historical arrows:\n{product}"
    );
    assert!(
        product.contains("fn main() int {"),
        "the canonical signature shape:\n{product}"
    );
    let map_text = std::fs::read_to_string(dir.join("out.map")).expect("map sidecar");
    assert!(
        map_text.contains("\"output_line\"") && map_text.contains("\"source_line\""),
        "the sidecar carries the line map:\n{map_text}"
    );

    // 可执行：产物在**默认 sys 车道**跑（值 oracle 42）。
    let run = run_default(&dir, "out.at");
    let run_log = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    eprintln!(
        "p982 s2s-map run: ok={} out={:?}",
        run.status.success(),
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(
        run.status.success()
            && String::from_utf8_lossy(&run.stdout)
                .trim_end()
                .ends_with("42"),
        "the canonical product runs with the source semantics: {run_log}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// (2) 诊断映回原源：坏源（int 函数返回字符串，原行 4）⇒ 产物运行失败 ⇒
/// span→产物行→map ⇒ 原行 4。
#[test]
fn p982_s2s_map_diagnostics_map_back_to_the_original_line() {
    let dir = scratch("diag");
    // 原源：第 4 行 `return "oops"` 在 int 函数里（类型错）。
    let original = "fn bad(n int) -> int {\n    if n > 1 {\n        return n\n    }\n    return \"oops\"\n}\nfn main() -> int {\n    return bad(2)\n}\n";
    std::fs::write(dir.join("in.at"), original).expect("in.at");
    let bad_line = original
        .lines()
        .position(|l| l.contains("return \"oops\""))
        .expect("the bad statement line")
        + 1;

    let out = trans_map(&dir, "in.at", "out.at", "out.map");
    assert!(
        out.status.success(),
        "the transpile itself succeeds: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let run = run_default(&dir, "out.at");
    let err = String::from_utf8_lossy(&run.stderr).to_string();
    eprintln!("p982 s2s-map diag: ok={} err={err:?}", run.status.success());
    assert!(!run.status.success(), "the type error must fail the run");
    // 产物 span（sys 诊断带 AceSpan { start: N, end: M }——产物坐标字节位）。
    let start = parse_first_span_start(&err).expect("the diagnostic carries a span");
    let product = std::fs::read_to_string(dir.join("out.at")).expect("product");
    let product_line = 1 + product[..start.min(product.len())].matches('\n').count();
    eprintln!("p982 s2s-map diag: span={start} product_line={product_line} bad_line={bad_line}");

    let map_text = std::fs::read_to_string(dir.join("out.map")).expect("map sidecar");
    let at = line_mapper(&map_text);
    let mapped =
        at(product_line).unwrap_or_else(|| panic!("product line {product_line} has a map entry"));
    assert_eq!(
        mapped, bad_line,
        "the diagnostic maps back to the ORIGINAL bad line (product line {product_line})"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

fn parse_first_span_start(err: &str) -> Option<usize> {
    let at = err.find("start:")?;
    let rest = &err[at + "start:".len()..];
    let num: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num.parse().ok()
}

/// (3) py 规则实改写：`use.py` 源经规则链真实命中（`py_call_may(…).?`）。
#[test]
fn p982_s2s_map_py_rules_rewrite_for_real() {
    let dir = scratch("py");
    std::fs::write(
        dir.join("in.at"),
        "use.py torch: arange\nfn main() {\n    var t = arange(6)\n    var s = t.sum()\n}\n",
    )
    .expect("in.at");
    let out = trans_map(&dir, "in.at", "out.at", "out.map");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let product = std::fs::read_to_string(dir.join("out.at")).expect("product");
    eprintln!("p982 s2s-map py product:\n{product}");
    assert!(
        product.contains("py_call_may(t, \"sum\").?"),
        "the builtin rule chain rewrote the method call for real:\n{product}"
    );
    assert!(
        dir.join("out.map").is_file(),
        "the rewrite carries its map sidecar"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
