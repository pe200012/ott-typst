use crate::ast::*;
use crate::error::{OttError, OttResult, Position};

const TOPLEVEL_KEYWORDS: &[&str] = &[
    "embed",
    "metavar",
    "indexvar",
    "grammar",
    "subrules",
    "contextrules",
    "substitutions",
    "freevars",
    "defns",
    "defn",
    "funs",
    "fun",
    "homs",
    "parsing",
    "begincoqsection",
    "endcoqsection",
    "coqvariable",
];

#[must_use]
fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

#[must_use]
fn is_blank_or_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty() || t.starts_with('%')
}

#[must_use]
fn is_comment_block_start(line: &str) -> bool {
    line.trim_start().starts_with(">>")
}

#[must_use]
fn is_comment_block_end(line: &str) -> bool {
    line.trim_start().starts_with("<<")
}

fn skip_comment_block(lines: &[&str], i: &mut usize) {
    // Ott supports multi-line comment blocks delimited by `>>` (open) and `<<` (close).
    // They can be nested.
    let mut depth = 0usize;

    while *i < lines.len() {
        let t = lines[*i].trim_start();
        if is_comment_block_start(t) {
            depth += 1;
            *i += 1;
            continue;
        }
        if is_comment_block_end(t) {
            if depth > 0 {
                depth -= 1;
            }
            *i += 1;
            if depth == 0 {
                break;
            }
            continue;
        }
        *i += 1;
    }
}

fn skip_trivia(lines: &[&str], i: &mut usize) {
    // Skip blank lines, `%` line comments, and `>> .. <<` comment blocks.
    while *i < lines.len() {
        let line = lines[*i];
        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        if is_comment_block_start(line) {
            skip_comment_block(lines, i);
            continue;
        }
        break;
    }
}

#[must_use]
fn is_toplevel_keyword_line(line: &str) -> bool {
    // In Ott, section keywords are recognized after skipping leading whitespace.
    // (This matches the reference lexer behavior.)
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    let w = first_word(t);
    TOPLEVEL_KEYWORDS.contains(&w)
}

pub fn parse_spec(src: &str) -> OttResult<Spec> {
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0usize;
    let mut items = Vec::new();

    while i < lines.len() {
        skip_trivia(&lines, &mut i);
        if i >= lines.len() {
            break;
        }

        let line = lines[i];
        let t = line.trim_start();
        let kw = first_word(t);

        match kw {
            "embed" => items.push(Item::Embed(parse_embed(&lines, &mut i)?)),
            "metavar" => items.push(Item::Metavar(parse_metavar(false, &lines, &mut i)?)),
            "indexvar" => items.push(Item::Metavar(parse_metavar(true, &lines, &mut i)?)),
            "grammar" => items.push(Item::Grammar(parse_grammar(&lines, &mut i)?)),
            "subrules" => items.push(Item::Subrules(parse_subrules(&lines, &mut i)?)),
            "substitutions" => items.push(Item::Substitutions(parse_substitutions(&lines, &mut i)?)),
            "defns" => items.push(Item::Defns(parse_defns(&lines, &mut i)?)),
            "defn" => items.push(Item::Defn(parse_defn(&lines, &mut i)?)),
            _ => items.push(Item::UnknownSection(parse_unknown_section(kw, &lines, &mut i))),
        }
    }

    Ok(Spec::new(items))
}

fn parse_unknown_section(keyword: &str, lines: &[&str], i: &mut usize) -> UnknownSection {
    let mut out = Vec::new();
    out.push(lines[*i].to_string());
    *i += 1;

    while *i < lines.len() {
        let line = lines[*i];
        if is_toplevel_keyword_line(line) {
            break;
        }
        out.push(line.to_string());
        *i += 1;
    }

    UnknownSection {
        keyword: keyword.to_string(),
        lines: out,
    }
}

