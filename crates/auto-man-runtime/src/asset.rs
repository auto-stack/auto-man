use crate::util::split_last;
use crate::{AutoError, AutoResult};
use auto_lang_core::interpreter::AutoInterpreter;
use auto_man_assets::{plan_template_family, GhsAssets, IarAssets, TemplateAssets};
use auto_val::AutoStr;
use auto_val::Value;
use rust_embed::{EmbeddedCompressedFile, EmbeddedFile, Filenames, RustEmbed};
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::Path;

/// PLAN-982（982/TEMPLATE · round 202）：模板取消旗标——
/// `AUTO_ACE_TEMPLATE_CANCEL_AFTER_MS`（毫秒）：
/// - `Some(0)` ⇒ **立即取消**（同步置旗——确定性语义，宿主/测试可依赖）；
/// - `Some(ms>0)` ⇒ watchdog 线程在 ms 后置旗（与 EXEC 腿同款纪律）；
/// - 未设 ⇒ 不取消。
/// 写盘段在**写前与逐文件之间**检查；取消 ⇒ 具名 `ace.template/cancelled`
/// ＋回滚（见 [`materialize_transactional`]）。
static TEMPLATE_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub const TEMPLATE_CANCEL_ENV: &str = "AUTO_ACE_TEMPLATE_CANCEL_AFTER_MS";

fn arm_template_cancel_watchdog() {
    // 每次物化都是独立 run：先清旗（不继承上一 run 的取消信号——进程内
    // 串行多次 copy 不残留）。
    TEMPLATE_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let ms = match std::env::var(TEMPLATE_CANCEL_ENV) {
        Ok(v) => match v.trim().parse::<u64>() {
            Ok(ms) => ms,
            Err(_) => return,
        },
        Err(_) => return,
    };
    if ms == 0 {
        TEMPLATE_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        TEMPLATE_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
    });
}

fn template_cancelled() -> bool {
    TEMPLATE_CANCEL.load(std::sync::atomic::Ordering::SeqCst)
}

