//! PLAN-982（982/DEVSDK）：**watch 快路径"不触发 Rust 重编"的护栏**。
//!
//! 合同 V 第 1 条："真实 process 树 cargo/rustc 0，Rust 输入变化仍正确重建"。
//! `cmd_watch.rs` 目前**自身不 spawn 任何子进程**（只重建 SFC/`gen/back` 并打印
//! 提示），所以"Auto 编辑不引起 cargo/rustc"是**由构造保证**的。
//!
//! 本用例是**源码级护栏**：断言 `cmd_watch.rs` 里没有 `Command::new` / `process::`
//! 的 spawn 面——这样"未来谁给 watch 加了 cargo 调用"会立刻变红，而不是靠复核者
//! 逐行读。运行时行为（产物更新）由 watch 既有测试覆盖，本文件不重复。

use std::path::PathBuf;

fn watch_source() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    std::fs::read_to_string(root.join("crates/auto/src/cmd_watch.rs"))
        .expect("the watch command source is readable")
}

#[test]
fn p982_watch_never_spawns_cargo_or_rustc() {
    let source = watch_source();
    assert!(
        source.contains("pub fn run_watch("),
        "the file still holds the watch entrypoint (else this guard is meaningless)"
    );
    assert!(
        !source.contains("Command::new("),
        "cmd_watch.rs must not spawn child processes (that is what keeps cargo/rustc at zero \
         for Auto edits); found a Command::new call"
    );
    assert!(
        !source.contains("process::Command"),
        "cmd_watch.rs must not import/use process::Command"
    );
}
