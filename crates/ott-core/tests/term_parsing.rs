use std::path::PathBuf;

use ott_core::{OttOptions, check_spec, compile_syntax, parse_spec};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("ott-core is expected at crates/ott-core")
        .to_path_buf()
}

#[test]
fn parse_terms_against_tapl_arrow_grammar() {
    let path = repo_root().join("fixtures/tapl/arrow.ott");
    let src = std::fs::read_to_string(&path).expect("fixture should be readable");

    let spec = parse_spec(&src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");

    let syntax = compile_syntax(&checked).expect("syntax compile should succeed");
    let root = syntax
        .default_root()
        .expect("should have at least one root");
    assert_eq!(root, "t");

    // variable
    assert!(syntax.parse("t", "x").is_ok());

    // abstraction (with and without spaces)
    assert!(syntax.parse("t", "\\ x . x").is_ok());
    assert!(syntax.parse("t", "\\x.x").is_ok());

    // application
    assert!(syntax.parse("t", "x x").is_ok());

    // substitution (no spaces)
    assert!(syntax.parse("t", "[x|->x]x").is_ok());

    // should reject junk
    assert!(syntax.parse("t", "@").is_err());
}
