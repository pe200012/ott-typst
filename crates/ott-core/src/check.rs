use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::ast::{Item, Spec};
use crate::error::{OttError, OttResult, Position};

#[derive(Debug, Clone)]
pub struct OttOptions {
    pub strict: bool,
}

impl Default for OttOptions {
    fn default() -> Self {
        Self { strict: true }
    }
}

#[derive(Debug, Clone)]
pub struct CheckedSpec {
    pub spec: Spec,

    /// Grammar roots declared in the `grammar` section(s).
    pub grammar_roots: BTreeSet<String>,

    /// Map root -> first occurrence item index (for stable diagnostics).
    pub grammar_index: BTreeMap<String, usize>,
}

pub fn check_spec(spec: Spec, opts: &OttOptions) -> OttResult<CheckedSpec> {
    let mut roots = BTreeSet::new();
    let mut root_index = BTreeMap::new();

    for (item_idx, item) in spec.items.iter().enumerate() {
        if let Item::Grammar(g) = item {
            for rule in &g.rules {
                for root in &rule.roots {
                    if !roots.insert(root.clone()) {
                        return Err(
                            OttError::new(format!("duplicate grammar root `{}`", root))
                                .with_position(Position::new(item_idx + 1, 1)),
                        );
                    }
                    root_index.entry(root.clone()).or_insert(item_idx);
                }

                if opts.strict {
                    let primary = rule
                        .roots
                        .first()
                        .expect("grammar rule should have at least one root");

                    for prod in &rule.productions {
                        for bs in &prod.bind_specs {
                            if let Err(e) = ott_bind::parse_bind_spec(bs) {
                                return Err(OttError::new(format!(
                                    "invalid binding specification in grammar `{}`: {}",
                                    primary, e
                                ))
                                .with_position(Position::new(item_idx + 1, 1)));
                            }
                        }
                    }
                }
            }
        }
    }

    for (item_idx, item) in spec.items.iter().enumerate() {
        match item {
            Item::Subrules(s) => {
                for rel in &s.relations {
                    if !roots.contains(&rel.sub) {
                        return Err(OttError::new(format!(
                            "subrules references unknown grammar root `{}`",
                            rel.sub
                        ))
                        .with_position(Position::new(item_idx + 1, 1)));
                    }
                    if !roots.contains(&rel.sup) {
                        return Err(OttError::new(format!(
                            "subrules references unknown grammar root `{}`",
                            rel.sup
                        ))
                        .with_position(Position::new(item_idx + 1, 1)));
                    }
                }
            }
            Item::Defn(d) => {
                let mut seen = HashSet::new();
                for r in &d.rules {
                    if r.conclusion.is_empty() {
                        return Err(OttError::new(format!(
                            "inference rule `{}` has empty conclusion",
                            r.name
                        ))
                        .with_position(Position::new(item_idx + 1, 1)));
                    }
                    if !seen.insert(r.name.clone()) {
                        return Err(OttError::new(format!(
                            "duplicate inference rule name `{}` within a defn block",
                            r.name
                        ))
                        .with_position(Position::new(item_idx + 1, 1)));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(CheckedSpec {
        spec,
        grammar_roots: roots,
        grammar_index: root_index,
    })
}