/// PLAN-982（982/TEMPLATE · round 202）：**事务式写盘段**——取消通道＋中断
/// 回滚，[`Templates::copy`] 与 [`Templates::copy_user`] 共用。
///
/// - 回滚账本：本 run **新建的文件**删除；**既有文件按字节还原**（`File::
///   create` 会截断——覆盖前先快照原字节）；本 run **新建的目录**（已空）
///   按逆序移除。失败/取消后已有工作区**逐字节不变**，绝无半成品。
/// - 失败具名：`ace.template/cancelled`（取消）、`ace.template/write: <路径>
///   （<原因>）`（写盘中断）；回滚自身的失败附在同一消息里（best-effort，
///   绝不吞掉首因）。
fn materialize_transactional(rendered: Vec<(std::path::PathBuf, String)>) -> Result<(), AutoError> {
    arm_template_cancel_watchdog();
    if template_cancelled() {
        return Err("ace.template/cancelled: the template materialization observed                     cancellation before any write"
            .into());
    }
    let mut backups: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    let mut created_files: Vec<std::path::PathBuf> = Vec::new();
    let mut created_dirs: Vec<std::path::PathBuf> = Vec::new();
    let outcome = (|| -> Result<(), AutoError> {
        for (out, text) in rendered {
            if template_cancelled() {
                return Err("ace.template/cancelled: the template materialization                             observed cancellation mid-write"
                    .into());
            }
            if let Some(parent) = out.parent() {
                // 记录本 run 新建的目录链（create_dir_all 不告知哪些是新建）。
                let mut missing: Vec<std::path::PathBuf> = Vec::new();
                let mut probe = parent.to_path_buf();
                while !probe.as_os_str().is_empty() && !probe.exists() {
                    missing.push(probe.clone());
                    match probe.parent() {
                        Some(p) => probe = p.to_path_buf(),
                        None => break,
                    }
                }
                create_dir_all(parent)
                    .map_err(|e| format!("ace.template/write: {} ({e})", parent.display()))?;
                created_dirs.extend(missing);
            }
            // 快照只对**常规文件**做：产物位若被目录等非常规项占用，后续
            // create 自会具名失败（读目录在 Windows 上是 os error 5）。
            if out.is_file() {
                let bytes = std::fs::read(&out)?;
                backups.push((out.clone(), bytes));
            } else {
                created_files.push(out.clone());
            }
            let mut f = File::create(&out)
                .map_err(|e| format!("ace.template/write: {} ({e})", out.display()))?;
            f.write_all(text.as_bytes())
                .map_err(|e| format!("ace.template/write: {} ({e})", out.display()))?;
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => Ok(()),
        Err(first) => {
            // 回滚：还原既有字节 → 删除新建文件 → 逆序移除新建（已空）目录。
            let mut rollback_notes: Vec<String> = Vec::new();
            for (path, bytes) in backups.iter().rev() {
                if let Err(e) = std::fs::write(path, bytes) {
                    rollback_notes.push(format!("restore {}: {e}", path.display()));
                }
            }
            for path in created_files.iter().rev() {
                if let Err(e) = std::fs::remove_file(path) {
                    rollback_notes.push(format!("remove {}: {e}", path.display()));
                }
            }
            for path in created_dirs.iter() {
                if path
                    .read_dir()
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false)
                {
                    if let Err(e) = std::fs::remove_dir(path) {
                        rollback_notes.push(format!("rmdir {}: {e}", path.display()));
                    }
                }
            }
            let note = if rollback_notes.is_empty() {
                String::new()
            } else {
                format!("; rollback issues: {}", rollback_notes.join(", "))
            };
            Err(format!("{first}{note}").into())
        }
    }
}

/// 模板脚手架兼容门面（PLAN-803）：资源唯一来源在
/// `crates/auto-man-assets`（`TemplateAssets`）。本类型保留旧公开面——
/// inherent `copy` 与 `RustEmbed` 的 get/iter 转发；不再 derive 第二份
/// 嵌入。Auto 求值与文件物化留在此上层（adapter）。
pub struct Templates;

impl Templates {
    // 复制模板到指定路径
    //
    // PLAN-803：枚举/族选择/路径 "name" 替换经
    // `auto_man_assets::plan_template_family`（旧臂冻结投影，顺序一致）；
    // 逐文件的求值/输出步骤保持旧臂原样——含 unwrap panic 语义与
    // 「先创建/截断文件再求值」的遗留顺序（行为改善另立计划，不在
    // 结构迁移中夹带）。
    /// PLAN-982（982/TEMPLATE）：**独立用户模板**——`template` 是文件系统目录时，
    /// 按目录内容逐文件渲染到目标工程（`${name}` 替换；`pac.at` 在选中
    /// `AUTO_ACE_TEMPLATE=ace` 时走 ACE 求值）。嵌入族（`app`/`clib`/…）仍走
    /// [`Templates::copy`]。
    /// PLAN-982（982/TEMPLATE）：**独立用户模板**（事务式物化）。
    ///
    /// 先把整个模板**渲染进内存**（任何文件失败都还没落盘），再统一写盘；
    /// 这样"坏模板"在生成阶段就失败，目标 workspace 连一个新文件都不会多出来
    /// （round 62 的实现是边走边写，坏模板会留下半成品）。
    pub fn copy_user(template_dir: &Path, path: &str) -> Result<(), AutoError> {
        let dest = Path::new(path);
        let pac_name = dest
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "the project name must be a directory name".to_string())?;
        // 渲染阶段：不写任何文件。
        let mut rendered: Vec<(std::path::PathBuf, String)> = Vec::new();
        let mut stack = vec![template_dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                let src = entry.path();
                if src.is_dir() {
                    stack.push(src);
                    continue;
                }
                let rel = src
                    .strip_prefix(template_dir)
                    .map_err(|e| format!("template path is outside the template dir: {e}"))?;
                // 输出路径里的 `name` 占位符按目标工程名替换（与嵌入族一致）。
                let out_rel = rel.to_string_lossy().replace("name", pac_name);
                let code = String::from_utf8_lossy(&std::fs::read(&src)?).to_string();
                let text = if out_rel.ends_with("pac.at")
                    && crate::template_engine::ace_template_selected()
                {
                    let mut args = auto_val::Obj::new();
                    args.set("name", Value::from(pac_name));
                    let config = crate::config_engine::parse_code(&code, &args)
                        .map_err(|e| format!("user template `{out_rel}` under ACE: {e}"))?;
                    auto_val::Value::Node(Box::new(config.root))
                        .repr()
                        .to_string()
                } else {
                    let mut interp = AutoInterpreter::new();
                    interp.set_global("name", Value::from(pac_name));
                    interp
                        .eval_template("", &code)
                        .map_err(|e| format!("user template `{out_rel}`: {e:?}"))?
                        .repr()
                        .to_string()
                };
                rendered.push((dest.join(out_rel), text));
            }
        }
        rendered.sort_by(|a, b| a.0.cmp(&b.0));
        // 写盘阶段：所有渲染都已成功——事务式（round 202：取消＋中断回滚）。
        materialize_transactional(rendered)
    }
    pub fn copy(template: &str, path: &str) -> Result<(), AutoError> {
        let path = Path::new(path);
        let pac_name = path.file_name().unwrap().to_str().unwrap();
        // PLAN-982（982/TEMPLATE）：**事务式物化**——先把整族渲染进内存，
        // 任何一个文件失败都还没落盘（round 63 把用户模板改成这个顺序，
        // 本轮把嵌入族补齐），再统一写盘。
        let mut rendered: Vec<(String, String)> = Vec::new();
        for planned in plan_template_family(template, pac_name) {
            let sub_path = planned.output_rel;
            // F-816-B-R1-05：框架诊断/模板进度走 stderr——stdout 在
            // `--format json` 下必须只承载机器输出。
            eprintln!("Copying {}", sub_path);
            // 模板原始字节（未求值）
            let code = String::from_utf8_lossy(&planned.bytes).to_string();
            // 配置文件（`pac.at`）的求值可以走 ACE 引擎（`AUTO_ACE_TEMPLATE=ace`）；
            // 其余文件仍按旧模板内插（它们不是配置文档）。
            let code = if sub_path.ends_with("pac.at")
                && crate::template_engine::ace_template_selected()
            {
                let mut args = auto_val::Obj::new();
                args.set("name", Value::from(pac_name));
                let config = crate::config_engine::parse_code(&code, &args)
                    .map_err(|e| format!("template `{sub_path}` under ACE: {e}"))?;
                auto_val::Value::Node(Box::new(config.root))
                    .repr()
                    .to_string()
            } else {
                // 替换模板中的 ${name} 为 pac_name（旧模板内插）
                let mut interp = AutoInterpreter::new();
                interp.set_global("name", Value::from(pac_name));
                interp
                    .eval_template("", &code)
                    .map_err(|e| format!("template `{sub_path}`: {e:?}"))?
                    .repr()
                    .to_string()
            };
            rendered.push((sub_path, code));
        }
        rendered.sort_by(|a, b| a.0.cmp(&b.0));
        // 写盘阶段：所有渲染都已成功——事务式（round 202：取消＋中断回滚）。
        let planned: Vec<(std::path::PathBuf, String)> = rendered
            .into_iter()
            .map(|(sub_path, code)| (path.join(sub_path), code))
            .collect();
        materialize_transactional(planned)
    }
}