fn parse_embed(lines: &[&str], i: &mut usize) -> OttResult<EmbedSection> {
    let start_line = *i + 1;
    let line = lines
        .get(*i)
        .copied()
        .ok_or_else(|| OttError::new("unexpected EOF while parsing embed section"))?;

    let mut rest = line.trim_start();
    rest = rest
        .strip_prefix("embed")
        .ok_or_else(|| OttError::new("internal error: embed keyword mismatch"))?
        .trim_start();

    *i += 1;

    let mut blocks = Vec::new();

    // Support `embed {{ ... }}` (start of a hom block on the same line).
    if rest.starts_with("{{") {
        blocks.push(parse_hom_block_from_first_line(rest, lines, i, start_line)?);
    }

    while *i < lines.len() {
        skip_trivia(lines, i);
        if *i >= lines.len() {
            break;
        }

        let ln = lines[*i];
        if is_toplevel_keyword_line(ln) {
            break;
        }
        if !ln.trim_start().starts_with("{{") {
            return Err(OttError::new("expected `{{ ... }}` block in embed section")
                .with_position(Position::new(*i + 1, 1)));
        }
        blocks.push(parse_multiline_hom_block(lines, i)?);
    }

    Ok(EmbedSection { blocks })
}

fn parse_metavar(is_index: bool, lines: &[&str], i: &mut usize) -> OttResult<MetavarDecl> {
    let keyword = if is_index { "indexvar" } else { "metavar" };
    let start_line = *i + 1;

    // Ott allows metavar declarations to span multiple lines before the `::=`.
    let mut names_buf = String::new();
    let mut after_cce: Option<String> = None;

    while *i < lines.len() {
        let line = lines[*i];

        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        if is_comment_block_start(line) {
            skip_comment_block(lines, i);
            continue;
        }

        let t = line.trim_start();
        let chunk = if names_buf.is_empty() {
            t.strip_prefix(keyword)
                .ok_or_else(|| OttError::new("internal error: metavar keyword mismatch"))?
                .trim_start()
        } else {
            t
        };

        if let Some((before, after)) = chunk.split_once("::=") {
            names_buf.push_str(before);
            after_cce = Some(after.to_string());
            *i += 1;
            break;
        }

        names_buf.push_str(chunk);
        names_buf.push(' ');
        *i += 1;
    }

    let after_cce = after_cce.ok_or_else(|| {
        OttError::new("expected `::=` in metavar declaration")
            .with_position(Position::new(start_line, 1))
    })?;

    // Metavar names can themselves carry inline `{{ ... }}` blocks (e.g. per-name TeX).
    // For now we treat these as part of the metavar's overall hom list.
    let (mut reps, names_stripped) = extract_inline_hom_blocks(&names_buf);

    let names: Vec<String> = names_stripped
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();

    if names.is_empty() {
        return Err(OttError::new("metavar declaration must name at least one identifier")
            .with_position(Position::new(start_line, 1)));
    }

    reps.extend(extract_inline_hom_blocks(&after_cce).0);

    // Additional representation hom blocks can appear on subsequent lines.
    while *i < lines.len() {
        skip_trivia(lines, i);
        if *i >= lines.len() {
            break;
        }

        let ln = lines[*i];
        if is_toplevel_keyword_line(ln) {
            break;
        }
        let t = ln.trim_start();
        if !t.starts_with("{{") {
            break;
        }
        reps.extend(extract_inline_hom_blocks(t).0);
        *i += 1;
    }

    Ok(MetavarDecl {
        is_index,
        names,
        reps,
    })
}

