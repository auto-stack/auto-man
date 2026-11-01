# generated-cargo-context

> **Status**: active（PLAN-910 SD-04）
> 范围：`auto service --server rust`（`http_service::serve_rust`）如何消费语言仓
> 单源的受管生成构建上下文；owned 进程/READY 语义不变。

- workspace 落点由 `auto_web::rust_workspace::resolve_rust_workspace_dir` 以
  **项目目录为锚**解析（cwd 无关）；本仓原 `resolve_workspace_anchored` 重复推断器
  与 `AUTO_RUST_WORKSPACE` 强制设置已退役。
- 后端成员 `<basename>-<id8>-back`（`generated_build::member_dir_name(Back)`）。
- `cargo run --release --manifest-path <member>/Cargo.toml`：cwd=`<ws>`，env 另加
  `GeneratedBuildContext::cargo_env()`（`CARGO_TARGET_DIR=<producer>/target/
  generated-consumers/<ctx8>`、`CARGO_ENCODED_RUSTFLAGS` 256MiB 栈；用户 RUSTFLAGS
  具名隔离）；`context.json` 先于派生写入。
- 服务生命周期内持有 `<root>/.lease/<pid>-<ms>.json`（清理器据此拒绝）；服务退出/
  被杀释放或留 dead-pid 过期件。owned ProcessTree：杀父进程即后端退出、端口释放
  （PLAN-910 V-07 C4 实证）。
- READY 仍只认子进程真实 `AUTO_SERVICE_READY` 行；同工程二次启动而首实例仍运行时
  cargo 无法替换运行中的 exe → "backend exited with 101 before ready"（具名失败）。
