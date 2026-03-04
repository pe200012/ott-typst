use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use wasm_minimal_protocol::*;

initiate_protocol!();

const MAX_CACHED_SPECS: usize = 8;

#[derive(Debug)]
struct CachedSyntax {
    spec: String,
    syntax: Rc<ott_core::OttSyntax>,
}

#[derive(Default)]
struct State {
    /// Hash -> bucket (to handle possible hash collisions).
    by_key: HashMap<u64, Vec<CachedSyntax>>,
    /// Insertion order of keys (approximate FIFO eviction).
    order: VecDeque<u64>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn spec_key(spec: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut h = DefaultHasher::new();
    spec.hash(&mut h);
    h.finish()
}

fn compile_syntax(spec_src: &str) -> Result<ott_core::OttSyntax, String> {
    let spec = ott_core::parse_spec(spec_src).map_err(|e| e.to_string())?;
    let checked =
        ott_core::check_spec(spec, &ott_core::OttOptions::default()).map_err(|e| e.to_string())?;
    ott_core::compile_syntax(&checked).map_err(|e| e.to_string())
}

fn get_or_insert_syntax(
    state: &mut State,
    spec_src: &str,
) -> Result<Rc<ott_core::OttSyntax>, String> {
    let key = spec_key(spec_src);

    if let Some(bucket) = state.by_key.get(&key) {
        if let Some(found) = bucket.iter().find(|e| e.spec == spec_src) {
            return Ok(Rc::clone(&found.syntax));
        }
    }

    let syntax = Rc::new(compile_syntax(spec_src)?);

    if !state.by_key.contains_key(&key) {
        state.order.push_back(key);
    }

    state.by_key.entry(key).or_default().push(CachedSyntax {
        spec: spec_src.to_string(),
        syntax: Rc::clone(&syntax),
    });

    while state.order.len() > MAX_CACHED_SPECS {
        if let Some(old_key) = state.order.pop_front() {
            state.by_key.remove(&old_key);
        }
    }

    Ok(syntax)
}

#[wasm_func]
pub fn parse_rules(spec_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let src =
        std::str::from_utf8(spec_bytes).map_err(|e| format!("input is not UTF-8: {e}"))?;

    let spec = ott_core::parse_spec(src).map_err(|e| e.to_string())?;
    let checked =
        ott_core::check_spec(spec, &ott_core::OttOptions::default()).map_err(|e| e.to_string())?;

    let doc = ott_render::render_for_typst(&checked);
    ott_render::to_cbor_bytes(&doc).map_err(|e| e.to_string())
}

/// Parse a snippet of object language according to the grammars defined in `spec_bytes`.
///
/// - `spec_bytes`: UTF-8 encoded Ott spec text.
/// - `root_bytes`: grammar root name (or synonym). If empty, uses default.
/// - `term_bytes`: the term text.
///
/// Returns Typst **math code** (without surrounding `$...$`) as UTF-8 bytes.
#[wasm_func]
pub fn parse_term(
    spec_bytes: &[u8],
    root_bytes: &[u8],
    term_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let spec_src =
        std::str::from_utf8(spec_bytes).map_err(|e| format!("spec is not UTF-8: {e}"))?;
    let root = std::str::from_utf8(root_bytes).map_err(|e| format!("root is not UTF-8: {e}"))?;
    let term = std::str::from_utf8(term_bytes).map_err(|e| format!("term is not UTF-8: {e}"))?;

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let syntax = get_or_insert_syntax(&mut state, spec_src)?;

        let root = root.trim();
        let root = if root.is_empty() {
            syntax
                .default_root()
                .ok_or_else(|| "spec does not define any grammar roots".to_string())?
        } else {
            root
        };

        let code = syntax
            .render_typst_math(root, term)
            .map_err(|e| e.to_string())?;
        Ok(code.into_bytes())
    })
}