fn parse_grammar(lines: &[&str], i: &mut usize) -> OttResult<GrammarSection> {
    // consume `grammar` line
    *i += 1;

    let mut rules = Vec::new();

    while *i < lines.len() {
        skip_trivia(lines, i);
        if *i >= lines.len() {
            break;
        }

        let line = lines[*i];
        if is_toplevel_keyword_line(line) {
            break;
        }

        // Grammar rule header line: `<root> :: <sort> ::= ...`
        let header = line;
        if !header.contains("::=") {
            return Err(OttError::new("expected grammar rule header containing `::=`")
                .with_position(Position::new(*i + 1, 1)));
        }

        let (lhs, rhs) = header
            .split_once("::=")
            .ok_or_else(|| OttError::new("expected `::=` in grammar header"))?;

        let (root_part, sort_part) = lhs
            .split_once("::")
            .ok_or_else(|| {
                OttError::new("expected `::` in grammar header before `::=`")
                    .with_position(Position::new(*i + 1, 1))
            })?;

        let (root_annos, root_part_stripped) = extract_inline_hom_blocks(root_part);

        let roots: Vec<String> = root_part_stripped
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();

        if roots.is_empty() {
            return Err(OttError::new("grammar rule must have at least one root")
                .with_position(Position::new(*i + 1, 1)));
        }

        let (sort_annos, sort_part_stripped) = extract_inline_hom_blocks(sort_part);
        let sort = sort_part_stripped.trim().to_string();

        let (mut annotations, _stripped_rhs) = extract_inline_hom_blocks(rhs);
        annotations.extend(root_annos);
        annotations.extend(sort_annos);

        *i += 1;

        // Grammar rules may carry additional `{{ ... }}` blocks on the following lines
        // before the first production (e.g. Coq/Isa/HOL type annotations).
        while *i < lines.len() {
            skip_trivia(lines, i);
            if *i >= lines.len() {
                break;
            }
            let ln = lines[*i];
            if is_toplevel_keyword_line(ln) {
                break;
            }
            if ln.contains("::=") && !ln.trim_start().starts_with('|') {
                break;
            }

            let t = ln.trim_start();
            if !t.starts_with("{{") {
                break;
            }

            if t.contains("}}") {
                annotations.extend(extract_inline_hom_blocks(t).0);
                *i += 1;
            } else {
                annotations.push(parse_multiline_hom_block(lines, i)?);
            }
        }

        let comment = annotations
            .iter()
            .find(|b| b.name == "com")
            .map(|b| b.body.trim().to_string());

        let mut productions = Vec::new();

        while *i < lines.len() {
            skip_trivia(lines, i);
            if *i >= lines.len() {
                break;
            }
            let ln = lines[*i];
            if is_toplevel_keyword_line(ln) {
                break;
            }
            // Next grammar rule header.
            // Grammar rules may be indented; we treat any non-production line containing `::=`
            // as a new rule header.
            if ln.contains("::=") && !ln.trim_start().starts_with('|') {
                break;
            }

            if !ln.trim_start().starts_with('|') {
                return Err(OttError::new("expected production line starting with `|`")
                    .with_position(Position::new(*i + 1, 1)));
            }

            productions.push(parse_production(lines, i)?);
        }

        rules.push(GrammarRule {
            roots,
            sort,
            comment,
            annotations,
            productions,
        });
    }

    Ok(GrammarSection { rules })
}

fn parse_production(lines: &[&str], i: &mut usize) -> OttResult<Production> {
    let start_line = *i + 1;

    let mut raw_lines = Vec::new();

    let first = lines[*i];
    raw_lines.push(first.to_string());
    *i += 1;

    while *i < lines.len() {
        let ln = lines[*i];
        if ln.trim().is_empty() {
            break;
        }
        if is_blank_or_comment(ln) {
            // comment lines are *not* part of the production, but we just skip them.
            *i += 1;
            continue;
        }
        if is_toplevel_keyword_line(ln) {
            break;
        }
        if ln.contains("::=") && !ln.trim_start().starts_with('|') {
            break;
        }
        if ln.trim_start().starts_with('|') {
            break;
        }
        raw_lines.push(ln.to_string());
        *i += 1;
    }

    let raw = raw_lines.join("\n");

    // Continuation lines often carry `{{ com ... }}` / `{{ ich ... }}` blocks,
    // so we extract annotations from the full raw production text.
    let (annos, raw_wo_homs) = extract_inline_hom_blocks(&raw);
    let (bind_specs, _raw_wo_bindspec) = extract_bind_specs(&raw_wo_homs);

    // Parse the structural part `pattern :: meta :: name` from the whole (possibly multi-line)
    // production. Ott allows the `:: ... :: ...` metadata to appear on a later line.
    let mut structural_parts = Vec::new();
    for (idx, ln) in raw_lines.iter().enumerate() {
        let t = ln.trim();
        if idx == 0 {
            let t = t.strip_prefix('|').ok_or_else(|| {
                OttError::new("internal error: production does not start with `|`")
                    .with_position(Position::new(start_line, 1))
            })?;
            structural_parts.push(t.trim_start().to_string());
        } else {
            structural_parts.push(t.to_string());
        }
    }

    let structural = structural_parts.join(" ");
    let (_struct_annos, structural_wo_homs) = extract_inline_hom_blocks(&structural);

    let (pattern_part, after_first) = structural_wo_homs.split_once("::").ok_or_else(|| {
        OttError::new("expected `::` metadata in production")
            .with_position(Position::new(start_line, 1))
    })?;
    let (meta_part, after_second) = after_first.split_once("::").ok_or_else(|| {
        OttError::new("expected second `::` metadata in production")
            .with_position(Position::new(start_line, 1))
    })?;

    let pattern = pattern_part.trim_end().to_string();
    let meta = match meta_part.trim() {
        "" => None,
        s => Some(s.to_string()),
    };

    let rest = after_second.trim();
    let name = rest.split_whitespace().next().map(|s| s.to_string());

    Ok(Production {
        pattern,
        meta,
        name,
        bind_specs,
        annotations: annos,
        raw,
    })
}