// inherent get/iter/compressed：旧 derive 除 trait impl 外还生成同名
// inherent 方法（无需 import trait 即可 `Templates::iter()` 调用）——
// 兼容门面逐形态保留。
impl Templates {
    pub fn get(file_path: &str) -> Option<EmbeddedFile> {
        TemplateAssets::get(file_path)
    }
    pub fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        TemplateAssets::compressed(file_path)
    }
    pub fn iter() -> impl Iterator<Item = std::borrow::Cow<'static, str>> + 'static {
        TemplateAssets::iter()
    }
}

impl RustEmbed for Templates {
    fn get(file_path: &str) -> Option<EmbeddedFile> {
        TemplateAssets::get(file_path)
    }
    fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        TemplateAssets::compressed(file_path)
    }
    // F803-R1-01：保持旧 derive trait impl 的具体返回类型 Filenames——
    // RPITIT 的隐藏类型为具体类型时跨 crate 对调用方可见；若此处写
    // `impl Iterator`，外部 `let _: Filenames = <Templates as RustEmbed>::iter()`
    // 会以 E0308 失编（公开 API 回归）。必须转发底层 **trait** 入口，
    // 不能转发 inherent iter()（后者本就是 opaque，derive 亦如此）。
    // F803-R2-01：该返回类型细化**有意**作为本 crate 公开 API 的一部分
    //（旧臂兼容承诺），按 rustc 指引在方法级最小范围局部允许
    // refining_impl_trait，不回退 opaque、不扩大 allow 范围。
    #[allow(refining_impl_trait)]
    fn iter() -> Filenames {
        <TemplateAssets as RustEmbed>::iter()
    }
}

