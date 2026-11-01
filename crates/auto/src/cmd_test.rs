//! PLAN-014 T-03：`auto test`/`auto test:ui` 命令编排（自 main.rs 原样收编）。
//! 职责：.at 测试收集与运行报告；Playwright 一次性 runner 及其
//! PATH/包管理器/子进程 live 输出助手（command_exists/pkg_cmd/
//! run_command_live 随唯一消费者同迁）。行为合同：错误文案/退出码/
//! live stdio 语义零改。
use crate::cli_spec::{init_logger, println_logo};

/// Plan 366a-3 (`auto test:ui`): is a command available in PATH?
pub(crate) fn command_exists(cmd: &str) -> bool {
    #[cfg(windows)]
    let check = std::process::Command::new("where").arg(cmd).output();
    #[cfg(not(windows))]
    let check = std::process::Command::new("which").arg(cmd).output();
    check.map(|o| o.status.success()).unwrap_or(false)
}

/// Plan 366a-3: JS package manager detection (bun > pnpm > npm), same
/// preference order as the vue dev-server tooling.
pub(crate) fn pkg_cmd() -> &'static str {
    if command_exists("bun") {
        "bun"
    } else if command_exists("pnpm") {
        "pnpm"
    } else {
        "npm"
    }
}

/// Plan 366a-3: run a command with live output (inherits stdio). On Windows,
/// cmd.exe /C resolves npm/pnpm/bun .cmd shims from PATH.
pub(crate) fn run_command_live(cmd: &str, args: &[&str], cwd: &std::path::Path) -> Result<(), String> {
    use std::process::Stdio;
    #[cfg(windows)]
    let status = {
        let mut full_args = vec!["/C", cmd];
        full_args.extend(args);
        std::process::Command::new("cmd")
            .args(&full_args)
            .current_dir(cwd)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| format!("Failed to run {}: {}", cmd, e))?
    };
    #[cfg(not(windows))]
    let status = std::process::Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with code {:?}", cmd, status.code()))
    }
}

pub(crate) fn test(
    dir: Option<String>,
    filter: Option<String>,
    verbose: bool,
) -> miette::Result<()> {
let target = dir.unwrap_or_else(|| ".".to_string());

// Collect .at files to test
let path = std::path::Path::new(&target);
let files: Vec<String> = if path.is_file() {
    // File mode: test a single file
    vec![target.clone()]
} else if path.is_dir() {
    // Directory mode: discover all .at files recursively
    let mut found = Vec::new();
    fn collect_at_files(dir: &std::path::Path, out: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    collect_at_files(&p, out);
                } else if p.extension().map_or(false, |e| e == "at" || e == "au") {
                    out.push(p.to_string_lossy().to_string());
                }
            }
        }
    }
    collect_at_files(path, &mut found);
    found.sort();
    if found.is_empty() {
        eprintln!("error: no .at files found in '{}'", target);
        std::process::exit(1);
    }
    found
} else {
    eprintln!("error: '{}' not found", target);
    std::process::exit(1);
};

let start = std::time::Instant::now();
let mut all_results = auto_lang_core::test_runner::TestResult::default();
let mut test_files = 0;
let mut failed_files = 0;
let multi_file = files.len() > 1;

for file in &files {
    match auto_lang_core::test_file(file) {
        Ok(result) => {
            // Skip files with no tests
            if result.reports.is_empty() {
                continue;
            }
            test_files += 1;
            let mut file_failed = 0;
            if multi_file {
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                println!("\nRunning {} ({} tests):", file_name, result.reports.len());
            }
            for report in &result.reports {
                if let Some(f) = &filter {
                    if !report.qualified_name.contains(f.as_str()) {
                        continue;
                    }
                }
                if verbose && !report.stdout.is_empty() {
                    println!("--- {} stdout ---", report.qualified_name);
                    println!("{}", report.stdout);
                }
                match &report.outcome {
                    auto_lang_core::test_runner::TestOutcome::Passed => {}
                    auto_lang_core::test_runner::TestOutcome::Failed(_) => {
                        file_failed += 1
                    }
                }
                all_results.reports.push(report.clone());
            }
            if file_failed > 0 {
                failed_files += 1;
            }
        }
        Err(_) => {
            // Compile errors are expected for non-test files (stdlib, examples, etc.)
            // Only report errors in single-file mode
            if !multi_file {
                eprintln!("error: failed to compile {}", file);
                std::process::exit(1);
            }
        }
    }
}

