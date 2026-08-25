use voidspace_filter::{Expr, parse, parse_bytes};

#[test]
fn precedence_matches_spec() {
    let expression = parse("size > 1GiB AND NOT attr:system OR ext:zip").unwrap();
    assert!(matches!(expression, Expr::Or(_, _)));
}

#[test]
fn implicit_whitespace_means_and() {
    let expression = parse("ext:zip size>10MiB").unwrap();
    assert!(matches!(expression, Expr::And(_, _)));
}

#[test]
fn parses_decimal_and_binary_sizes() {
    assert_eq!(parse_bytes("1GB").unwrap(), 1_000_000_000);
    assert_eq!(parse_bytes("1GiB").unwrap(), 1_073_741_824);
}
