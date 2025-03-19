use rust_embed::*;
use crate::util::{split_first, split_last};
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::io::Write;
use crate::AutoError;
use auto_lang::universe::Universe;
use auto_val::Value;
use auto_val::AutoStr;
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

#[derive(Embed)]
#[folder = "assets/builders/iar"]
pub struct IARTemplate;

impl IARTemplate {

    pub fn eww() -> Result<AutoStr, AutoError> {
        let file = IARTemplate::get("iar.eww").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewp() -> Result<AutoStr, AutoError> {
        let file = IARTemplate::get("iar.ewp").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewt() -> Result<AutoStr, AutoError> {
        let file = IARTemplate::get("iar.ewt").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
    }

    pub fn ewd() -> Result<AutoStr, AutoError> {
        let file = IARTemplate::get("iar.ewd").unwrap();
        let str = String::from_utf8(file.data.as_ref().to_vec()).unwrap();
        Ok(AutoStr::from(str))
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
