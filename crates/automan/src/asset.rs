use rust_embed::*;
use crate::AutoError;


#[derive(Embed)]
#[folder = "assets/templates"]
pub struct Templates;


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
