use std::path::PathBuf;

use ott_core::{check_spec, parse_spec, OttOptions};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("ott-core is expected at crates/ott-core")
        .to_path_buf()
}

#[test]
fn parse_and_check_test10_synonyms_and_subrules() {
    let path = repo_root().join("fixtures/tests/test10.ott");
    let src = std::fs::read_to_string(&path).expect("fixture should be readable");

    let spec = parse_spec(&src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");

    // grammar roots should include both canonical and synonym roots.
    assert!(checked.grammar_roots.contains("term"));
    assert!(checked.grammar_roots.contains("t"));
    assert!(checked.grammar_roots.contains("val"));
    assert!(checked.grammar_roots.contains("v"));
}
