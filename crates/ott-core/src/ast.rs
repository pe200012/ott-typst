#[derive(Debug, Clone)]
pub struct Spec {
    pub items: Vec<Item>,
}

impl Spec {
    #[must_use]
    pub fn new(items: Vec<Item>) -> Self {
        Self { items }
    }
}

#[derive(Debug, Clone)]
pub enum Item {
    Embed(EmbedSection),
    Metavar(MetavarDecl),
    Grammar(GrammarSection),
    Subrules(SubrulesSection),
    Substitutions(SubstitutionsSection),
    Defns(DefnsSection),
    Defn(DefnBlock),

    /// A top-level section we have not modelled yet. We keep it so that the
    /// parser can continue and future phases can implement it without changing
    /// the parser contract.
    UnknownSection(UnknownSection),
}

#[derive(Debug, Clone)]
pub struct UnknownSection {
    pub keyword: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HomBlock {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EmbedSection {
    pub blocks: Vec<HomBlock>,
}

#[derive(Debug, Clone)]
pub struct MetavarDecl {
    pub is_index: bool,
    /// First name is the canonical sort name; remaining are synonyms.
    pub names: Vec<String>,
    pub reps: Vec<HomBlock>,
}

#[derive(Debug, Clone)]
pub struct GrammarSection {
    pub rules: Vec<GrammarRule>,
}

#[derive(Debug, Clone)]
pub struct GrammarRule {
    /// First name is the canonical nonterminal root; remaining are synonyms.
    pub roots: Vec<String>,
    pub sort: String,
    pub comment: Option<String>,
    pub annotations: Vec<HomBlock>,
    pub productions: Vec<Production>,
}

#[derive(Debug, Clone)]
pub struct Production {
    /// The concrete pattern part before the `:: ... :: ...` metadata.
    pub pattern: String,
    /// Optional production meta marker (e.g. `M`).
    pub meta: Option<String>,
    /// Optional constructor/production name.
    pub name: Option<String>,
    /// Optional binding specification body (inside `(+ ... +)`), stored raw.
    pub bind_spec: Option<String>,
    /// Any additional `{{ ... }}` blocks attached to this production.
    pub annotations: Vec<HomBlock>,
    /// Original raw text (may span multiple lines).
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct SubrulesSection {
    pub relations: Vec<Subrule>,
}

#[derive(Debug, Clone)]
pub struct Subrule {
    pub sub: String,
    pub sup: String,
}

#[derive(Debug, Clone)]
pub struct SubstitutionsSection {
    pub entries: Vec<SubstEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstKind {
    Single,
    Multiple,
}

#[derive(Debug, Clone)]
pub struct SubstEntry {
    pub kind: SubstKind,
    pub nonterminal: String,
    pub metavar: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DefnsSection {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DefnBlock {
    pub header: String,
    pub comment: Option<String>,
    pub rules: Vec<InferenceRule>,
}

#[derive(Debug, Clone)]
pub struct InferenceRule {
    pub name: String,
    pub premises: Vec<String>,
    pub conclusion: Vec<String>,
}
