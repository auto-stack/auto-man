use std::path::Path;
use std::fs::{create_dir_all, File};
use std::io::Write;
use crate::AutoError;
use crate::fs::copy_dir_all;
use rust_embed::RustEmbed;
use crate::asset::Templates;
use crate::util::{split_first, split_last};

pub struct Man {
}



// API
impl Man {
    pub fn new() -> Self {
        Self { }
    }

    pub fn create(&self, name: &str) -> Result<(), AutoError> {
        // 1. Check if project already exists
        let path = Path::new(name);
        if path.is_file() {
            return Err(format!("A file named with {} already exists", name).into());
        }
        if path.is_dir() {
            // TODO: Let the user choose to replace the directory
            return Err(format!("A directory named with {} already exists", name).into());
        }
        // 2. Copy default template to destination
        self.copy_template(path, "default")?;
        Ok(())
    }

    pub fn build(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn run(&self) -> Result<(), AutoError> {
        Ok(())
    }

    pub fn clean(&self) -> Result<(), AutoError> {
        Ok(())
    }
}


// Implementations
impl Man {
    fn copy_template(&self, path: &Path, template: &str) -> Result<(), AutoError> {
        println!("Copying template from {}", template);
        for item in Templates::iter() {
            println!("Checking {}", item.as_ref());
            let (folder, sub_path) = split_first(item.as_ref(), '/');
            // 只读取 template/ 开头的子目录的文件
            if folder == template {
                println!("Copying {}", sub_path);
                let (dir, _) = split_last(sub_path, '/');
                println!("Creating {}", dir);
                // 创建对应的文件夹
                create_dir_all(path.join(dir))?;
                // 创建文件
                let mut f = File::create(path.join(sub_path))?;
                // 写入文件
                f.write_all(Templates::get(item.as_ref()).unwrap().data.as_ref())?;
                f.flush()?;
            }
            //create_file_recursive(path, sub.as_ref())?;
            // write data into file
            // let data = get_template(sub.as_ref()).unwrap();
            // std::fs::write(path.join(sub.as_ref()), data.data)?;
        }
        // let template_path = Path::new("crates/automan/assets/templates").join(template);
        // println!("Copying template from {}", template_path.display());
        // copy_dir_all(template_path, path)?;
        Ok(())
    }
}
