//! PLAN-014 T-03：transpile 命令编排（自 main.rs 原样收编）。
//! 职责：单文件 `auto trans` 及 C/Rust/Python/JavaScript/GDScript/r2a 各
//! 语言臂。行为合同：输出形式（stdout/文件/成功信封）、错误 JSON、
//! 退出码零改。
use crate::cli_spec::TransTarget;
use crate::diag::{format_error_json, output_success, to_miette_err};

pub(crate) fn c(
    ai_mode: bool,
    path: String,
    target: Option<String>,
) -> miette::Result<()> {
if let Some(target_val) = target {
    // SAFETY: set_var is safe in single-threaded context during CLI startup
    #[allow(deprecated)]
    unsafe {
        std::env::set_var("AUTO_TARGET", target_val);
    }
}
let c = auto_lang_core::trans_c(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
output_success(ai_mode, &c);
    Ok(())
}

/// A2R 候选腿（`AUTO_ACE_RUST` 选中时）：读源 → 宿主驱动渲染 → 把
/// `generated.rs`（降级后的程序）作为单文件输出；失败**具名**且不回退旧路径。
#[cfg(feature = "ace-candidate")]
fn rust_via_a2r(ai_mode: bool, path: String) -> miette::Result<()> {
    let source = std::fs::read_to_string(&path).map_err(|e| {
        let err = auto_lang_core::error::AutoError::Io(format!("Failed to read file: {e}"));
        if ai_mode {
            eprintln!("{}", format_error_json(&err));
            std::process::exit(1);
        }
        to_miette_err(err)
    })?;
    auto_man_runtime::rust_engine::note_ace_a2r_dispatch();
    let module = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "trans".to_string());
    match auto_lang_ace_host::rust_transpile::transpile_scalar_source(&source, &module, "main") {
        Ok(fileset) => {
            // 单文件面：输出降级后的程序（三件套中的 generated.rs）。
            let generated = fileset
                .files
                .iter()
                .find(|f| f.name == "generated.rs")
                .map(|f| f.text.clone())
                .unwrap_or_default();
            output_success(ai_mode, &generated);
            Ok(())
        }
        Err(err) => {
            let message = format!("{}: {}", err.code, err.message);
            if ai_mode {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "code": err.code,
                        "message": err.message,
                    })
                );
                std::process::exit(1);
            }
            Err(miette::miette!("{message}"))
        }
    }
}

