use ciborium::{de, ser};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypstRenderDoc {
    pub items: Vec<TypstRenderItem>,
}

impl TypstRenderDoc {
    #[must_use]
    pub fn new(items: Vec<TypstRenderItem>) -> Self {
        Self { items }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypstRenderItem {
    Section {
        title: String,
    },

    Grammar {
        nonterminal: String,
        comment: Option<String>,
        alternatives: Vec<String>,
    },

    Rule {
        name: String,
        comment: Option<String>,
        premises: Vec<String>,
        conclusion: String,
    },
}

#[derive(Debug)]
pub struct RenderError {
    message: String,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{msg}", msg = self.message)
    }
}

impl std::error::Error for RenderError {}

impl From<ciborium::ser::Error<std::io::Error>> for RenderError {
    fn from(value: ciborium::ser::Error<std::io::Error>) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}

impl From<ciborium::de::Error<std::io::Error>> for RenderError {
    fn from(value: ciborium::de::Error<std::io::Error>) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}

pub fn to_cbor_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    ser::into_writer(value, &mut out)?;
    Ok(out)
}

pub fn from_cbor_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, RenderError> {
    Ok(de::from_reader(bytes)?)
}

#[must_use]
pub fn render_for_typst(spec: &ott_core::CheckedSpec) -> TypstRenderDoc {
    use ott_core::Item;

    let mut items = Vec::new();

    for item in &spec.spec.items {
        match item {
            Item::Grammar(g) => {
                for r in &g.rules {
                    let alternatives = r
                        .productions
                        .iter()
                        .map(|p| {
                            if let Some(bind) = &p.bind_spec {
                                format!("{} (+ {} +)", p.pattern, bind.trim())
                            } else {
                                p.pattern.clone()
                            }
                        })
                        .collect::<Vec<_>>();

                    let nonterminal = r
                        .roots
                        .first()
                        .expect("grammar rule should have at least one root")
                        .clone();

                    items.push(TypstRenderItem::Grammar {
                        nonterminal,
                        comment: r.comment.clone(),
                        alternatives,
                    });
                }
            }
            Item::Defn(d) => {
                let title = d
                    .comment
                    .clone()
                    .unwrap_or_else(|| d.header.clone());
                items.push(TypstRenderItem::Section { title });

                for rule in &d.rules {
                    items.push(TypstRenderItem::Rule {
                        name: rule.name.clone(),
                        comment: None,
                        premises: rule.premises.clone(),
                        conclusion: rule.conclusion.join(" "),
                    });
                }
            }
            _ => {}
        }
    }

    TypstRenderDoc::new(items)
}