fn parse_subrules(lines: &[&str], i: &mut usize) -> OttResult<SubrulesSection> {
    *i += 1;
    let mut relations = Vec::new();

    while *i < lines.len() {
        let line = lines[*i];
        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        if is_toplevel_keyword_line(line) {
            break;
        }

        let t = line.trim();
        let (sub, sup) = t
            .split_once("<::")
            .ok_or_else(|| OttError::new("expected `<::` in subrules line").with_position(Position::new(*i + 1, 1)))?;
        relations.push(Subrule {
            sub: sub.trim().to_string(),
            sup: sup.trim().to_string(),
        });
        *i += 1;
    }

    Ok(SubrulesSection { relations })
}

fn parse_substitutions(lines: &[&str], i: &mut usize) -> OttResult<SubstitutionsSection> {
    *i += 1;
    let mut entries = Vec::new();

    while *i < lines.len() {
        let line = lines[*i];
        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }
        if is_toplevel_keyword_line(line) {
            break;
        }

        let t = line.trim();
        let mut it = t.split_whitespace();
        let kind_str = it.next().ok_or_else(|| {
            OttError::new("expected `single` or `multiple` in substitutions")
                .with_position(Position::new(*i + 1, 1))
        })?;
        let kind = match kind_str {
            "single" => SubstKind::Single,
            "multiple" => SubstKind::Multiple,
            _ => {
                return Err(OttError::new("expected `single` or `multiple` in substitutions")
                    .with_position(Position::new(*i + 1, 1)));
            }
        };

        let nonterminal = it.next().ok_or_else(|| {
            OttError::new("expected nonterminal in substitutions")
                .with_position(Position::new(*i + 1, 1))
        })?;
        let metavar = it.next().ok_or_else(|| {
            OttError::new("expected metavar in substitutions")
                .with_position(Position::new(*i + 1, 1))
        })?;

        let rest = it.collect::<Vec<_>>().join(" ");
        let (_before, after) = rest
            .split_once("::")
            .ok_or_else(|| OttError::new("expected `::` before substitution name").with_position(Position::new(*i + 1, 1)))?;
        let name = after.trim().to_string();

        entries.push(SubstEntry {
            kind,
            nonterminal: nonterminal.to_string(),
            metavar: metavar.to_string(),
            name,
        });

        *i += 1;
    }

    Ok(SubstitutionsSection { entries })
}

fn parse_defns(lines: &[&str], i: &mut usize) -> OttResult<DefnsSection> {
    // `defns` section is mostly orthogonal to our initial Typst rendering; we preserve its raw lines.
    let mut out = Vec::new();
    out.push(lines[*i].to_string());
    *i += 1;

    while *i < lines.len() {
        let line = lines[*i];
        if is_toplevel_keyword_line(line) {
            break;
        }
        out.push(line.to_string());
        *i += 1;
    }

    Ok(DefnsSection { lines: out })
}

