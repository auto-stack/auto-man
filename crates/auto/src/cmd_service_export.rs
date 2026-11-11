//! PLAN-1006 T-04: `auto service-export` — Web 生成成员独立交付的公开 CLI
//! 薄入口（export-only：不启动监听、不复用 UI gen 路由）。全部导出语义在
//! `auto_web::delivery`；本模块只解析参数、解析项目目录并打印可核回执。

use auto_web::delivery::{export_rust_service, ServiceExportRequest};
use miette::Result;
use std::path::PathBuf;

pub struct ExportArgs {
    pub dir: Option<String>,
    pub delivery_root: String,
    pub resource: Vec<String>,
}

pub fn cmd_service_export(args: ExportArgs) -> Result<()> {
    let project_dir = match &args.dir {
        Some(dir) => PathBuf::from(dir),
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
    // 与 cmd_service 相同的规范化（剥 Windows 扩展长度前缀）：导出以原项目为
    // 语义根，锚点按绝对目录计算。
    let project_dir = std::fs::canonicalize(&project_dir)
        .map_err(|e| miette::miette!("failed to canonicalize {}: {e}", project_dir.display()))?;
    const EXTENDED_PREFIX: &str = "\\\\?\\";
    let project_dir = match project_dir.strip_prefix(EXTENDED_PREFIX) {
        Ok(path) => path.to_path_buf(),
        Err(_) => project_dir,
    };
    let receipt = export_rust_service(&ServiceExportRequest {
        project_dir,
        delivery_root: PathBuf::from(&args.delivery_root),
        resources: args.resource.iter().map(PathBuf::from).collect(),
    })
    .map_err(|e| miette::miette!("service-export failed: {e}"))?;
    println!(
        "service-export: member {} -> {}",
        receipt.member_name,
        receipt.delivery_root.display()
    );
    println!(
        "  sdk packages: {} | debug policy: {} | lock sha256: {}",
        receipt.sdk_packages.len(),
        receipt.debug_policy,
        receipt.lock_sha256
    );
    println!(
        "  manifest: {} ({})",
        receipt.manifest_path.display(),
        receipt.manifest_sha256
    );
    if let Some(backup) = &receipt.backup_root {
        println!("  prior bundle preserved at: {}", backup.display());
    }
    Ok(())
}
