# AutoMan

AutoMan 仓拥有 `auto` 命令、包/构建管理、语言服务 adapter 和 I-TOOL client。当前交付分为两种 profile：

- **Core**：语言与管理命令，不需要 UI 源码或 `auto-ui-tool`。
- **Full**：Core 加固定版本的 AutoUI 工具链和 UI 资产。

当前可执行文件是 **`auto`**，不是 `am`。旧单 crate 的 `cargo install --path .` 与 `am` 说明仅保存在[历史文档](docs/legacy-am.cn.md)，不是当前安装入口。

## 安装

固定来源获取和完整步骤见[安装、版本、升级与恢复](docs/installation.md)。在版本匹配的源码组中执行：

```bash
# Core：只需要 auto-man 与 auto-lang 源码。
python scripts/man-install.py install --profile core --install-root /path/to/auto-core

# Full：需要匹配的 auto-man、auto-lang、auto-ui、auto-down 源码。
python scripts/man-install.py install --profile full --install-root /path/to/auto-full

# 校验安装文件与 profile 身份。
python scripts/man-install.py verify --profile core --install-root /path/to/auto-core
```

安装器先在 staging 构建并校验文件哈希和来源，再交换到目标目录。升级会把旧受管安装保留为具名兄弟 backup；陌生非空目录不会被认领或删除。

## 使用

```bash
auto --version
auto script.at
auto new demo
auto build
auto run
```

Core 对 UI 专用请求返回具名 profile/能力错误。Full 的 UI 生成、构建和运行经真实 `auto-ui-tool` 进程执行，不做静态 UI fallback。

## 开发

```bash
# Core 使用独立 workspace/锁，不读取 full/UI manifest。
cargo build --locked --manifest-path core/Cargo.toml -p auto --no-default-features --features core

# Full 开发构建（需要匹配的 UI 源码兄弟仓）。
cargo build --locked -p auto

# 仓内定向测试入口。
cargo t
cargo tc
cargo tcore
cargo ti
```

跨仓本地 CI 入口是 `auto-lang/scripts/cross-repo-tiers.py`。它会在 Man 集成测试前分别在 AutoUI 构建 `auto-ui-tools`、在 AutoLang 构建 `te-child`，并注入 owner 路径。显式 `AUTO_UI_TOOL_BIN`、`AUTO_TE_CHILD_BIN`/`TE_CHILD_BIN` 覆盖仍优先。

## 源码布局

- `crates/auto`：共享 CLI 源码及 full package/tests。
- `crates/auto-core` 与 `core/`：Core package、workspace、锁和 target，不含 UI path 依赖。
- `crates/auto-man-*`、`crates/auto-package-*`、`crates/auto-toolchain-core`：管理服务与包设施。
- `crates/auto-tool-client`：工具发现与协议 client。
- `scripts/man-install.py`：Core/full 受管安装与校验。
- `scripts/man-distribution.py`：独立 clone、安装、重定位和失败恢复验证。

## 许可证

本项目属于 AutoStack 生态，见 [LICENSE](LICENSE)。
