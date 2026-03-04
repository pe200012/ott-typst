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
struct RhsItem {
    sym: Sym,

    /// Label used by template holes like `[[t1]]`.
    ///
    /// Only nonterminal/metavar occurrences carry labels.
    label: Option<String>,
}

#[derive(Debug, Clone)]
struct Template {
    parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone)]
enum TemplatePart {
    Lit(String),
    Hole(String),
}

impl Template {
    fn parse(src: &str) -> OttResult<Self> {
        let mut parts = Vec::new();
        let mut i = 0usize;

        while let Some(rel) = src[i..].find("[[") {
            let start = i + rel;
            if start > i {
                parts.push(TemplatePart::Lit(src[i..start].to_string()));
            }

            let after = start + 2;
            let Some(rel_end) = src[after..].find("]]") else {
                return Err(OttError::new("unclosed `[[` in typst template"));
            };
            let end = after + rel_end;
            let key = src[after..end].trim();
            if key.is_empty() {
                return Err(OttError::new("empty `[[...]]` hole in typst template"));
            }
            parts.push(TemplatePart::Hole(key.to_string()));
            i = end + 2;
        }

        if i < src.len() {
            parts.push(TemplatePart::Lit(src[i..].to_string()));
        }

        Ok(Self { parts })
    }

    fn render(&self, env: &HashMap<String, String>) -> OttResult<String> {
        let mut out = String::new();
        for p in &self.parts {
            match p {
                TemplatePart::Lit(s) => out.push_str(s),
                TemplatePart::Hole(k) => {
                    let v = env.get(k).ok_or_else(|| {
                        OttError::new(format!(
                            "unknown typst template hole `[[{}]]` (available: {})",
                            k,
                            env.keys().cloned().collect::<Vec<_>>().join(", ")
                        ))
                    })?;
                    out.push_str(v);
                }
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
struct Prod {
    lhs: String,
    rhs: Vec<RhsItem>,
    typst: Option<Template>,
}

#[derive(Debug, Default, Clone)]
struct Grammar {
    prods: Vec<Prod>,
    by_lhs: HashMap<String, Vec<usize>>,
}

impl Grammar {
    fn add_prod(&mut self, lhs: String, rhs: Vec<RhsItem>, typst: Option<Template>) -> usize {
        let id = self.prods.len();
        self.prods.push(Prod {
            lhs: lhs.clone(),
            rhs,
            typst,
        });
        self.by_lhs.entry(lhs).or_default().push(id);
        id
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
    typst: Option<Template>,
    /// Canonical + synonyms, all dequoted.
    names: Vec<String>,
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

#[derive(Debug, Clone)]
enum TermNode {
    Terminal {
        text: String,
    },

    Metavar {
        sort: String,
        text: String,
    },

    Nonterminal {
        prod_id: usize,
        children: Vec<TermNode>,
        span: Span,
    },
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

    /// Parse `input` and pretty-print it using `{{ typst ... }}` hom templates.
    ///
    /// Returns Typst **math code** (without surrounding `$...$`).
    pub fn render_typst_math(&self, root: &str, input: &str) -> OttResult<String> {
        let root = self
            .resolve_root(root)
            .ok_or_else(|| OttError::new(format!("unknown grammar root `{}`", root)))?;

        let toks = self.lex_input(input)?;
        let tree = earley_parse(root, &self.grammar, &self.metavars, &toks, input)?;
        self.render_node(&tree, input)
    }

    fn render_node(&self, node: &TermNode, src: &str) -> OttResult<String> {
        match node {
            TermNode::Terminal { text } => Ok(typst_quote_text(text)),

            TermNode::Metavar { sort, text } => self.render_metavar(sort, text),

            TermNode::Nonterminal {
                prod_id,
                children,
                span,
            } => {
                let prod = self
                    .grammar
                    .prods
                    .get(*prod_id)
                    .ok_or_else(|| OttError::new("internal error: invalid production id"))?;

                if let Some(tmpl) = &prod.typst {
                    let mut env: HashMap<String, String> = HashMap::new();

                    for (rhs_item, child) in prod.rhs.iter().zip(children) {
                        let Some(label) = &rhs_item.label else {
                            continue;
                        };

                        let value = match &rhs_item.sym {
                            Sym::Metavar(sort) => self.render_metavar(
                                sort,
                                match child {
                                    TermNode::Metavar { text, .. } => text,
                                    _ => {
                                        return Err(OttError::new(
                                            "internal error: metavar hole did not bind a metavar",
                                        ));
                                    }
                                },
                            ),
                            Sym::Nonterminal(_) => self.render_node(child, src),
                            Sym::Terminal(_) => {
                                return Err(OttError::new(
                                    "internal error: terminal symbols must not have labels",
                                ));
                            }
                        }?;

                        if env.insert(label.clone(), value).is_some() {
                            return Err(OttError::new(format!(
                                "duplicate label `{}` in production (cannot be used in typst template)",
                                label
                            )));
                        }
                    }

                    tmpl.render(&env)
                } else {
                    // Fallback: show the original source slice as math text.
                    let slice = span_to_str(span.clone(), src)?;
                    Ok(typst_quote_text(slice.trim()))
                }
            }
        }
    }

    fn render_metavar(&self, sort: &str, text: &str) -> OttResult<String> {
        let def = self.metavars.get(sort).ok_or_else(|| {
            OttError::new(format!(
                "internal error: missing metavariable sort `{}`",
                sort
            ))
        })?;

        let rendered = typst_render_ident(text.trim());

        if let Some(tmpl) = &def.typst {
            let mut env = HashMap::new();
            for n in &def.names {
                env.insert(n.clone(), rendered.clone());
            }
            tmpl.render(&env)
        } else {
            Ok(rendered)
        }
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
                    kind: TokenKind::Terminal(term),
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

    fn match_terminal(&self, src: &str, start: usize) -> Option<(String, usize)> {
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

            return Some((t.clone(), start + t.len()));
        }

        None
    }
}

fn span_to_str<'a>(span: Span, src: &'a str) -> OttResult<&'a str> {
    src.get(span.start..span.end)
        .ok_or_else(|| OttError::new("internal error: invalid span"))
}

fn typst_quote_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn typst_render_ident(s: &str) -> String {
    if s.is_empty() {
        return typst_quote_text("");
    }

    // numbers
    if s.bytes().all(|b| b.is_ascii_digit())
        || (s.starts_with('-') && s.len() > 1 && s[1..].bytes().all(|b| b.is_ascii_digit()))
    {
        return s.to_string();
    }

    // Single ASCII letter (common case for Ott metavars).
    if s.len() == 1 && s.as_bytes()[0].is_ascii_alphabetic() {
        return s.to_string();
    }

    // Single letter + primes.
    if s.len() >= 2
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.as_bytes()[1..].iter().all(|b| *b == b'\'')
    {
        return s.to_string();
    }

    // x_12 or x_12''
    if let Some((base, rest)) = s.split_once('_') {
        if base.len() == 1 && base.as_bytes()[0].is_ascii_alphabetic() {
            let (digits, _primes) = rest.split_at(rest.trim_end_matches('\'').len());
            let primes = &rest[digits.len()..];
            if !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && primes.bytes().all(|b| b == b'\'')
            {
                return s.to_string();
            }
        }
    }

    // x12 or x12'' -> x_12''
    if s.len() >= 2 && s.as_bytes()[0].is_ascii_alphabetic() {
        let mut split = 1usize;
        while split < s.len() && s.as_bytes()[split].is_ascii_digit() {
            split += 1;
        }
        if split > 1 {
            let (digits, primes) = s[1..].split_at(split - 1);
            if !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit())
                && primes.bytes().all(|b| b == b'\'')
            {
                return format!("{}_{}{}", &s[..1], digits, primes);
            }
        }
    }

    // Fallback: render as text in math.
    typst_quote_text(s)
}

fn is_ident_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_')
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || matches!(b, b'0'..=b'9' | b'\'')
}

fn is_word_like_terminal(t: &str) -> bool {
    t.as_bytes().first().copied().is_some_and(is_ident_start)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    prod_id: usize,
    dot: usize,
    origin: usize,
}

#[derive(Debug, Clone)]
enum BackPtr {
    Scan { prev: State, token_index: usize },
    Complete { prev: State, completed: State },
}

#[derive(Debug, Default, Clone)]
struct Chart {
    states: HashSet<State>,
    back: HashMap<State, Vec<BackPtr>>,
}

impl Chart {
    fn insert(&mut self, st: State, bp: Option<BackPtr>) -> bool {
        let is_new = self.states.insert(st);
        if let Some(bp) = bp {
            self.back.entry(st).or_default().push(bp);
        }
        is_new
    }
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
            let states: Vec<State> = chart[i].iter().copied().collect();
            for st in states {
                let prod = &grammar.prods[st.prod_id];
                if st.dot < prod.rhs.len() {
                    match &prod.rhs[st.dot].sym {
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
                    let origin_states: Vec<State> = chart[st.origin].iter().copied().collect();
                    for prev in origin_states {
                        let prev_prod = &grammar.prods[prev.prod_id];
                        if prev.dot < prev_prod.rhs.len() {
                            if prev_prod.rhs[prev.dot].sym == Sym::Nonterminal(prod.lhs.clone()) {
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
        let states: Vec<State> = chart[i].iter().copied().collect();
        for st in states {
            let prod = &grammar.prods[st.prod_id];
            if st.dot >= prod.rhs.len() {
                continue;
            }

            let next = &prod.rhs[st.dot].sym;
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

fn earley_parse(
    start: &str,
    grammar: &Grammar,
    metavars: &HashMap<String, MetavarDef>,
    tokens: &[Token],
    src: &str,
) -> OttResult<TermNode> {
    let n = tokens.len();
    let mut chart: Vec<Chart> = vec![Chart::default(); n + 1];

    for pid in grammar.prods_for(start) {
        chart[0].insert(
            State {
                prod_id: pid,
                dot: 0,
                origin: 0,
            },
            None,
        );
    }

    for i in 0..=n {
        // closure: prediction + completion until no new states
        let mut changed = true;
        while changed {
            changed = false;
            let states: Vec<State> = chart[i].states.iter().copied().collect();
            for st in states {
                let prod = &grammar.prods[st.prod_id];
                if st.dot < prod.rhs.len() {
                    if let Sym::Nonterminal(nt) = &prod.rhs[st.dot].sym {
                        for pid in grammar.prods_for(nt) {
                            let added = chart[i].insert(
                                State {
                                    prod_id: pid,
                                    dot: 0,
                                    origin: i,
                                },
                                None,
                            );
                            changed |= added;
                        }
                    }
                } else {
                    // completion
                    let lhs = prod.lhs.clone();
                    let origin = st.origin;
                    let origin_states: Vec<State> = chart[origin].states.iter().copied().collect();
                    for prev in origin_states {
                        let prev_prod = &grammar.prods[prev.prod_id];
                        if prev.dot >= prev_prod.rhs.len() {
                            continue;
                        }

                        if prev_prod.rhs[prev.dot].sym == Sym::Nonterminal(lhs.clone()) {
                            let adv = State {
                                prod_id: prev.prod_id,
                                dot: prev.dot + 1,
                                origin: prev.origin,
                            };
                            let added = chart[i].insert(
                                adv,
                                Some(BackPtr::Complete {
                                    prev,
                                    completed: st,
                                }),
                            );
                            if !added {
                                chart[i]
                                    .back
                                    .entry(adv)
                                    .or_default()
                                    .push(BackPtr::Complete {
                                        prev,
                                        completed: st,
                                    });
                            }
                            changed |= added;
                        }
                    }
                }
            }
        }

        if i == n {
            break;
        }

        // scanning
        let tok = &tokens[i];
        let states: Vec<State> = chart[i].states.iter().copied().collect();
        for st in states {
            let prod = &grammar.prods[st.prod_id];
            if st.dot >= prod.rhs.len() {
                continue;
            }

            let next = &prod.rhs[st.dot].sym;
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
                let adv = State {
                    prod_id: st.prod_id,
                    dot: st.dot + 1,
                    origin: st.origin,
                };
                let added = chart[i + 1].insert(
                    adv,
                    Some(BackPtr::Scan {
                        prev: st,
                        token_index: i,
                    }),
                );
                if !added {
                    chart[i + 1]
                        .back
                        .entry(adv)
                        .or_default()
                        .push(BackPtr::Scan {
                            prev: st,
                            token_index: i,
                        });
                }
            }
        }
    }

    // Find an accept state deterministically by checking start productions in order.
    let accept = grammar
        .prods_for(start)
        .find_map(|pid| {
            let st = State {
                prod_id: pid,
                dot: grammar.prods[pid].rhs.len(),
                origin: 0,
            };
            chart[n].states.contains(&st).then_some(st)
        })
        .or_else(|| {
            chart[n].states.iter().copied().find(|st| {
                st.origin == 0
                    && grammar.prods[st.prod_id].lhs == start
                    && st.dot == grammar.prods[st.prod_id].rhs.len()
            })
        });

    let Some(accept) = accept else {
        // diagnose similarly to recognizer
        let mut farthest = 0usize;
        for i in 0..=n {
            if !chart[i].states.is_empty() {
                farthest = i;
            }
        }

        let span = if farthest < n {
            tokens[farthest].span.clone()
        } else {
            Span::new(src.len(), src.len())
        };

        return Err(
            OttError::new(format!("failed to parse input as `{}`", start)).with_span(span, src),
        );
    };

    let mut memo_node: HashMap<(usize, State), TermNode> = HashMap::new();
    let mut memo_children: HashMap<(usize, State), Option<Vec<TermNode>>> = HashMap::new();

    build_node(
        n,
        accept,
        &chart,
        grammar,
        tokens,
        src,
        &mut memo_node,
        &mut memo_children,
    )
    .ok_or_else(|| OttError::new("internal error: failed to reconstruct parse tree"))
}

fn state_span(tokens: &[Token], src: &str, origin: usize, pos: usize) -> Span {
    if origin == pos {
        let off = if origin < tokens.len() {
            tokens[origin].span.start
        } else {
            src.len()
        };
        return Span::new(off, off);
    }

    let start = tokens
        .get(origin)
        .map(|t| t.span.start)
        .unwrap_or(src.len());
    let end = tokens
        .get(pos.saturating_sub(1))
        .map(|t| t.span.end)
        .unwrap_or(src.len());

    Span::new(start, end)
}

fn build_node(
    pos: usize,
    st: State,
    chart: &[Chart],
    grammar: &Grammar,
    tokens: &[Token],
    src: &str,
    memo_node: &mut HashMap<(usize, State), TermNode>,
    memo_children: &mut HashMap<(usize, State), Option<Vec<TermNode>>>,
) -> Option<TermNode> {
    if let Some(n) = memo_node.get(&(pos, st)) {
        return Some(n.clone());
    }

    let prod = grammar.prods.get(st.prod_id)?;
    if st.dot != prod.rhs.len() {
        return None;
    }

    let children = build_children(
        pos,
        st,
        chart,
        grammar,
        tokens,
        src,
        memo_node,
        memo_children,
    )?;
    let span = state_span(tokens, src, st.origin, pos);

    let node = TermNode::Nonterminal {
        prod_id: st.prod_id,
        children,
        span,
    };

    memo_node.insert((pos, st), node.clone());
    Some(node)
}

fn build_children(
    pos: usize,
    st: State,
    chart: &[Chart],
    grammar: &Grammar,
    tokens: &[Token],
    src: &str,
    memo_node: &mut HashMap<(usize, State), TermNode>,
    memo_children: &mut HashMap<(usize, State), Option<Vec<TermNode>>>,
) -> Option<Vec<TermNode>> {
    if st.dot == 0 {
        return Some(Vec::new());
    }

    if let Some(cached) = memo_children.get(&(pos, st)) {
        return cached.clone();
    }

    let prod = grammar.prods.get(st.prod_id)?;
    let rhs_item = prod.rhs.get(st.dot - 1)?;

    let mut out: Option<Vec<TermNode>> = None;
    let bps = chart[pos].back.get(&st)?;

    for bp in bps {
        match bp {
            BackPtr::Scan { prev, token_index } => {
                // last rhs item must be terminal/metavar.
                match &rhs_item.sym {
                    Sym::Terminal(t) => {
                        let mut prefix = build_children(
                            *token_index,
                            *prev,
                            chart,
                            grammar,
                            tokens,
                            src,
                            memo_node,
                            memo_children,
                        )?;

                        prefix.push(TermNode::Terminal { text: t.clone() });
                        out = Some(prefix);
                        break;
                    }
                    Sym::Metavar(sort) => {
                        let mut prefix = build_children(
                            *token_index,
                            *prev,
                            chart,
                            grammar,
                            tokens,
                            src,
                            memo_node,
                            memo_children,
                        )?;

                        let tok = tokens.get(*token_index)?;
                        let TokenKind::Word(w) = &tok.kind else {
                            continue;
                        };

                        prefix.push(TermNode::Metavar {
                            sort: sort.clone(),
                            text: w.clone(),
                        });
                        out = Some(prefix);
                        break;
                    }
                    Sym::Nonterminal(_) => {}
                }
            }
            BackPtr::Complete { prev, completed } => {
                // last rhs item must be a nonterminal.
                let Sym::Nonterminal(expected) = &rhs_item.sym else {
                    continue;
                };

                let completed_prod = grammar.prods.get(completed.prod_id)?;
                if &completed_prod.lhs != expected {
                    continue;
                }

                let completed_node = build_node(
                    pos,
                    *completed,
                    chart,
                    grammar,
                    tokens,
                    src,
                    memo_node,
                    memo_children,
                )?;

                let mut prefix = build_children(
                    completed.origin,
                    *prev,
                    chart,
                    grammar,
                    tokens,
                    src,
                    memo_node,
                    memo_children,
                )?;
                prefix.push(completed_node);
                out = Some(prefix);
                break;
            }
        }
    }

    memo_children.insert((pos, st), out.clone());
    out
}

#[derive(Debug, Clone)]
enum PatToken {
    Atom(RhsItem),
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

    fn add_prod(&mut self, lhs: &str, rhs: Vec<RhsItem>, typst: Option<Template>) {
        for item in &rhs {
            if let Sym::Terminal(t) = &item.sym {
                if !t.is_empty() {
                    self.terminals.insert(t.clone());
                }
            }
        }
        self.grammar.add_prod(lhs.to_string(), rhs, typst);
    }

    fn compile_production(
        &mut self,
        lhs: &str,
        pattern: &str,
        typst: Option<Template>,
    ) -> OttResult<()> {
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

        self.add_prod(lhs, rhs, typst);
        Ok(())
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

    fn compile_comp_form(
        &mut self,
        raw_tokens: &[&str],
        start: usize,
    ) -> OttResult<(RhsItem, usize)> {
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

        Ok((
            RhsItem {
                sym: Sym::Nonterminal(list_nt),
                label: None,
            },
            i,
        ))
    }

    fn fresh_list_nonterminal(&mut self) -> String {
        self.list_counter += 1;
        format!("__ott_list_{}", self.list_counter)
    }

    fn define_list_nonterminal(&mut self, list_nt: &str, item: &[RhsItem], sep: Option<&str>) {
        let tail = format!("{list_nt}__tail");

        let item_syms = item
            .iter()
            .map(|it| RhsItem {
                sym: it.sym.clone(),
                label: None,
            })
            .collect::<Vec<_>>();

        // list_nt -> item tail
        let mut rhs = item_syms.clone();
        rhs.push(RhsItem {
            sym: Sym::Nonterminal(tail.clone()),
            label: None,
        });
        self.add_prod(list_nt, rhs, None);

        // tail -> ε
        self.add_prod(&tail, Vec::new(), None);

        // tail -> (sep?) item tail
        let mut rhs2 = Vec::new();
        if let Some(sep) = sep {
            rhs2.push(RhsItem {
                sym: Sym::Terminal(sep.to_string()),
                label: None,
            });
        }
        rhs2.extend(item_syms);
        rhs2.push(RhsItem {
            sym: Sym::Nonterminal(tail.clone()),
            label: None,
        });
        self.add_prod(&tail, rhs2, None);
    }

    fn expand_dots_once(&mut self, toks: &[PatToken], idx: usize) -> OttResult<Vec<PatToken>> {
        let PatToken::Dots(_dots) = &toks[idx] else {
            return Ok(toks.to_vec());
        };

        let (sep, left, right) = if idx >= 1
            && idx + 1 < toks.len()
            && matches!(
                &toks[idx - 1],
                PatToken::Atom(RhsItem {
                    sym: Sym::Terminal(_),
                    ..
                })
            )
            && toks[idx - 1].as_atom_sym().is_some_and(|s| {
                s == toks[idx + 1]
                    .as_atom_sym()
                    .unwrap_or(&Sym::Terminal("".to_string()))
            }) {
            let sep = match &toks[idx - 1] {
                PatToken::Atom(RhsItem {
                    sym: Sym::Terminal(sep),
                    ..
                }) => sep.clone(),
                _ => unreachable!(),
            };
            (Some(sep), &toks[..idx - 1], &toks[idx + 2..])
        } else {
            (None, &toks[..idx], &toks[idx + 1..])
        };

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
        out.push(PatToken::Atom(RhsItem {
            sym: Sym::Nonterminal(list_nt),
            label: None,
        }));
        out.extend(suffix);
        Ok(out)
    }

    fn classify_atom(&self, raw: &str) -> RhsItem {
        let raw = raw.trim();
        let tok = dequote(raw);

        if tok.is_empty() {
            return RhsItem {
                sym: Sym::Terminal(String::new()),
                label: None,
            };
        }

        if let Some(canon) = self.root_alias.get(tok) {
            return RhsItem {
                sym: Sym::Nonterminal(canon.clone()),
                label: Some(tok.to_string()),
            };
        }

        if let Some(canon) = self.metavar_alias.get(tok) {
            return RhsItem {
                sym: Sym::Metavar(canon.clone()),
                label: Some(tok.to_string()),
            };
        }

        if let Some((base, _suffix)) = split_index_suffix(tok) {
            if let Some(canon) = self.root_alias.get(base) {
                return RhsItem {
                    sym: Sym::Nonterminal(canon.clone()),
                    label: Some(tok.to_string()),
                };
            }
            if let Some(canon) = self.metavar_alias.get(base) {
                return RhsItem {
                    sym: Sym::Metavar(canon.clone()),
                    label: Some(tok.to_string()),
                };
            }
        }

        RhsItem {
            sym: Sym::Terminal(tok.to_string()),
            label: None,
        }
    }
}

impl PatToken {
    fn as_atom_sym(&self) -> Option<&Sym> {
        match self {
            Self::Atom(a) => Some(&a.sym),
            Self::Dots(_) => None,
        }
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

fn pat_compatible(a: &RhsItem, b: &RhsItem) -> bool {
    a.sym == b.sym
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

fn parse_typst_template(blocks: &[HomBlock]) -> OttResult<Option<Template>> {
    let blk = blocks.iter().find(|b| b.name == "typst");
    let Some(blk) = blk else {
        return Ok(None);
    };

    let body = blk.body.trim();
    if body.is_empty() {
        return Ok(None);
    }

    Ok(Some(Template::parse(body)?))
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
                    .ok_or_else(|| OttError::new("grammar rule missing root"))?;
                let canon = dequote(canon).to_string();

                if !root_alias.contains_key(&canon) {
                    roots.push(canon.clone());
                }

                for r in &rule.roots {
                    root_alias.insert(dequote(r).to_string(), canon.clone());
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
                .ok_or_else(|| OttError::new("metavar decl missing name"))?;
            let canon = dequote(canon).to_string();

            let names = mv
                .names
                .iter()
                .map(|n| dequote(n).to_string())
                .collect::<Vec<_>>();

            for name in &names {
                metavar_alias.insert(name.clone(), canon.clone());
            }

            if metavars.contains_key(&canon) {
                continue;
            }

            let lex = parse_lex_block(&mv.reps)
                .unwrap_or_else(|| LexPattern::Builtin(BuiltinLex::Alphanum));
            let typst = parse_typst_template(&mv.reps)?;

            metavars.insert(canon.clone(), MetavarDef { lex, typst, names });
        }
    }

    let mut builder = GrammarBuilder::new(root_alias.clone(), metavar_alias.clone());

    for item in &spec.spec.items {
        if let Item::Grammar(g) = item {
            for rule in &g.rules {
                let canon = rule
                    .roots
                    .first()
                    .ok_or_else(|| OttError::new("grammar rule missing root"))?;
                let canon = dequote(canon).to_string();

                for prod in &rule.productions {
                    let typst = parse_typst_template(&prod.annotations)?;
                    builder.compile_production(&canon, &prod.pattern, typst)?;
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
