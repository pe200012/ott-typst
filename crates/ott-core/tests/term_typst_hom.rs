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
fn typst_hom_pretty_prints_terms() {
    let path = repo_root().join("fixtures/tapl/arrow_typst.ott");
    let src = std::fs::read_to_string(&path).expect("fixture should be readable");

    let spec = parse_spec(&src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");
    let syntax = compile_syntax(&checked).expect("syntax compile should succeed");

    assert_eq!(
        syntax
            .render_typst_math("t", "\\x.x")
            .expect("render should succeed"),
        "lambda x. x"
    );

    assert_eq!(
        syntax
            .render_typst_math("t", "[x|->x]x")
            .expect("render should succeed"),
        "[ x mapsto x ] x"
    );
}
