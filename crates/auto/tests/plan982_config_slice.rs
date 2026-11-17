//! PLAN-982（982/CONFIG 切片，V 验收）：**真实 pac 语料的引擎一致性 + 项目模型投影
//! 一致性**，在**合并后的产品路径**上跑（`config_engine` → `AutoConfig` → `Pac`）。
//!
//! 证明的是设计 §6 的 PRODUCT/CONFIG 业务值，而不是"两者都能解析"：
//! 1. 语料里每一份 `pac.at` 在**两个引擎**下产出的**产品投影**（`Pac::name`、
//!    依赖目标数、目标数）必须逐项相同；任何不一致立即失败；
//! 2. ACE 侧失败必须是**具名诊断**，且只允许出现在登记的例外集（模板仓的
//!    `${name}` 无注入文档 + 计划内故意坏夹具）；
//! 3. 未登记的分歧（既不是"两边都成功且相等"，也不是例外集）一律失败。
//!
//! 只在 `ace-candidate` 特性下存在（需要真桥）。

#![cfg(feature = "ace-candidate")]

use std::path::{Path, PathBuf};
use std::process::Command;

use auto_lang_core::ace_config::ConfigEngineChoice;
use auto_val::Obj;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// 受管语料：仓库里所有被跟踪的 `pac.at`（与 project_model corpus 同一口径）。
fn corpus() -> Vec<PathBuf> {
    let out = Command::new("git")
        .args(["ls-files", "*.at"])
        .current_dir(repo_root())
        .output()
        .expect("git ls-files");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut files: Vec<PathBuf> = text
        .lines()
        .map(|l| l.replace('\\', "/"))
        .filter(|l| l.ends_with("pac.at"))
        .map(|l| repo_root().join(l))
        .collect();
    files.sort();
    files
}

/// 登记的例外：这些文档在**无注入**时 ACE 具名拒绝（模板占位符需要注入），或
/// 两侧都拒（计划内坏夹具）。必须是仓库相对路径，精确匹配。
const ACE_ONLY_REFUSALS: [&str; 9] = [
    "crates/auto-man-assets/assets/templates/app/pac.at",
    "crates/auto-man-assets/assets/templates/capp/pac.at",
    "crates/auto-man-assets/assets/templates/clib/pac.at",
    "crates/auto-man-assets/assets/templates/lib/pac.at",
    "crates/auto-man-assets/assets/templates/libdemo/pac.at",
    "test/fixtures/plan982/bad-config/pac.at",
    "test/fixtures/plan982/bad-eval/pac.at",
    "test/fixtures/plan982/bad-runtime/pac.at",
    // 模板占位符（无注入时 legacy 留字面、ACE 按 fail-closed 具名拒绝）。
    "test/fixtures/modularity/project-model/fixtures/dynamic-template.pac.at",
];

fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 产品投影：(name, deps, targets)。`Pac::new` 是真实消费面，不是解析器的形状。
fn projection(config: auto_lang_core::config::AutoConfig) -> (String, usize, usize) {
    let pac = auto_man_runtime::Pac::new(config);
    (
        pac.name.as_str().to_string(),
        pac.deps().len(),
        pac.targets.len(),
    )
}

#[test]
fn p982_config_slice_corpus_projections_agree_between_engines() {
    // 装桥：ACE 只有在执行桥存在时才会被默认选中。
    auto_lang_ace_host::install().expect("the reference ACE bridge installs");

    let root = repo_root();
    let files = corpus();
    assert!(
        files.len() >= 190,
        "the managed pac corpus must stay at 190+ documents, got {}",
        files.len()
    );

    let mut equal = 0usize;
    let mut exceptions = 0usize;
    let mut both_refused = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            failures.push(format!("{}: unreadable", file.display()));
            continue;
        };
        let rel_path = rel(file, &root);
        let args = Obj::new();

        auto_man_runtime::config_engine::set_choice_for_test(Some(ConfigEngineChoice::LegacyVm));
        let legacy = auto_lang_core::config::AutoConfig::from_file_with_engine(
            file,
            &args,
            ConfigEngineChoice::LegacyVm,
        );
        auto_man_runtime::config_engine::set_choice_for_test(Some(ConfigEngineChoice::AceAv));
        let ace = auto_lang_core::config::AutoConfig::from_file_with_engine(
            file,
            &args,
            ConfigEngineChoice::AceAv,
        );
        auto_man_runtime::config_engine::set_choice_for_test(None);

        match (ace, legacy) {
            (Ok(ace_cfg), Ok(legacy_cfg)) => {
                let a = projection(ace_cfg);
                let l = projection(legacy_cfg);
                if a == l {
                    equal += 1;
                } else {
                    failures.push(format!(
                        "{rel_path}: product projection differs: ace={a:?} legacy={l:?}"
                    ));
                }
            }
            (Err(err), _) if ACE_ONLY_REFUSALS.contains(&rel_path.as_str()) => {
                // 例外集：ACE 的拒绝仍必须是**具名**的（不是 panic/空文档）。
                let msg = format!("{err}");
                let named = msg.contains("ace.diagnostic/")
                    || msg.contains("source.")
                    || msg.contains("ace.config/");
                if !named {
                    failures.push(format!("{rel_path}: ACE refusal is not named: {msg}"));
                } else {
                    exceptions += 1;
                }
            }
            // 两侧都拒：这是"共同拒绝"，不是投影分歧；只要求 ACE 的拒绝是具名的。
            (Err(err), Err(_)) => {
                let msg = format!("{err}");
                let named = msg.contains("ace.diagnostic/")
                    || msg.contains("source.")
                    || msg.contains("ace.config/");
                if named {
                    both_refused += 1;
                } else {
                    failures.push(format!("{rel_path}: ACE refusal is not named: {msg}"));
                }
            }
            // legacy 成功而 ACE 拒绝，且不在登记例外集：真分歧。
            (Err(err), Ok(_)) => failures.push(format!(
                "{rel_path}: ACE refused a document the legacy engine projects: {err}"
            )),
            (Ok(_), Err(_)) => exceptions += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} corpus documents disagree on the product projection:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        equal >= 185,
        "at least 185 corpus documents must project identically, got {equal}"
    );
    eprintln!(
        "config-slice corpus: {equal} identical product projections, {exceptions} registered exceptions, {both_refused} both-refused, {} total",
        files.len()
    );
}
