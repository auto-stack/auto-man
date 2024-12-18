
#[cfg(test)]
mod tests {
    use super::*;
    use autolang::parse;
    use autolang::util::pretty;

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
        println!("{}", pretty(&auto.to_string()));
    }
}
