//! PLAN-736: `auto service` — 工程级 HTTP 服务独立启动（server-only）。
//!
//! 命名注记：计划文本的 `auto serve` 与既有 Plan 269 AutoVM daemon 命令
//! `auto serve`（--foreground/--stdio/--pipe-name）撞名；daemon 是活跃命令面，
//! 本服务入口让位改名 `auto service`（语义不变，T-02 调整已记录）。
//!
//! 不启动 Vue/Vite/桌面 UI/webview；两轨（vm/rust）消费同一份解析后的
//! [HttpServiceConfig]。错误以 miette 报告上抛（main 层 Err = 非零退出）。

use auto_man_runtime::http_service::{resolve_config, serve_project_with_api, HttpConfigSource};
use miette::Result;
use std::path::{Path, PathBuf};

pub struct ServeArgs {
    pub dir: Option<String>,
    pub server: String,
    pub http_config: Option<String>,
    pub http_config_inline: Option<String>,
    pub back_port: Option<String>,
    /// PLAN-983 C0：显式 AV 源路径（仅 `--server av`；默认发现 `src/back/api.at`）。
    pub api: Option<String>,
    /// PLAN-982：`web` 是**别名**——等价于 `--server av` 且显式给 `--api`；
    /// 省略 = 保持既有的 av 宿主 / `d_main` 车道不变。
    pub stack: Option<String>,
}

pub fn cmd_service(args: ServeArgs) -> Result<()> {
    // PLAN-982：`--stack web` 是 `--server av --api <explicit source>` 的**别名**；
    // 未知取值仍是具名拒绝（不静默落到默认车道）。
    let api = args.api.clone();
    if let Some(stack) = args.stack.as_deref() {
        match stack {
            "web" => {
                if api.is_none() {
                    return Err(miette::miette!(
                        "ace.service/stack-web-needs-api: `--stack web` needs an explicit source path via `--api <file>`; the AV file-channel stack never guesses the business source"
                    ));
                }
            }
            other => {
                return Err(miette::miette!(
                    "ace.service/stack-unknown: `--stack {other}` is not a known service stack (omit it for the native lane, or use `web` together with `--api`)"
                ))
            }
        }
    }
    let project_dir = match &args.dir {
        Some(d) => PathBuf::from(d),
        None => {
            std::env::current_dir().map_err(|e| miette::miette!("failed to resolve cwd: {e}"))?
        }
    };
    if !project_dir.exists() {
        return Err(miette::miette!(
            "project directory not found: {}",
            project_dir.display()
        ));
    }
    // 规范化为绝对路径（剥 Windows 扩展长度前缀）：workspace 锚点/模块解析/
    // 子进程 cwd 都按绝对目录计算——相对 `.` 会让 walk-up 锚点算法落 fallback
    // 坏路径。
    let project_dir = std::fs::canonicalize(&project_dir)
        .map_err(|e| miette::miette!("failed to canonicalize {}: {e}", project_dir.display()))?;
    const EXTENDED_PREFIX: &str = "\\\\?\\";
    let project_dir = match project_dir.strip_prefix(EXTENDED_PREFIX) {
        Ok(p) => p.to_path_buf(),
        Err(_) => project_dir,
    };

    // 配置来源互斥：--http-config 与 --http-config-inline 并存 = 指名诊断。
    let source = match (&args.http_config, &args.http_config_inline) {
        (Some(_), Some(_)) => {
            return Err(miette::miette!(
                "--http-config and --http-config-inline are mutually exclusive; pass exactly one"
            ))
        }
        (Some(f), None) => Some(HttpConfigSource::File(PathBuf::from(f))),
        (None, Some(i)) => Some(HttpConfigSource::Inline(i.clone())),
        (None, None) => None,
    };

    // CLI 显式 -B > config listen.port（决策 §3.1）；非法值此处先挡。
    let cli_port = match &args.back_port {
        Some(p) => Some(p.trim().parse::<u16>().map_err(|_| {
            miette::miette!("Invalid backend port '{p}': must be a number 0-65535")
        })?),
        None => None,
    };

    let config = resolve_config(source.as_ref(), cli_port).map_err(|e| miette::miette!("{e}"))?;

    println!(
        "auto serve: project {} · server {} · profile {} · listen {}:{}",
        project_dir.display(),
        args.server,
        config.profile.as_str(),
        config.listen_addr,
        config.listen_port
    );

    let code = serve_project_with_api(
        &project_dir,
        &args.server,
        config,
        api.as_deref().map(Path::new),
    )
    .map_err(|e| miette::miette!("{e}"))?;
    if code != 0 {
        return Err(miette::miette!("http service exited with code {code}"));
    }
    Ok(())
}
