# Man CLI 结构收口（structure closure）

> PLAN-014 交付合同（r2）。CLI 解析声明/分派/诊断/命令编排的职责分离合同；
> 公开行为零改。分类：structure ｜ 状态：active ｜ 来源计划：
> auto-ui docs/plans/014-ui-man-structure-closure.md

## 1. 模块布局

`crates/auto/src/`：

| 模块 | 职责 |
|---|---|
| `main.rs`（1050行） | `main`/`real_main` 纯分派 + 既有小命令臂 + 2个CLI解析测试mod |
| `cli_spec.rs` | clap面（Cli/Commands/子枚举）、FULL_VERSION、println_logo/init_logger——只声明不编排 |
| `diag.rs` | to_miette_err/format_error_json/format_success_json/output_success——机器诊断唯一格式化来源 |
| `cmd_build.rs` | build编排（strict穿进程登记、端口解析注入、scan/build） |
| `cmd_run.rs` | 裸脚本（dump-lowered审查面+ui-extra动态UI分岔）+run编排（双模入口/端口/env注入/-q分岔/am.run） |
| `cmd_test.rs` | test/test:ui编排 + PATH/包管理器/live子进程助手 |
| `cmd_env.rs` | env/缓存编排 |
| `cmd_trans.rs` | trans/C/Rust/Python/JavaScript/GDScript/r2a 编排 |
| `cmd_req.rs` | serve（前台/stdio/detach经auto_tool_execution）/req 编排 |
| `cmd_wizard.rs` | wizard脚手架（模板include_str路径随迁） |

既有9模块（cmd_service/cmd_ui/cmd_watch/cmd_docs/cmd_a2c_stdlib/cmd_stdlib/
cmd_bp/cmd_autodesk/tool_ui_ops）职责不变，复用不重造。

## 2. owner与不变量

- 工具进程唯一设施：`tool_ui_ops`（I-TOOL seam）+ `auto_tool_client`；daemon
  detach 唯一走 `auto_tool_execution::spawn_detached`。禁止第二发现/owned设施。
- `GENERATION_STRICT` 唯一写者=Build臂；`ToolUiOps.client` 懒建owned子进程。
- Core（crates/auto-core）经路径共享全部源文件；ui-extra 门内模块在 Core 编译
  为 stub——门结构零改。
- 行为不变量：help/flag/默认值/别名、机器JSON形状、stdio/退出码、Run臂
  `exit_audit(1,"main_err_ai_mode")`、裸脚本透传（plan816矩阵）。

## 3. 验证锚点

unit 24/24（含 cli_service_tests/cli_passthrough_tests）；`cargo tcore` 24/24
（Core形状+plan816共享源重编）；plan816_cli_driver+plan890_web_consumers+cmd_ui
真实进程 10/10；901 consumer漂移（死转发feature with-file-history）已随
PLAN-014 T-03 修复（对 auto-lang b1a4871a 可编译）。