/// 无桥构建：A2R 被显式选中也没有实现 ⇒ **具名拒绝**，不静默回退旧路径。
#[cfg(not(feature = "ace-candidate"))]
fn rust_via_a2r(ai_mode: bool, _path: String) -> miette::Result<()> {
    let message = "AUTO_ACE_RUST selects A2R but this build has no ACE host \
                   (`ace-candidate` off); refusing instead of falling back to the legacy transpiler";
    if ai_mode {
        eprintln!(
            "{}",
            serde_json::json!({
                "status": "error",
                "code": "ace.rust/executor-unavailable",
                "message": message,
            })
        );
        std::process::exit(1);
    }
    Err(miette::miette!("ace.rust/executor-unavailable: {message}"))
}
pub(crate) fn rust(ai_mode: bool, path: String) -> miette::Result<()> {
    // PLAN-982 982/RUST：显式引擎选择。A2R 选中时走宿主驱动（无缓存、确定性
    // FileSet），并把**真实派发**记进观测；未选中时行为与旧路径完全一致。
    if auto_man_runtime::rust_engine::ace_a2r_selected() {
        return rust_via_a2r(ai_mode, path);
    }
    auto_man_runtime::rust_engine::note_legacy_dispatch();
let r = auto_lang_core::trans_rust(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
output_success(ai_mode, &r);
    Ok(())
}

pub(crate) fn python(ai_mode: bool, path: String) -> miette::Result<()> {
let py = auto_lang_core::trans_python(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
output_success(ai_mode, &py);
    Ok(())
}

pub(crate) fn javascript(ai_mode: bool, path: String) -> miette::Result<()> {
let js = auto_lang_core::trans_javascript(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
output_success(ai_mode, &js);
    Ok(())
}

pub(crate) fn gdscript(ai_mode: bool, path: String) -> miette::Result<()> {
let gd = auto_lang_core::trans_gdscript(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
output_success(ai_mode, &gd);
    Ok(())
}

pub(crate) fn trans(
    ai_mode: bool,
    path: String,
    target: TransTarget,
) -> miette::Result<()> {
    match target {
TransTarget::Ts { output } => {
    let out_path = output.unwrap_or_else(|| {
        std::path::Path::new(&path)
            .with_extension("ts")
            .to_string_lossy()
            .into_owned()
    });
    let msg = auto_lang_core::trans_typescript_to(&path, &out_path).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    println!("{}", msg);
}
TransTarget::C { output } => {
    let c = auto_lang_core::trans_c(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &c)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[trans] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &c);
    }
}
TransTarget::Rust { output, merge } => {
    if merge {
        let merged = auto_lang_core::trans_rust_merged(path.as_str()).map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&e));
                std::process::exit(1);
            }
            to_miette_err(e)
        })?;
        let content = String::from_utf8_lossy(&merged);
        if let Some(out) = output {
            std::fs::write(&out, &*merged)
                .map_err(|e| miette::miette!("Failed to write: {}", e))?;
            println!("[trans] {} -> {} (merged)", path, out);
        } else {
            output_success(ai_mode, &content);
        }
    } else {
        let r = auto_lang_core::trans_rust(path.as_str()).map_err(|e| {
            if ai_mode {
                eprintln!("{}", format_error_json(&e));
                std::process::exit(1);
            }
            to_miette_err(e)
        })?;
        if let Some(_out) = output {
            println!("{}", r);
        } else {
            output_success(ai_mode, &r);
        }
    }
}
TransTarget::Python { output } => {
    let py = auto_lang_core::trans_python(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &py)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[trans] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &py);
    }
}
TransTarget::Js { output } => {
    let js = auto_lang_core::trans_javascript(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &js)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[trans] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &js);
    }
}
TransTarget::Gd { output } => {
    let gd = auto_lang_core::trans_gdscript(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &gd)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[trans] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &gd);
    }
}
TransTarget::Tscn { output } => {
    let tscn = auto_lang_core::trans_tscn(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &tscn)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[trans] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &tscn);
    }
}
TransTarget::Godot { output: _ } => {
    // trans_godot writes <base>.tscn and <base>.gd next to the source.
    let msg = auto_lang_core::trans_godot(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    output_success(ai_mode, &msg);
}
// Plan 555 T07: Auto → Auto（s2s 改写器；W1 identity passthrough）
TransTarget::Auto { output } => {
    let lowered = auto_lang_core::trans_auto_s2s(path.as_str()).map_err(|e| {
        if ai_mode {
            eprintln!("{}", format_error_json(&e));
            std::process::exit(1);
        }
        to_miette_err(e)
    })?;
    if let Some(out) = output {
        std::fs::write(&out, &lowered)
            .map_err(|e| miette::miette!("Failed to write: {}", e))?;
        println!("[s2s] {} -> {}", path, out);
    } else {
        output_success(ai_mode, &lowered);
    }
}
    }
    Ok(())
}

pub(crate) fn r2a(
    ai_mode: bool,
    path: String,
    output: Option<String>,
) -> miette::Result<()> {
let r = auto_lang_core::transpile_r2a_file(path.as_str()).map_err(|e| {
    if ai_mode {
        eprintln!("{}", format_error_json(&e));
        std::process::exit(1);
    }
    to_miette_err(e)
})?;
if let Some(out) = output {
    std::fs::write(&out, &r).map_err(|e| miette::miette!("Failed to write: {}", e))?;
    println!("[r2a] {} -> {}", path, out);
} else {
    output_success(ai_mode, &r);
}
    Ok(())
}
