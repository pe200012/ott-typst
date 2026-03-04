use std::path::{Path, PathBuf};

use ott_core::{OttOptions, check_spec, parse_spec};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("ott-core is expected at crates/ott-core")
        .to_path_buf()
}

fn collect_ott_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ott_files(&path, out);
            continue;
        }
        if path.extension().is_some_and(|e| e == "ott") {
            out.push(path);
        }
    }
}

#[test]
fn upstream_ott_tests_parse_and_check() {
    let root = repo_root();
    let dir = root.join("fixtures/upstream-ott/tests");
    assert!(dir.is_dir(), "missing upstream tests dir: {dir:?}");

    let mut files = Vec::new();
    collect_ott_files(&dir, &mut files);
    files.sort();

    assert!(!files.is_empty(), "no .ott files found under {dir:?}");

    let expected_fail = ["test7.ott"];

    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<non-utf8>");

        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"));

        if expected_fail.contains(&name) {
            assert!(
                parse_spec(&src).is_err(),
                "{name} is expected to fail parsing, but succeeded",
            );
            continue;
        }

        let spec = parse_spec(&src)
            .unwrap_or_else(|e| panic!("parse failed for {path:?}: {e}"));
        check_spec(spec, &OttOptions::default())
            .unwrap_or_else(|e| panic!("check failed for {path:?}: {e}"));
    }
}
