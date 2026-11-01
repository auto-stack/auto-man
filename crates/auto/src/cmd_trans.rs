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

pub(crate) fn rust(ai_mode: bool, path: String) -> miette::Result<()> {
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
