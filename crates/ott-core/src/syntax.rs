use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::ast::{HomBlock, Item};
use crate::check::CheckedSpec;
use crate::error::{OttError, OttResult, Span};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Sym {
    Terminal(String),
    Nonterminal(String),
    Metavar(String),
}

#[derive(Debug, Clone)]
struct Prod {
    lhs: String,
    rhs: Vec<Sym>,
}

#[derive(Debug, Default, Clone)]
struct Grammar {
    prods: Vec<Prod>,
    by_lhs: HashMap<String, Vec<usize>>,
}

impl Grammar {
    fn add_prod(&mut self, lhs: String, rhs: Vec<Sym>) {
        let id = self.prods.len();
        self.prods.push(Prod {
            lhs: lhs.clone(),
            rhs,
        });
        self.by_lhs.entry(lhs).or_default().push(id);
    }

    fn prods_for(&self, lhs: &str) -> impl Iterator<Item = usize> + '_ {
        self.by_lhs
            .get(lhs)
            .into_iter()
            .flat_map(|v| v.iter().copied())
    }
}

#[derive(Debug, Clone)]
enum LexPattern {
    Builtin(BuiltinLex),
    Regex(Regex),
}

impl LexPattern {
    fn matches(&self, s: &str) -> bool {
        match self {
            Self::Builtin(b) => b.matches(s),
            Self::Regex(re) => re.is_match(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinLex {
    Alphanum,
    AlphanumUpper,
    Numeral,
    Numeric,
}

impl BuiltinLex {
    fn matches(self, s: &str) -> bool {
        match self {
            Self::Alphanum => {
                let mut it = s.bytes();
                let Some(first) = it.next() else {
                    return false;
                };
                if !(matches!(first, b'a'..=b'z' | b'_')) {
                    return false;
                }
                it.all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'\''))
            }
            Self::AlphanumUpper => {
                let mut it = s.bytes();
                let Some(first) = it.next() else {
                    return false;
                };
                if !(matches!(first, b'A'..=b'Z' | b'_')) {
                    return false;
                }
                it.all(|b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'\''))
            }
            Self::Numeral | Self::Numeric => {
                // Both are treated as (optional -) + digits.
                let s = s.as_bytes();
                if s.is_empty() {
                    return false;
                }
                let mut i = 0;
                if s[0] == b'-' {
                    i += 1;
                }
                if i >= s.len() {
                    return false;
                }
                s[i..].iter().all(|b| matches!(b, b'0'..=b'9'))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MetavarDef {
    lex: LexPattern,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Terminal(String),
    Word(String),
}

/// A compiled syntax environment that can be used to parse object-language
/// snippets using the grammars defined in an Ott spec.
#[derive(Debug, Clone)]
pub struct OttSyntax {
    /// Canonical grammar roots in stable order of appearance.
    pub roots: Vec<String>,

    root_alias: HashMap<String, String>,
    metavars: HashMap<String, MetavarDef>,

    grammar: Grammar,
    terminals: Vec<String>,
}

impl OttSyntax {
    #[must_use]
    pub fn default_root(&self) -> Option<&str> {
        self.roots.first().map(String::as_str)
    }

    /// Resolve a grammar root name or synonym to its canonical root name.
    #[must_use]
    pub fn resolve_root(&self, name: &str) -> Option<&str> {
        self.root_alias.get(name).map(String::as_str)
    }

    /// Parse (recognize) `input` as the grammar root `root`.
    ///
    /// On success, returns a normalized string (currently: trimmed input).
    pub fn parse(&self, root: &str, input: &str) -> OttResult<String> {
        let root = self
            .resolve_root(root)
            .ok_or_else(|| OttError::new(format!("unknown grammar root `{}`", root)))?;

        let toks = self.lex_input(input)?;
        earley_recognize(root, &self.grammar, &self.metavars, &toks, input)
            .map(|()| input.trim().to_string())
    }

    fn lex_input(&self, src: &str) -> OttResult<Vec<Token>> {
        let bytes = src.as_bytes();
        let mut out = Vec::new();
        let mut i = 0usize;

        while i < bytes.len() {
            // skip whitespace
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }

            // try terminals (longest first)
            if let Some((term, end)) = self.match_terminal(src, i) {
                out.push(Token {
                    kind: TokenKind::Terminal(term.to_string()),
                    span: Span::new(i, end),
                });
                i = end;
                continue;
            }

            // number literal
            if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
                let start = i;
                if bytes[i] == b'-' {
                    if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                        i += 1;
                    }
                }
                if bytes[i].is_ascii_digit() {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let text = &src[start..i];
                    out.push(Token {
                        kind: TokenKind::Word(text.to_string()),
                        span: Span::new(start, i),
                    });
                    continue;
                }
                // fall through to error
                i = start;
            }

            // identifier-like word
            if is_ident_start(bytes[i]) {
                let start = i;
                i += 1;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                let text = &src[start..i];
                out.push(Token {
                    kind: TokenKind::Word(text.to_string()),
                    span: Span::new(start, i),
                });
                continue;
            }

            return Err(OttError::new(format!(
                "unexpected character `{}`",
                src[i..].chars().next().unwrap_or('\u{FFFD}')
            ))
            .with_span(Span::new(i, i + 1), src));
        }

        Ok(out)
    }

    fn match_terminal<'a>(&'a self, src: &str, start: usize) -> Option<(&'a str, usize)> {
        let rest = &src[start..];

        for t in &self.terminals {
            if t.is_empty() {
                continue;
            }

            if !rest.starts_with(t) {
                continue;
            }

            // For word-like terminals (keywords), require a boundary so we don't
            // split identifiers like `letx` into `let` + `x`.
            if is_word_like_terminal(t) {
                let end = start + t.len();
                let next = src.as_bytes().get(end).copied();
                if next.is_some_and(is_ident_continue) {
                    continue;
                }
            }

            return Some((t.as_str(), start + t.len()));
        }

        None
    }
}

fn is_ident_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_')
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || matches!(b, b'0'..=b'9' | b'\'')
}