let elapsed = start.elapsed().as_millis();
print!(
    "{}",
    auto_lang_core::test_runner::format_test_report(&all_results, elapsed)
);

if multi_file {
    println!(
        "{} test file(s), {} file(s) had failures",
        test_files, failed_files
    );
}

// File-based suites (VM, A2R, A2C, A2TS) have entrypoints in test/runners/*.at.
// Plan 263 Phase 2-3: test/runners/a2r_tests.at, test/runners/vm_tests.at, test/runners/a2c_tests.at, test/runners/a2ts_tests.at

if all_results.has_failures() {
    std::process::exit(1);
}
    Ok(())
}

/// TestUi 臂参数（clap 变体字段一一对应）。
pub(crate) struct TestUiArgs {
    pub(crate) dir: Option<String>,
    pub(crate) headed: bool,
    pub(crate) filter: Option<String>,
    pub(crate) report: bool,
}

pub(crate) fn test_ui(ai_mode: bool, a: TestUiArgs) -> miette::Result<()> {
    let TestUiArgs {
        dir,
        headed,
        filter,
        report,
    } = a;
if !ai_mode {
    init_logger();
    println_logo();
}
let project = std::path::PathBuf::from(dir.unwrap_or_else(|| ".".to_string()));
let tests_dir = project.join("tests");
let config = tests_dir.join("playwright.config.ts");
if !config.is_file() {
    eprintln!("error: {} not found", config.display());
    eprintln!("hint: `auto test:ui` expects the 017-chat style tests/ layout:");
    eprintln!("      playwright.config.ts + *.spec.ts + package.json + acceptance.atd");
    std::process::exit(1);
}

let pkg = pkg_cmd();
if !command_exists(pkg) {
    eprintln!(
        "error: '{}' not found in PATH (required to run Playwright)",
        pkg
    );
    std::process::exit(1);
}

// First run: install @playwright/test and friends into tests/.
let package_json = tests_dir.join("package.json");
if package_json.is_file() && !tests_dir.join("node_modules").is_dir() {
    println!(
        "[test:ui] first run in {}: {} install ...",
        tests_dir.display(),
        pkg
    );
    if let Err(e) = run_command_live(pkg, &["install"], &tests_dir) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

if report {
    // Prefer the project's own `report` script, fall back to npx.
    let has_script = |name: &str| {
        std::fs::read_to_string(&package_json)
            .map(|s| s.contains(&format!("\"{}\":", name)))
            .unwrap_or(false)
    };
    let args: Vec<&str> = if has_script("report") {
        vec!["run", "report"]
    } else {
        vec!["exec", "playwright", "show-report", "playwright-report"]
    };
    if let Err(e) = run_command_live(pkg, &args, &tests_dir) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    return Ok(());
}

// Build the playwright argv: `run test -- [--headed] [filter]`.
let mut extra: Vec<String> = Vec::new();
if headed {
    extra.push("--headed".to_string());
}
if let Some(f) = &filter {
    extra.push(f.clone());
}
let has_test_script = std::fs::read_to_string(&package_json)
    .map(|s| s.contains("\"test\":"))
    .unwrap_or(false);
let mut args: Vec<String> = vec!["run".into(), "test".into(), "--".into()];
args.extend(extra.clone());
let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
if !has_test_script {
    // No package script: run the local playwright binary directly.
    let mut direct: Vec<String> =
        vec!["exec".into(), "playwright".into(), "test".into()];
    direct.extend(extra);
    let direct_refs: Vec<&str> = direct.iter().map(|s| s.as_str()).collect();
    println!(
        "[test:ui] {} {} (in {})",
        pkg,
        direct.join(" "),
        tests_dir.display()
    );
    if let Err(e) = run_command_live(pkg, &direct_refs, &tests_dir) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    return Ok(());
}
println!(
    "[test:ui] {} {} (in {}) — app must be running: auto run {}",
    pkg,
    args.join(" "),
    tests_dir.display(),
    project.display()
);
if let Err(e) = run_command_live(pkg, &arg_refs, &tests_dir) {
    eprintln!("error: {}", e);
    std::process::exit(1);
}
    Ok(())
}
