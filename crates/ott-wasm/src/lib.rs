use std::cell::RefCell;
use std::collections::HashMap;

use serde::Serialize;
use wasm_minimal_protocol::*;

initiate_protocol!();

#[derive(Default)]
struct State {
    next_id: u64,
    syntaxes: HashMap<u64, ott_core::OttSyntax>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[derive(Debug, Clone, Serialize)]
struct CompileResult {
    id: u64,
    roots: Vec<String>,
    default_root: String,
}

#[wasm_func]
pub fn parse_rules(spec_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let src = std::str::from_utf8(spec_bytes).map_err(|e| format!("input is not UTF-8: {e}"))?;

    let spec = ott_core::parse_spec(src).map_err(|e| e.to_string())?;
    let checked =
        ott_core::check_spec(spec, &ott_core::OttOptions::default()).map_err(|e| e.to_string())?;

    let doc = ott_render::render_for_typst(&checked);
    ott_render::to_cbor_bytes(&doc).map_err(|e| e.to_string())
}

/// Compile an Ott spec into an internal syntax environment for parsing
/// object-language snippets.
///
/// Returns a CBOR-encoded dictionary `{ id, roots, default_root }`.
#[wasm_func]
pub fn compile_spec(spec_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let src = std::str::from_utf8(spec_bytes).map_err(|e| format!("input is not UTF-8: {e}"))?;

    let spec = ott_core::parse_spec(src).map_err(|e| e.to_string())?;
    let checked =
        ott_core::check_spec(spec, &ott_core::OttOptions::default()).map_err(|e| e.to_string())?;

    let syntax = ott_core::compile_syntax(&checked).map_err(|e| e.to_string())?;
    let roots = syntax.roots.clone();
    let default_root = syntax
        .default_root()
        .ok_or_else(|| "spec does not define any grammar roots".to_string())?
        .to_string();

    let id = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next_id = state.next_id.saturating_add(1);
        let id = state.next_id;
        state.syntaxes.insert(id, syntax);
        id
    });

    let result = CompileResult {
        id,
        roots,
        default_root,
    };

    ott_render::to_cbor_bytes(&result).map_err(|e| e.to_string())
}

/// Parse a snippet of object language according to a compiled spec.
///
/// - `id_bytes`: decimal id returned from `compile_spec`.
/// - `root_bytes`: grammar root name (or synonym). If empty, uses default.
/// - `term_bytes`: the term text.
///
/// Returns the normalized term text as UTF-8 bytes.
#[wasm_func]
pub fn parse_term(
    id_bytes: &[u8],
    root_bytes: &[u8],
    term_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let id_str = std::str::from_utf8(id_bytes).map_err(|e| format!("id is not UTF-8: {e}"))?;
    let id: u64 = id_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid spec id `{id_str}`: {e}"))?;

    let root = std::str::from_utf8(root_bytes).map_err(|e| format!("root is not UTF-8: {e}"))?;
    let term = std::str::from_utf8(term_bytes).map_err(|e| format!("term is not UTF-8: {e}"))?;

    STATE.with(|state| {
        let state = state.borrow();
        let syntax = state
            .syntaxes
            .get(&id)
            .ok_or_else(|| format!("unknown spec id {id} (did you call compile_spec?)"))?;

        let root = root.trim();
        let root = if root.is_empty() {
            syntax
                .default_root()
                .ok_or_else(|| "spec does not define any grammar roots".to_string())?
        } else {
            root
        };

        let out = syntax.parse(root, term).map_err(|e| e.to_string())?;
        Ok(out.into_bytes())
    })
}
