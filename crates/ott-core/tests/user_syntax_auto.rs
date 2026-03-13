use ott_core::{check_spec, compile_syntax, parse_spec, OttOptions};

#[test]
fn user_syntax_auto_parses_without_root() {
    let spec_src = include_str!("../../../fixtures/tapl/arrow_typst.ott");

    let spec = parse_spec(spec_src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");
    let syntax = compile_syntax(&checked).expect("compile should succeed");

    // Empty root should behave like upstream Ott's `user_syntax`.
    let code = syntax
        .render_typst_math("", "\\x.x")
        .expect("should render");

    assert_eq!(code, "lambda x. x");
}

#[test]
fn user_syntax_reports_ambiguity() {
    let spec_src = r#"
metavar ident, x ::= {{ lex alphanum }}

grammar

a :: A_ ::=
  | x y :: :: Axy {{ typst A }}

b :: B_ ::=
  | x y :: :: Bxy {{ typst B }}
"#;

    let spec = parse_spec(spec_src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");
    let syntax = compile_syntax(&checked).expect("compile should succeed");

    let err = syntax
        .render_typst_math("", "x y")
        .expect_err("should be ambiguous");

    assert!(
        err.to_string().contains("ambiguous snippet"),
        "unexpected error: {err}"
    );
}
