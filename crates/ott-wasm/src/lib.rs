use wasm_minimal_protocol::*;

initiate_protocol!();

#[wasm_func]
pub fn parse_rules(spec_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let src = std::str::from_utf8(spec_bytes).map_err(|e| format!("input is not UTF-8: {e}"))?;

    let spec = ott_core::parse_spec(src).map_err(|e| e.to_string())?;
    let checked = ott_core::check_spec(spec, &ott_core::OttOptions::default()).map_err(|e| e.to_string())?;

    let doc = ott_render::render_for_typst(&checked);
    ott_render::to_cbor_bytes(&doc).map_err(|e| e.to_string())
}
