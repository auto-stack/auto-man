use rust_embed::*;
use crate::util::{split_first, split_last};
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::io::Write;
use crate::AutoError;
use auto_lang::scope::Universe;
use auto_val::Value;

#[derive(Embed)]
#[folder = "assets/templates"]
pub struct Templates;


impl Templates {
    // 复制模板到指定路径
    pub fn copy(template: &str, path: &str) -> Result<(), AutoError> {
        let path = Path::new(path);
        let pac_name = path.file_name().unwrap().to_str().unwrap();
        for item in Templates::iter() {
            let (folder, sub_path) = split_first(item.as_ref(), '/');
            // 只读取 template/ 开头的子目录的文件
            if folder == template {
                let sub_path = sub_path.replace("name", pac_name);
                println!("Copying {}", sub_path);
                let (dir, _) = split_last(sub_path.as_ref(), '/');
                // 创建对应的文件夹
                create_dir_all(path.join(dir))?;
                // 创建文件
                let mut f = File::create(path.join(sub_path))?;
                // 读取模板文件
                let code = String::from_utf8(Templates::get(item.as_ref()).unwrap().data.as_ref().to_vec()).unwrap();
                // 替换模板中的 ${name} 为 pac_name
                let mut universe = Universe::new();
                universe.set_global("name", Value::from(pac_name));
                let auto_code = auto_lang::eval_template(&code, universe);
                let result = auto_code.unwrap().result;
                let code = result.repr();
                println!("code: {}", code);
                // 写入文件
                f.write_all(code.as_bytes())?;
            }
        }
        Ok(())
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
