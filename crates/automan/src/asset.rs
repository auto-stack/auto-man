use rust_embed::*;
use crate::util::{split_first, split_last};
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::io::Write;
use crate::AutoError;


#[derive(Embed)]
#[folder = "assets/templates"]
pub struct Templates;


impl Templates {
    // 复制模板到指定路径
    pub fn copy(template: &str, path: &str) -> Result<(), AutoError> {
        let path = Path::new(path);
        for item in Templates::iter() {
            let (folder, sub_path) = split_first(item.as_ref(), '/');
            // 只读取 template/ 开头的子目录的文件
            if folder == template {
                let (dir, _) = split_last(sub_path, '/');
                // 创建对应的文件夹
                create_dir_all(path.join(dir))?;
                // 创建文件
                let mut f = File::create(path.join(sub_path))?;
                // 写入文件
                f.write_all(Templates::get(item.as_ref()).unwrap().data.as_ref())?;
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
