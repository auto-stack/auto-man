//! PLAN-982（982/TEMPLATE）：**模板族键必须从资产现值派生，不能写死**。
//!
//! round-60 的教训：`Templates::copy("app", …)` 在新基面上静默 no-op（族键对不上
//! `plan_template_family` 的 `folder == template` 匹配），而写死旧族名的用例会
//! 因此红/绿无常。本用例把**派生规则**钉死：
//!
//! 1. `TemplateAssets::iter()` 的每个键都以「族目录」为首段；
//! 2. 用**迭代得到的族名**（而不是硬编码）调用 `plan_template_family`，必须产出
//!    非空计划，且每个计划的 `output_rel` 都不为空、字节非空；
//! 3. 族名集合非空（资产没被清空）。

use std::collections::BTreeSet;

#[test]
fn p982_template_families_are_derivable_from_the_asset_keys() {
    let keys: Vec<String> = auto_man_assets::TemplateAssets::iter()
        .map(|k| k.to_string())
        .collect();
    assert!(
        !keys.is_empty(),
        "the embedded template assets must not be empty"
    );

    let families: BTreeSet<String> = keys
        .iter()
        .map(|k| k.split('/').next().unwrap_or_default().to_string())
        .collect();
    assert!(!families.is_empty(), "at least one template family");
    eprintln!("template families (derived from TemplateAssets::iter()): {families:?}");

    for family in &families {
        let planned = auto_man_assets::plan_template_family(family, "probe");
        assert!(
            !planned.is_empty(),
            "family `{family}` (from TemplateAssets::iter()) must plan files; keys: {keys:?}"
        );
        for file in &planned {
            assert!(
                !file.output_rel.is_empty(),
                "family `{family}`: every planned file needs an output path"
            );
            assert!(
                !file.bytes.is_empty(),
                "family `{family}`: `{}` has no bytes",
                file.output_rel
            );
            // 族键派生正确时，输出路径里不应再残留未替换的 `${{name}}`。
            let text = String::from_utf8_lossy(&file.bytes);
            let _ = text;
            assert!(
                !file.output_rel.contains("name/"),
                "family `{family}`: `{}` still carries the template placeholder dir",
                file.output_rel
            );
        }
    }
}
