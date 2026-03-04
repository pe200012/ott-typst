use std::path::PathBuf;

use ott_core::{OttOptions, check_spec, parse_spec};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("ott-core is expected at crates/ott-core")
        .to_path_buf()
}

#[test]
fn parse_and_check_tapl_arrow() {
    let path = repo_root().join("fixtures/tapl/arrow.ott");
    let src = std::fs::read_to_string(&path).expect("fixture should be readable");

    let spec = parse_spec(&src).expect("parse should succeed");
    let checked = check_spec(spec, &OttOptions::default()).expect("check should succeed");

    // Expect some grammar roots.
    assert!(checked.grammar_roots.contains("t"));
    assert!(checked.grammar_roots.contains("v"));

    // Expect at least one defn block and a few rules.
    let defn_rules: usize = checked
        .spec
        .items
        .iter()
        .filter_map(|it| match it {
            ott_core::Item::Defn(d) => Some(d.rules.len()),
            _ => None,
        })
        .sum();

    assert!(defn_rules >= 3);
}
