use super::*;

fn fmt(source: &str) -> String {
    format(source.as_bytes(), &Options::default()).unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn indentation_and_spacing() {
    assert_eq!(
        fmt("extends Node\nfunc f(a,b):\n    if a==b :\n        return a+b\n"),
        "extends Node\n\n\nfunc f(a, b):\n\tif a == b:\n\t\treturn a + b\n"
    );
}