fn is_word_like_terminal(t: &str) -> bool {
    let b = t.as_bytes();
    b.first().copied().is_some_and(is_ident_start)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct State {
    prod_id: usize,
    dot: usize,
    origin: usize,
}

fn earley_recognize(
    start: &str,
    grammar: &Grammar,
    metavars: &HashMap<String, MetavarDef>,
    tokens: &[Token],
    src: &str,
) -> OttResult<()> {
    let n = tokens.len();
    let mut chart: Vec<HashSet<State>> = vec![HashSet::new(); n + 1];

    for pid in grammar.prods_for(start) {
        chart[0].insert(State {
            prod_id: pid,
            dot: 0,
            origin: 0,
        });
    }

    for i in 0..=n {
        // closure: prediction + completion until no new states
        let mut changed = true;
        while changed {
            changed = false;
            let states: Vec<State> = chart[i].iter().cloned().collect();
            for st in states {
                let prod = &grammar.prods[st.prod_id];
                if st.dot < prod.rhs.len() {
                    match &prod.rhs[st.dot] {
                        Sym::Nonterminal(nt) => {
                            for pid in grammar.prods_for(nt) {
                                let added = chart[i].insert(State {
                                    prod_id: pid,
                                    dot: 0,
                                    origin: i,
                                });
                                changed |= added;
                            }
                        }
                        Sym::Terminal(_) | Sym::Metavar(_) => {}
                    }
                } else {
                    // completion: advance all items in chart[origin] that expect this lhs
                    let origin_states: Vec<State> = chart[st.origin].iter().cloned().collect();
                    for prev in origin_states {
                        let prev_prod = &grammar.prods[prev.prod_id];
                        if prev.dot < prev_prod.rhs.len() {
                            if prev_prod.rhs[prev.dot] == Sym::Nonterminal(prod.lhs.clone()) {
                                let added = chart[i].insert(State {
                                    prod_id: prev.prod_id,
                                    dot: prev.dot + 1,
                                    origin: prev.origin,
                                });
                                changed |= added;
                            }
                        }
                    }
                }
            }
        }

        if i == n {
            break;
        }

        // scanning step
        let tok = &tokens[i];
        let states: Vec<State> = chart[i].iter().cloned().collect();
        for st in states {
            let prod = &grammar.prods[st.prod_id];
            if st.dot >= prod.rhs.len() {
                continue;
            }

            let next = &prod.rhs[st.dot];
            let matches = match next {
                Sym::Terminal(t) => tok.kind == TokenKind::Terminal(t.clone()),
                Sym::Metavar(sort) => {
                    let Some(def) = metavars.get(sort) else {
                        return Err(OttError::new(format!(
                            "missing lex specification for metavariable sort `{}`",
                            sort
                        )));
                    };
                    match &tok.kind {
                        TokenKind::Word(w) => def.lex.matches(w),
                        TokenKind::Terminal(_) => false,
                    }
                }
                Sym::Nonterminal(_) => false,
            };

            if matches {
                chart[i + 1].insert(State {
                    prod_id: st.prod_id,
                    dot: st.dot + 1,
                    origin: st.origin,
                });
            }
        }
    }

    // accept if any start production completed at end
    let ok = chart[n].iter().any(|st| {
        let prod = &grammar.prods[st.prod_id];
        st.origin == 0 && prod.lhs == start && st.dot == prod.rhs.len()
    });

    if ok {
        return Ok(());
    }

    // diagnose: find farthest i with non-empty chart
    let mut farthest = 0usize;
    for i in 0..=n {
        if !chart[i].is_empty() {
            farthest = i;
        }
    }

    let span = if farthest < n {
        tokens[farthest].span.clone()
    } else {
        // EOF
        Span::new(src.len(), src.len())
    };

    Err(OttError::new(format!("failed to parse input as `{}`", start)).with_span(span, src))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatToken {
    Atom(Sym),
    Dots(u8),
}

struct GrammarBuilder {
    root_alias: HashMap<String, String>,
    metavar_alias: HashMap<String, String>,
    grammar: Grammar,
    terminals: HashSet<String>,
    list_counter: usize,
}

impl GrammarBuilder {
    fn new(root_alias: HashMap<String, String>, metavar_alias: HashMap<String, String>) -> Self {
        Self {
            root_alias,
            metavar_alias,
            grammar: Grammar::default(),
            terminals: HashSet::new(),
            list_counter: 0,
        }
    }

    fn finish(self) -> (Grammar, Vec<String>) {
        let mut terms = self.terminals.into_iter().collect::<Vec<_>>();
        terms.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        (self.grammar, terms)
    }

    fn add_prod(&mut self, lhs: &str, rhs: Vec<Sym>) {
        for s in &rhs {
            if let Sym::Terminal(t) = s {
                if !t.is_empty() {
                    self.terminals.insert(t.clone());
                }
            }
        }
        self.grammar.add_prod(lhs.to_string(), rhs);
    }

    fn compile_production(&mut self, lhs: &str, pattern: &str) -> OttResult<Vec<Sym>> {
        let raw_tokens = pattern.split_whitespace().collect::<Vec<_>>();
        let mut toks = self.compile_pattern_tokens(&raw_tokens)?;

        // Expand all dot-form lists.
        while let Some(idx) = toks.iter().position(|t| matches!(t, PatToken::Dots(_))) {
            toks = self.expand_dots_once(&toks, idx)?;
        }

        let rhs = toks
            .into_iter()
            .map(|t| match t {
                PatToken::Atom(s) => s,
                PatToken::Dots(_) => unreachable!("dots should be fully expanded"),
            })
            .collect::<Vec<_>>();

        self.add_prod(lhs, rhs.clone());
        Ok(rhs)
    }

    fn compile_pattern_tokens(&mut self, raw_tokens: &[&str]) -> OttResult<Vec<PatToken>> {
        let mut out = Vec::new();
        let mut i = 0usize;
        while i < raw_tokens.len() {
            let tok = raw_tokens[i];
            match tok {
                ".." => {
                    out.push(PatToken::Dots(2));
                    i += 1;
                }
                "..." => {
                    out.push(PatToken::Dots(3));
                    i += 1;
                }
                "...." => {
                    out.push(PatToken::Dots(4));
                    i += 1;
                }
                "</" => {
                    let (list_sym, next_i) = self.compile_comp_form(raw_tokens, i)?;
                    out.push(PatToken::Atom(list_sym));
                    i = next_i;
                }
                _ => {
                    out.push(PatToken::Atom(self.classify_atom(tok)));
                    i += 1;
                }
            }
        }
        Ok(out)
    }

    fn compile_comp_form(&mut self, raw_tokens: &[&str], start: usize) -> OttResult<(Sym, usize)> {
        // Syntax (subset):
        //   </ item... // bound... />
        //   </ item... // sep // bound... />
        // We ignore the bound part; only use sep (if present).
        let mut i = start + 1; // skip "</"
        let mut item_tokens = Vec::new();

        while i < raw_tokens.len() {
            if raw_tokens[i] == "//" {
                break;
            }
            item_tokens.push(raw_tokens[i]);
            i += 1;
        }

        if i >= raw_tokens.len() || raw_tokens[i] != "//" {
            return Err(OttError::new(
                "unterminated comprehension form: expected `//`",
            ));
        }
        i += 1; // skip first "//"

        if i >= raw_tokens.len() {
            return Err(OttError::new(
                "unterminated comprehension form: expected bound or separator",
            ));
        }

        let mut sep: Option<String> = None;
        if i + 1 < raw_tokens.len() && raw_tokens[i + 1] == "//" {
            sep = Some(dequote(raw_tokens[i]).to_string());
            i += 2; // skip sep and second "//"
        }

        let mut found_end = false;
        while i < raw_tokens.len() {
            if raw_tokens[i] == "/>" {
                i += 1; // consume "/>"
                found_end = true;
                break;
            }
            i += 1;
        }

        if !found_end {
            return Err(OttError::new(
                "unterminated comprehension form: expected `/>`",
            ));
        }

        // Compile the item pattern (no dot expansion inside, for now).
        let item = item_tokens
            .iter()
            .map(|t| self.classify_atom(t))
            .collect::<Vec<_>>();

        if item.is_empty() {
            return Err(OttError::new("empty comprehension item pattern"));
        }

        let list_nt = self.fresh_list_nonterminal();
        self.define_list_nonterminal(&list_nt, &item, sep.as_deref());

        Ok((Sym::Nonterminal(list_nt), i))
    }

    fn fresh_list_nonterminal(&mut self) -> String {
        self.list_counter += 1;
        format!("__ott_list_{}", self.list_counter)
    }

    fn define_list_nonterminal(&mut self, list_nt: &str, item: &[Sym], sep: Option<&str>) {
        let tail = format!("{list_nt}__tail");

        // list_nt -> item tail
        let mut rhs = item.to_vec();
        rhs.push(Sym::Nonterminal(tail.clone()));
        self.add_prod(list_nt, rhs);

        // tail -> ε
        self.add_prod(&tail, Vec::new());

        // tail -> (sep?) item tail
        let mut rhs2 = Vec::new();
        if let Some(sep) = sep {
            rhs2.push(Sym::Terminal(sep.to_string()));
        }
        rhs2.extend_from_slice(item);
        rhs2.push(Sym::Nonterminal(tail));
        self.add_prod(&format!("{list_nt}__tail"), rhs2);
    }

    fn expand_dots_once(&mut self, toks: &[PatToken], idx: usize) -> OttResult<Vec<PatToken>> {
        let PatToken::Dots(_dots) = &toks[idx] else {
            return Ok(toks.to_vec());
        };

        let (sep, left, right) = if idx >= 1
            && idx + 1 < toks.len()
            && matches!(&toks[idx - 1], PatToken::Atom(Sym::Terminal(_)))
            && toks[idx - 1] == toks[idx + 1]
        {
            let PatToken::Atom(Sym::Terminal(sep)) = &toks[idx - 1] else {
                unreachable!();
            };
            (Some(sep.clone()), &toks[..idx - 1], &toks[idx + 2..])
        } else {
            (None, &toks[..idx], &toks[idx + 1..])
        };

        // We require atoms on both sides.
        let left_atoms = left
            .iter()
            .filter_map(|t| match t {
                PatToken::Atom(a) => Some(a.clone()),
                PatToken::Dots(_) => None,
            })
            .collect::<Vec<_>>();

        let right_atoms = right
            .iter()
            .filter_map(|t| match t {
                PatToken::Atom(a) => Some(a.clone()),
                PatToken::Dots(_) => None,
            })
            .collect::<Vec<_>>();

        let max_k = left_atoms.len().min(right_atoms.len());
        let mut k = None;
        for cand in (1..=max_k).rev() {
            let lstart = left_atoms.len() - cand;
            let mut ok = true;
            for j in 0..cand {
                if !pat_compatible(&left_atoms[lstart + j], &right_atoms[j]) {
                    ok = false;
                    break;
                }
            }
            if ok {
                k = Some(cand);
                break;
            }
        }

        let Some(k) = k else {
            return Err(OttError::new(
                "cannot interpret dot-form list: failed to align left/right patterns",
            ));
        };

        let prefix = left_atoms[..left_atoms.len() - k]
            .iter()
            .cloned()
            .map(PatToken::Atom)
            .collect::<Vec<_>>();
        let item = left_atoms[left_atoms.len() - k..]
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let suffix = right_atoms[k..]
            .iter()
            .cloned()
            .map(PatToken::Atom)
            .collect::<Vec<_>>();

        let list_nt = self.fresh_list_nonterminal();
        self.define_list_nonterminal(&list_nt, &item, sep.as_deref());

        let mut out = Vec::new();
        out.extend(prefix);
        out.push(PatToken::Atom(Sym::Nonterminal(list_nt)));
        out.extend(suffix);
        Ok(out)
    }

    fn classify_atom(&self, raw: &str) -> Sym {
        let tok = raw.trim();

        if tok.starts_with('\'') && tok.ends_with('\'') && tok.len() >= 2 {
            return Sym::Terminal(dequote(tok).to_string());
        }

        if tok == "" {
            return Sym::Terminal(String::new());
        }

        // Exact alias match first.
        if let Some(canon) = self.root_alias.get(tok) {
            return Sym::Nonterminal(canon.clone());
        }
        if let Some(canon) = self.metavar_alias.get(tok) {
            return Sym::Metavar(canon.clone());
        }

        // Try index suffixes.
        if let Some((base, _suffix)) = split_index_suffix(tok) {
            if let Some(canon) = self.root_alias.get(base) {
                return Sym::Nonterminal(canon.clone());
            }
            if let Some(canon) = self.metavar_alias.get(base) {
                return Sym::Metavar(canon.clone());
            }
        }

        Sym::Terminal(dequote(tok).to_string())
    }
}

fn dequote(s: &str) -> &str {
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn split_index_suffix(s: &str) -> Option<(&str, &str)> {
    if let Some((base, suffix)) = s.split_once('_') {
        if !base.is_empty() && !suffix.is_empty() {
            return Some((base, suffix));
        }
    }

    // digits
    if let Some(pos) = s.as_bytes().iter().rposition(|b| !matches!(b, b'0'..=b'9')) {
        if pos + 1 < s.len() {
            let base = &s[..=pos];
            let suffix = &s[pos + 1..];
            if !suffix.is_empty() && suffix.bytes().all(|b| matches!(b, b'0'..=b'9')) {
                return Some((base, suffix));
            }
        }
    }

    // trailing apostrophes
    if let Some(pos) = s.as_bytes().iter().rposition(|b| *b != b'\'') {
        if pos + 1 < s.len() {
            let base = &s[..=pos];
            let suffix = &s[pos + 1..];
            if suffix.bytes().all(|b| b == b'\'') {
                return Some((base, suffix));
            }
        }
    }

    // single letter index
    if s.len() >= 2 {
        let last = s.as_bytes()[s.len() - 1];
        if last.is_ascii_alphabetic() {
            let base = &s[..s.len() - 1];
            let suffix = &s[s.len() - 1..];
            return Some((base, suffix));
        }
    }

    None
}

fn pat_compatible(a: &Sym, b: &Sym) -> bool {
    a == b
}

fn parse_lex_block(blocks: &[HomBlock]) -> Option<LexPattern> {
    let blk = blocks.iter().find(|b| b.name == "lex")?;
    let body = blk.body.trim();
    if body.is_empty() {
        return None;
    }

    match body {
        "alphanum" => Some(LexPattern::Builtin(BuiltinLex::Alphanum)),
        "Alphanum" => Some(LexPattern::Builtin(BuiltinLex::AlphanumUpper)),
        "numeral" => Some(LexPattern::Builtin(BuiltinLex::Numeral)),
        "numeric" => Some(LexPattern::Builtin(BuiltinLex::Numeric)),
        other => {
            // Treat as raw regex (Ott uses OCaml-style sometimes; we accept Rust regex here).
            let re = Regex::new(&format!("^(?:{})$", other)).ok()?;
            Some(LexPattern::Regex(re))
        }
    }
}

/// Compile an Ott spec into a syntax environment usable for parsing object
/// language snippets.
pub fn compile_syntax(spec: &CheckedSpec) -> OttResult<OttSyntax> {
    let mut root_alias: HashMap<String, String> = HashMap::new();
    let mut roots = Vec::new();

    for item in &spec.spec.items {
        if let Item::Grammar(g) = item {
            for rule in &g.rules {
                let canon = rule
                    .roots
                    .first()
                    .ok_or_else(|| OttError::new("grammar rule missing root"))?
                    .clone();

                if !root_alias.contains_key(&canon) {
                    roots.push(canon.clone());
                }

                for r in &rule.roots {
                    root_alias.insert(r.clone(), canon.clone());
                }
            }
        }
    }

    let mut metavar_alias: HashMap<String, String> = HashMap::new();
    let mut metavars: HashMap<String, MetavarDef> = HashMap::new();

    for item in &spec.spec.items {
        if let Item::Metavar(mv) = item {
            let canon = mv
                .names
                .first()
                .ok_or_else(|| OttError::new("metavar decl missing name"))?
                .clone();

            for name in &mv.names {
                metavar_alias.insert(name.clone(), canon.clone());
            }

            if metavars.contains_key(&canon) {
                continue;
            }

            if let Some(lex) = parse_lex_block(&mv.reps) {
                metavars.insert(canon.clone(), MetavarDef { lex });
            } else {
                // Default to `alphanum` if absent; this matches common Ott usage
                // and keeps the parser usable for simplified specs.
                metavars.insert(
                    canon.clone(),
                    MetavarDef {
                        lex: LexPattern::Builtin(BuiltinLex::Alphanum),
                    },
                );
            }
        }
    }

    let mut builder = GrammarBuilder::new(root_alias.clone(), metavar_alias.clone());

    for item in &spec.spec.items {
        if let Item::Grammar(g) = item {
            for rule in &g.rules {
                let canon = rule
                    .roots
                    .first()
                    .ok_or_else(|| OttError::new("grammar rule missing root"))?
                    .clone();

                for prod in &rule.productions {
                    let _rhs = builder.compile_production(&canon, &prod.pattern)?;
                }
            }
        }
    }

    let (grammar, terminals) = builder.finish();

    Ok(OttSyntax {
        roots,
        root_alias,
        metavars,
        grammar,
        terminals,
    })
}