fn parse_defn(lines: &[&str], i: &mut usize) -> OttResult<DefnBlock> {
    // consume `defn` keyword line
    let defn_line = lines[*i];
    let mut header = defn_line.trim_start().strip_prefix("defn").unwrap_or("").trim();
    *i += 1;

    if header.is_empty() {
        // header is on the next non-blank line
        while *i < lines.len() && is_blank_or_comment(lines[*i]) {
            *i += 1;
        }
        if *i >= lines.len() {
            return Err(OttError::new("unexpected EOF after `defn`")
                .with_position(Position::new(*i + 1, 1)));
        }
        header = lines[*i].trim();
        *i += 1;
    }

    // strip trailing `by` if present
    let header_wo_by = header.strip_suffix("by").map(str::trim).unwrap_or(header);

    let (annos, header_stripped) = extract_inline_hom_blocks(header_wo_by);
    let comment = annos
        .iter()
        .find(|b| b.name == "com")
        .map(|b| b.body.trim().to_string());

    let header_stripped = header_stripped.trim().to_string();

    let mut rules = Vec::new();
    let mut premises = Vec::new();

    while *i < lines.len() {
        let line = lines[*i];

        if is_toplevel_keyword_line(line) {
            break;
        }

        if is_blank_or_comment(line) {
            *i += 1;
            continue;
        }

        let t = line.trim_start();

        if is_rule_bar_line(t) {
            let name = parse_rule_name_from_bar_line(t).ok_or_else(|| {
                OttError::new("expected `:: RuleName` after rule bar")
                    .with_position(Position::new(*i + 1, 1))
            })?;
            *i += 1;

            // read conclusion lines
            while *i < lines.len() && is_blank_or_comment(lines[*i]) {
                *i += 1;
            }
            if *i >= lines.len() {
                return Err(OttError::new("unexpected EOF: missing rule conclusion")
                    .with_position(Position::new(*i + 1, 1)));
            }

            let mut conclusion = Vec::new();
            while *i < lines.len() {
                let ln = lines[*i];
                if ln.trim().is_empty() {
                    break;
                }
                if is_blank_or_comment(ln) {
                    *i += 1;
                    continue;
                }
                if is_toplevel_keyword_line(ln) {
                    break;
                }
                if is_rule_bar_line(ln.trim_start()) {
                    break;
                }
                conclusion.push(ln.trim().to_string());
                *i += 1;
            }

            rules.push(InferenceRule {
                name,
                premises: std::mem::take(&mut premises),
                conclusion,
            });

            continue;
        }

        premises.push(line.trim().to_string());
        *i += 1;
    }

    Ok(DefnBlock {
        header: header_stripped,
        comment,
        rules,
    })
}

#[must_use]
fn is_rule_bar_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("----")
}

#[must_use]
fn parse_rule_name_from_bar_line(line: &str) -> Option<String> {
    // Example: "----------------- :: App1"
    let (_, rest) = line.split_once("::")?;
    // Some Ott outputs use multiple :: fields; we take the last.
    let last = rest.rsplit("::").next().unwrap_or(rest);
    Some(last.trim().to_string())
}

/// Extract all inline `{{ ... }}` blocks from `s`.
///
/// Returns `(blocks, stripped)` where `stripped` is `s` with those blocks removed.
#[must_use]
fn extract_inline_hom_blocks(s: &str) -> (Vec<HomBlock>, String) {
    let mut blocks = Vec::new();
    let mut out = String::new();

    let mut idx = 0usize;
    while let Some(start) = s[idx..].find("{{") {
        let start = idx + start;
        out.push_str(&s[idx..start]);

        let after_start = start + 2;
        if let Some(end_rel) = s[after_start..].find("}}") {
            let end = after_start + end_rel;
            let inner = s[after_start..end].trim();
            let mut it = inner.splitn(2, char::is_whitespace);
            let name = it.next().unwrap_or("").trim();
            let body = it.next().unwrap_or("").trim();

            if !name.is_empty() {
                blocks.push(HomBlock {
                    name: name.to_string(),
                    body: body.to_string(),
                });
            }

            idx = end + 2;
        } else {
            // no closing, copy rest and stop
            out.push_str(&s[start..]);
            idx = s.len();
            break;
        }
    }

    if idx < s.len() {
        out.push_str(&s[idx..]);
    }

    (blocks, out)
}

