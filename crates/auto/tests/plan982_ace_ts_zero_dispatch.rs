//! PLAN-982（982/WEBTARGET）：**"未选中引擎时零 A2X 派发"的独立进程见证**。
//!
//! 派发计数是**进程级**的，同进程内的其它用例会污染它（round 74 的教训）。因此
//! 这条断言单独成一个测试二进制：`cargo test` 每个 Test target 是一个**独立进程**，
//! 这里的计数从 0 开始，"零派发"才有意义。
//!
//! 断言：未设 `AUTO_ACE_TS` 时，回执是 `engine:"legacy"`、`a2js_dispatches:0`；
//! 记一次 legacy 派发后 `legacy_dispatches >= 1`。

#![cfg(feature = "ace-candidate")]

#[test]
fn p982_ts_legacy_selects_zero_a2js_dispatches_in_a_fresh_process() {
    assert!(
        std::env::var_os("AUTO_ACE_TS").is_none(),
        "this binary must run without AUTO_ACE_TS in the environment"
    );

    let dir = std::env::temp_dir().join(format!(
        "p982-ts-zero-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch");
    let path = dir.join("ts-proof.json");

    std::env::set_var("AUTO_ACE_TS_PROOF", path.to_str().unwrap());
    // 旧腿：记一次 legacy 派发，然后写回执。
    auto_man_runtime::script_engine::note_legacy_dispatch();
    auto_man_runtime::script_engine::write_proof_if_requested();
    std::env::remove_var("AUTO_ACE_TS_PROOF");

    let text = std::fs::read_to_string(&path).expect("the proof receipt is written");
    assert!(text.contains("\"engine\":\"legacy\""), "engine: {text}");
    assert!(
        text.contains("\"a2js_dispatches\":0"),
        "a fresh legacy process must show ZERO A2JS dispatches: {text}"
    );
    assert!(
        text.contains("\"legacy_dispatches\":1"),
        "the legacy dispatch is counted: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
