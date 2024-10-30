use autolang::parse;
use autolang::util::s_form;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_config() {
        let config = r#"
            name: "hello"
            version: "0.1.0"

            exe("hello") {
                dir: "src"
                main: "main.c"
            }
        "#;

        let auto = parse(config).unwrap();
        println!("{}", auto);
        println!("{}", s_form(&auto.to_string()));
    }
}