/// GHS 构建器模板兼容门面（PLAN-803）：字节唯一来源 `GhsAssets`，
/// 旧 inherent default/program/subproject 与 RustEmbed get/iter 保留。
pub struct GHSTemplate;

impl GHSTemplate {
    pub fn default() -> AutoResult<AutoStr> {
        let file = GhsAssets::get("default.gpj").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(str.into())
    }

    pub fn program() -> AutoResult<AutoStr> {
        let file = GhsAssets::get("program.gpj").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(str.into())
    }

    pub fn subproject() -> AutoResult<AutoStr> {
        let file = GhsAssets::get("subproject.gpj").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(str.into())
    }
}

impl GHSTemplate {
    pub fn get(file_path: &str) -> Option<EmbeddedFile> {
        GhsAssets::get(file_path)
    }
    pub fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        GhsAssets::compressed(file_path)
    }
    pub fn iter() -> impl Iterator<Item = std::borrow::Cow<'static, str>> + 'static {
        GhsAssets::iter()
    }
}

impl RustEmbed for GHSTemplate {
    fn get(file_path: &str) -> Option<EmbeddedFile> {
        GhsAssets::get(file_path)
    }
    fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        GhsAssets::compressed(file_path)
    }
    // F803-R1-01：保持旧 derive trait impl 的具体返回类型 Filenames——
    // RPITIT 的隐藏类型为具体类型时跨 crate 对调用方可见；若此处写
    // `impl Iterator`，外部 `let _: Filenames = <GHSTemplate as RustEmbed>::iter()`
    // 会以 E0308 失编（公开 API 回归）。必须转发底层 **trait** 入口，
    // 不能转发 inherent iter()（后者本就是 opaque，derive 亦如此）。
    // F803-R2-01：该返回类型细化**有意**作为本 crate 公开 API 的一部分
    //（旧臂兼容承诺），按 rustc 指引在方法级最小范围局部允许
    // refining_impl_trait，不回退 opaque、不扩大 allow 范围。
    #[allow(refining_impl_trait)]
    fn iter() -> Filenames {
        <GhsAssets as RustEmbed>::iter()
    }
}

/// IAR 构建器模板兼容门面（PLAN-803）：字节唯一来源 `IarAssets`，
/// 旧 inherent eww/ewp/ewt/ewd 与 RustEmbed get/iter 保留。
pub struct IARTemplate;

impl IARTemplate {
    pub fn eww() -> Result<AutoStr, AutoError> {
        let file = IarAssets::get("iar.eww").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewp() -> Result<AutoStr, AutoError> {
        let file = IarAssets::get("iar.ewp").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewt() -> Result<AutoStr, AutoError> {
        let file = IarAssets::get("iar.ewt").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewd() -> Result<AutoStr, AutoError> {
        let file = IarAssets::get("iar.ewd").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }
}

impl IARTemplate {
    pub fn get(file_path: &str) -> Option<EmbeddedFile> {
        IarAssets::get(file_path)
    }
    pub fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        IarAssets::compressed(file_path)
    }
    pub fn iter() -> impl Iterator<Item = std::borrow::Cow<'static, str>> + 'static {
        IarAssets::iter()
    }
}

impl RustEmbed for IARTemplate {
    fn get(file_path: &str) -> Option<EmbeddedFile> {
        IarAssets::get(file_path)
    }
    fn compressed(file_path: &str) -> Option<EmbeddedCompressedFile> {
        IarAssets::compressed(file_path)
    }
    // F803-R1-01：保持旧 derive trait impl 的具体返回类型 Filenames——
    // RPITIT 的隐藏类型为具体类型时跨 crate 对调用方可见；若此处写
    // `impl Iterator`，外部 `let _: Filenames = <IARTemplate as RustEmbed>::iter()`
    // 会以 E0308 失编（公开 API 回归）。必须转发底层 **trait** 入口，
    // 不能转发 inherent iter()（后者本就是 opaque，derive 亦如此）。
    // F803-R2-01：该返回类型细化**有意**作为本 crate 公开 API 的一部分
    //（旧臂兼容承诺），按 rustc 指引在方法级最小范围局部允许
    // refining_impl_trait，不回退 opaque、不扩大 allow 范围。
    #[allow(refining_impl_trait)]
    fn iter() -> Filenames {
        <IarAssets as RustEmbed>::iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates() {
        for template in Templates::iter() {
            println!("{}", template.as_ref());
        }
    }
}