/// Extract all binding specifications of the form `(+ ... +)`.
///
/// Returns `(bind_specs, stripped)` where `stripped` is the input with all
/// occurrences of `(+ ... +)` removed.
#[must_use]
fn extract_bind_specs(s: &str) -> (Vec<String>, String) {
    let bytes = s.as_bytes();

    let before_ok = |b: u8| b.is_ascii_whitespace() || matches!(b, b')' | b']' | b'}');
    let after_ok = |b: u8| {
        b.is_ascii_whitespace() || matches!(b, b'{' | b'%' | b')' | b']' | b'}' | b',' | b';')
    };

    let mut specs = Vec::new();
    let mut stripped = String::new();

    let mut copy_from = 0usize;
    let mut search_from = 0usize;

    while let Some(rel_start) = s[search_from..].find("(+") {
        let start = search_from + rel_start;

        if start > 0 && !before_ok(bytes[start - 1]) {
            // Not a real bind-spec delimiter (e.g. inside a quoted terminal `'(+'`).
            search_from = start + 2;
            continue;
        }

        // Find a matching closing `+)` with a reasonable boundary.
        let mut end = None;
        let mut close_search_from = start + 2;
        while let Some(rel_end) = s[close_search_from..].find("+)") {
            let cand = close_search_from + rel_end;
            let after = cand + 2;

            if after < s.len() && !after_ok(bytes[after]) {
                close_search_from = cand + 2;
                continue;
            }

            end = Some(cand);
            break;
        }

        let Some(end) = end else {
            break;
        };

        stripped.push_str(&s[copy_from..start]);

        let body = s[start + 2..end].trim();
        if !body.is_empty() {
            specs.push(body.to_string());
        }

        copy_from = end + 2;
        search_from = copy_from;
    }

    stripped.push_str(&s[copy_from..]);
    (specs, stripped)
}

fn parse_hom_block_from_first_line(
    first_line: &str,
    lines: &[&str],
    i: &mut usize,
    start_line: usize,
) -> OttResult<HomBlock> {
    let first = first_line.trim_start();
    if !first.starts_with("{{") {
        return Err(OttError::new("expected `{{` to start hom block")
            .with_position(Position::new(start_line, 1)));
    }

    // Strip the leading `{{`.
    let after = &first[2..];

    // Support `{{ name body }}` inline blocks.
    if let Some((inner, _after_close)) = after.split_once("}}") {
        return parse_hom_inner(inner, start_line);
    }

    let mut collected = Vec::new();
    collected.push(after.to_string());

    while *i < lines.len() {
        let ln = lines[*i];
        if ln.trim_start().starts_with("}}") {
            *i += 1;
            break;
        }
        collected.push(ln.to_string());
        *i += 1;
    }

    let joined = collected.join("\n");
    parse_hom_inner(&joined, start_line)
}

fn parse_hom_inner(inner: &str, start_line: usize) -> OttResult<HomBlock> {
    let joined_trim = inner.trim();
    let mut it = joined_trim.splitn(2, char::is_whitespace);
    let name = it.next().unwrap_or("").trim();
    let body = it.next().unwrap_or("").trim();

    if name.is_empty() {
        return Err(OttError::new("hom block must have a name")
            .with_position(Position::new(start_line, 1)));
    }

    Ok(HomBlock {
        name: name.to_string(),
        body: body.to_string(),
    })
}

fn parse_multiline_hom_block(lines: &[&str], i: &mut usize) -> OttResult<HomBlock> {
    let start_line = *i + 1;
    let first_line = lines
        .get(*i)
        .copied()
        .ok_or_else(|| OttError::new("unexpected EOF while parsing hom block"))?;

    *i += 1;
    parse_hom_block_from_first_line(first_line, lines, i, start_line)
}
