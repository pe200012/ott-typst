use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindSpec {
    pub clauses: Vec<Clause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clause {
    Bind { binder: Expr, in_term: String },

    Assign { name: String, value: Expr },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    EmptySet,

    /// A simple identifier-like atom.
    Var(String),

    /// An uninterpreted atom that we keep as-is (used for Ott list-form
    /// comprehensions like `</ p // i />` that may appear in bind specs).
    Raw(String),

    Call {
        func: String,
        args: Vec<Expr>,
    },

    Dots {
        /// The number of dots: 2 (`..`), 3 (`...`), or 4 (`....`).
        dots: u8,
        start: Box<Expr>,
        end: Box<Expr>,
    },

    Union {
        left: Box<Expr>,
        right: Box<Expr>,
    },

    Hash {
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct BindParseError {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for BindParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{msg} (byte offset {off})",
            msg = self.message,
            off = self.offset
        )
    }
}

impl std::error::Error for BindParseError {}

type Result<T> = std::result::Result<T, BindParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokKind {
    Ident(String),
    Raw(String),
    Bind,
    In,
    Union,
    Eq,
    Hash,
    Dots(u8),
    LParen,
    RParen,
    Comma,
    EmptySet,
    Eof,
}

#[derive(Debug, Clone)]
struct Tok {
    kind: TokKind,
    start: usize,
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            i: 0,
        }
    }

    fn lex_all(mut self) -> Result<Vec<Tok>> {
        let mut out = Vec::new();
        loop {
            let tok = self.next_tok()?;
            let is_eof = matches!(tok.kind, TokKind::Eof);
            out.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(out)
    }

    fn next_tok(&mut self) -> Result<Tok> {
        self.skip_ws();
        let start = self.i;
        if self.i >= self.bytes.len() {
            return Ok(Tok {
                kind: TokKind::Eof,
                start,
            });
        }

        let b = self.bytes[self.i];
        match b {
            b'=' => {
                self.i += 1;
                Ok(Tok {
                    kind: TokKind::Eq,
                    start,
                })
            }
            b'#' => {
                self.i += 1;
                Ok(Tok {
                    kind: TokKind::Hash,
                    start,
                })
            }
            b'(' => {
                self.i += 1;
                Ok(Tok {
                    kind: TokKind::LParen,
                    start,
                })
            }
            b')' => {
                self.i += 1;
                Ok(Tok {
                    kind: TokKind::RParen,
                    start,
                })
            }
            b',' => {
                self.i += 1;
                Ok(Tok {
                    kind: TokKind::Comma,
                    start,
                })
            }
            b'.' => {
                // Match the longest dot-sequence first: `....`, `...`, `..`.
                let rem = &self.bytes[self.i..];

                if rem.len() >= 4 && rem[0..4] == [b'.', b'.', b'.', b'.'] {
                    self.i += 4;
                    Ok(Tok {
                        kind: TokKind::Dots(4),
                        start,
                    })
                } else if rem.len() >= 3 && rem[0..3] == [b'.', b'.', b'.'] {
                    self.i += 3;
                    Ok(Tok {
                        kind: TokKind::Dots(3),
                        start,
                    })
                } else if rem.len() >= 2 && rem[0..2] == [b'.', b'.'] {
                    self.i += 2;
                    Ok(Tok {
                        kind: TokKind::Dots(2),
                        start,
                    })
                } else {
                    Err(BindParseError {
                        message: "unexpected '.' (did you mean '..'?)".to_string(),
                        offset: start,
                    })
                }
            }
            b'{' => self.lex_empty_set(start),
            b'<' => self.lex_raw_comp(start),
            _ if is_ident_start(b) => self.lex_ident_or_keyword(start),
            _ => Err(BindParseError {
                message: format!(
                    "unexpected character '{}'",
                    self.src[self.i..].chars().next().unwrap_or('\u{FFFD}')
                ),
                offset: start,
            }),
        }
    }

    fn skip_ws(&mut self) {
        while self.i < self.bytes.len() {
            match self.bytes[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn lex_empty_set(&mut self, start: usize) -> Result<Tok> {
        // accept "{}" or "{ }" with whitespace.
        self.i += 1; // consume '{'
        self.skip_ws();
        if self.i < self.bytes.len() && self.bytes[self.i] == b'}' {
            self.i += 1;
            Ok(Tok {
                kind: TokKind::EmptySet,
                start,
            })
        } else {
            Err(BindParseError {
                message: "expected '}' to close empty set".to_string(),
                offset: start,
            })
        }
    }

    fn lex_raw_comp(&mut self, start: usize) -> Result<Tok> {
        // Parse Ott list-form comprehension-ish atoms of the form `</ ... />`.
        // These can appear inside bind specs (e.g. `b( </ pi // i /> )`).
        if self.i + 1 >= self.bytes.len() || self.bytes[self.i + 1] != b'/' {
            return Err(BindParseError {
                message: "unexpected '<' (did you mean `</ ... />`?)".to_string(),
                offset: start,
            });
        }

        // consume `</`
        self.i += 2;

        let rest = &self.src[self.i..];
        let Some(rel_end) = rest.find("/>") else {
            return Err(BindParseError {
                message: "unterminated comprehension atom (expected '/>')".to_string(),
                offset: start,
            });
        };

        let end = self.i + rel_end + 2; // include '/>'
        let raw = self.src[start..end].to_string();
        self.i = end;

        Ok(Tok {
            kind: TokKind::Raw(raw),
            start,
        })
    }

    fn lex_ident_or_keyword(&mut self, start: usize) -> Result<Tok> {
        let begin = self.i;
        self.i += 1;
        while self.i < self.bytes.len() && is_ident_continue(self.bytes[self.i]) {
            self.i += 1;
        }
        let s = &self.src[begin..self.i];
        let kind = match s {
            "bind" => TokKind::Bind,
            "in" => TokKind::In,
            "union" => TokKind::Union,
            _ => TokKind::Ident(s.to_string()),
        };
        Ok(Tok { kind, start })
    }
}

fn is_ident_start(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'_')
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || matches!(b, b'0'..=b'9' | b'\'')
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> &Tok {
        self.toks
            .get(self.pos)
            .unwrap_or_else(|| self.toks.last().expect("lexer always adds EOF"))
    }

    fn bump(&mut self) -> Tok {
        let tok = self.peek().clone();
        if !matches!(tok.kind, TokKind::Eof) {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: TokKind) -> Result<()> {
        let tok = self.bump();
        if tok.kind == kind {
            Ok(())
        } else {
            Err(BindParseError {
                message: format!("expected {kind:?}, found {found:?}", found = tok.kind),
                offset: tok.start,
            })
        }
    }

    fn parse_bind_spec(&mut self) -> Result<BindSpec> {
        let mut clauses = Vec::new();

        while !matches!(self.peek().kind, TokKind::Eof) {
            clauses.push(self.parse_clause()?);
        }

        Ok(BindSpec { clauses })
    }

    fn parse_clause(&mut self) -> Result<Clause> {
        match &self.peek().kind {
            TokKind::Bind => {
                self.bump();
                let binder = self.parse_expr()?;
                self.expect(TokKind::In)?;
                let in_term = self.parse_ident_string()?;
                Ok(Clause::Bind { binder, in_term })
            }
            TokKind::Ident(_) => {
                // assignment clause: <ident> = <expr>
                let name = self.parse_ident_string()?;
                self.expect(TokKind::Eq)?;
                let value = self.parse_expr()?;
                Ok(Clause::Assign { name, value })
            }
            other => Err(BindParseError {
                message: format!("unexpected token at start of clause: {other:?}"),
                offset: self.peek().start,
            }),
        }
    }

    fn parse_ident_string(&mut self) -> Result<String> {
        let tok = self.bump();
        match tok.kind {
            TokKind::Ident(s) => Ok(s),
            other => Err(BindParseError {
                message: format!("expected identifier, found {other:?}"),
                offset: tok.start,
            }),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_union()
    }

    fn parse_union(&mut self) -> Result<Expr> {
        let mut left = self.parse_range()?;

        loop {
            match self.peek().kind {
                TokKind::Union => {
                    self.bump();
                    let right = self.parse_range()?;
                    left = Expr::Union {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                TokKind::Hash => {
                    self.bump();
                    let right = self.parse_range()?;
                    left = Expr::Hash {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr> {
        let left = self.parse_primary()?;
        if let TokKind::Dots(dots) = self.peek().kind {
            self.bump();
            let right = self.parse_primary()?;
            Ok(Expr::Dots {
                dots,
                start: Box::new(left),
                end: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let tok = self.bump();
        match tok.kind {
            TokKind::EmptySet => Ok(Expr::EmptySet),
            TokKind::Raw(raw) => Ok(Expr::Raw(raw)),
            TokKind::Ident(name) => {
                if matches!(self.peek().kind, TokKind::LParen) {
                    self.bump(); // consume '('
                    let mut args = Vec::new();

                    if !matches!(self.peek().kind, TokKind::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek().kind, TokKind::Comma) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }

                    self.expect(TokKind::RParen)?;
                    Ok(Expr::Call { func: name, args })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            TokKind::LParen => {
                let e = self.parse_expr()?;
                self.expect(TokKind::RParen)?;
                Ok(e)
            }
            other => Err(BindParseError {
                message: format!("unexpected token in expression: {other:?}"),
                offset: tok.start,
            }),
        }
    }
}

pub fn parse_bind_spec(src: &str) -> Result<BindSpec> {
    let toks = Lexer::new(src).lex_all()?;
    Parser::new(toks).parse_bind_spec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_bind() {
        let bs = parse_bind_spec("bind x in t").unwrap();
        assert_eq!(bs.clauses.len(), 1);
    }

    #[test]
    fn parse_assignment_emptyset() {
        let bs = parse_bind_spec("binders = {}").unwrap();
        assert_eq!(bs.clauses.len(), 1);
    }

    #[test]
    fn parse_union_and_calls() {
        let bs = parse_bind_spec("binders = binders(p1) union binders(p2)").unwrap();
        assert_eq!(bs.clauses.len(), 1);
    }

    #[test]
    fn parse_multiclause() {
        let bs = parse_bind_spec("Tdom = Tdom(G) union X tdom = tdom(G)").unwrap();
        assert_eq!(bs.clauses.len(), 2);
    }

    #[test]
    fn parse_range() {
        let bs = parse_bind_spec("b = b(a1..an)").unwrap();
        assert_eq!(bs.clauses.len(), 1);
    }
}
