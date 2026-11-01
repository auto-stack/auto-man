//! PLAN-014 T-03：`auto wizard` 命令编排（自 main.rs 尾嵌 run_wizard/
//! to_pascal_case 原样收编，含模板 include_str 路径与缩进零改）。
//! 职责：Plan 363 脚手架（new-project 模板落盘、widget 生成）。
use crate::cli_spec::WizardAction;
use colored::Colorize;

/// Scaffold a new project or widget from templates (Plan 363 Phase 5).
pub(crate) fn run(action: WizardAction) -> miette::Result<()> {
    fn io_err(e: std::io::Error) -> miette::Report {
        miette::miette!("{}", e)
    }

    match action {
        WizardAction::New { name } => {
            let project_dir = std::env::current_dir().map_err(io_err)?.join(&name);
            if project_dir.exists() {
                return Err(miette::miette!("Directory '{}' already exists", name));
            }

            let store_name = format!("{}Store", to_pascal_case(&name));
            let templates = [
                (
                    "pac.at",
                    include_str!(
                        "../../../crates/autoui-skill/templates/new-project/pac.at.tmpl"
                    ),
                ),
                (
                    "src/front/app.at",
                    include_str!(
                        "../../../crates/autoui-skill/templates/new-project/app.at.tmpl"
                    ),
                ),
                (
                    "src/front/store.at",
                    include_str!(
                        "../../../crates/autoui-skill/templates/new-project/store.at.tmpl"
                    ),
                ),
            ];

            for (rel_path, tmpl) in &templates {
                let content = tmpl
                    .replace("{{PROJECT_NAME}}", &name)
                    .replace("{{STORE_NAME}}", &store_name);
                let out_path = project_dir.join(rel_path);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(io_err)?;
                }
                std::fs::write(&out_path, content).map_err(io_err)?;
                println!("{} {}", "✓".bright_green(), rel_path);
            }

            println!("\n{} Project '{}' created.", "✓".bright_green(), name);
            println!("  cd {}", name);
            println!("  auto watch  # start dev server");
            Ok(())
        }
        WizardAction::Add { name } => {
            let widget_name = to_pascal_case(&name);
            let file_name = format!("{}.at", name.to_lowercase().replace(' ', "_"));

            if std::path::Path::new(&file_name).exists() {
                return Err(miette::miette!("File '{}' already exists", file_name));
            }

            let content = format!(
                r#"widget {widget_name} {{
    view {{
        col {{
            text "{widget_name}"
        }}
    }}
}}
"#
            );
            std::fs::write(&file_name, content).map_err(io_err)?;
            println!("{} Created {}", "✓".bright_green(), file_name);
            println!("  Don't forget to add `use {name}: {widget_name}` to app.at");
            Ok(())
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect()
}
