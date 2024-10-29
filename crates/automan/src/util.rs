pub fn split_first(in_string: &str, sep: char) -> (&str, &str) {
    let mut splitter = in_string.splitn(2, sep);
    let first = splitter.next().unwrap();
    let second = splitter.next().unwrap();
    (first, second)
}

pub fn split_last(in_string: &str, sep: char) -> (&str, &str) {
    let mut splitter = in_string.rsplitn(2, sep);
    let last = splitter.next().unwrap();
    let second = match splitter.next() {
        Some(s) => s,
        None => "",
    };
    (second, last)
}


#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_split_once() {
        let (first, second) = split_first("assets/templates/lib/mylib.h", '/');
        assert_eq!(first, "assets");
        assert_eq!(second, "templates/lib/mylib.h");
    }

    #[test]
    fn test_split_last() {
        let (first, second) = split_last("assets/templates/lib/mylib.h", '/');
        assert_eq!(first, "mylib.h");
        assert_eq!(second, "assets/templates/lib");
    }
}
