//! Recursive-descent parser. Statements end at a newline or `;`. Newlines are
//! ignored inside parentheses and after an infix or connection operator, so long
//! expressions and chains can wrap.
//!
//! The parser never looks at whitespace: the lexer already decided which dots are
//! member access and which start a name.

use crate::ast::*;
use crate::error::{ParseError, Span};
use crate::lexer::{Tok, Token, lex};

pub fn parse(src: &str) -> Result<File, ParseError> {
    Parser {
        toks: lex(src)?,
        pos: 0,
        depth: 0,
        last_end: 0,
        in_generic: false,
    }
    .file()
}

/// Parse `src` as one expression and nothing else: `10kohm +/- 1%`, `1.8V..5.5V`. A
/// catalog cell is written this way (ADR 0022). The spans in an error are into `src`.
pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser {
        toks: lex(src)?,
        pos: 0,
        depth: 0,
        last_end: 0,
        in_generic: false,
    };
    let e = parser.expr()?;
    if parser.at(&Tok::Eof) {
        Ok(e)
    } else {
        Err(unexpected(parser.peek(), "the end of the value"))
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// Open parentheses; newlines are not significant while this is above zero.
    depth: usize,
    last_end: usize,
    /// Reading the body of a `generic`, whose pin rows may have no name and no pads (ADR 0068).
    in_generic: bool,
}

type Res<T> = Result<T, ParseError>;

/// `///` lines collected before an item, with the span of the first for errors.
struct Docs {
    lines: Vec<String>,
    first: Option<Span>,
}

impl Docs {
    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl Parser {
    fn peek(&mut self) -> &Token {
        if self.depth > 0 {
            while self.toks[self.pos].tok == Tok::Newline {
                self.pos += 1;
            }
        }
        &self.toks[self.pos]
    }

    fn bump(&mut self) -> Token {
        self.peek();
        let t = self.toks[self.pos].clone();
        if t.tok != Tok::Eof {
            self.pos += 1;
        }
        self.last_end = t.span.end;
        t
    }

    fn at(&mut self, tok: &Tok) -> bool {
        &self.peek().tok == tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        let found = self.at(tok);
        if found {
            self.bump();
        }
        found
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> Res<Token> {
        if self.at(tok) {
            Ok(self.bump())
        } else {
            Err(unexpected(self.peek(), what))
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek().tok == Tok::Newline {
            self.pos += 1;
        }
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek().tok, Tok::Newline | Tok::Semi) {
            self.pos += 1;
        }
    }

    fn end_statement(&mut self, in_block: bool) -> Res<()> {
        match self.peek().tok {
            Tok::Newline | Tok::Semi => {
                self.bump();
                Ok(())
            }
            Tok::Eof => Ok(()),
            Tok::RBrace if in_block => Ok(()),
            _ => Err(unexpected(self.peek(), "end of line or `;`")),
        }
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.last_end)
    }

    fn ident(&mut self, what: &str) -> Res<Ident> {
        let t = self.peek().clone();
        match t.tok {
            Tok::Ident(name) => {
                self.bump();
                Ok(Ident {
                    name,
                    quoted: false,
                    span: t.span,
                })
            }
            Tok::QuotedIdent(name) => {
                self.bump();
                Ok(Ident {
                    name,
                    quoted: true,
                    span: t.span,
                })
            }
            _ => Err(unexpected(&t, what)),
        }
    }

    /// Consecutive `//!` lines.
    fn inner_docs(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            self.skip_separators();
            match self.peek().tok.clone() {
                Tok::DocInner(text) => {
                    self.bump();
                    lines.push(text);
                }
                _ => return lines,
            }
        }
    }

    /// Consecutive `///` lines.
    fn outer_docs(&mut self) -> Docs {
        let mut docs = Docs {
            lines: Vec::new(),
            first: None,
        };
        loop {
            self.skip_separators();
            let t = self.peek().clone();
            match t.tok {
                Tok::DocOuter(text) => {
                    self.bump();
                    docs.first.get_or_insert(t.span);
                    docs.lines.push(text);
                }
                _ => return docs,
            }
        }
    }

    fn misplaced_inner_doc(&mut self) -> Option<ParseError> {
        let t = self.peek().clone();
        matches!(t.tok, Tok::DocInner(_)).then(|| {
            ParseError::new(
                t.span,
                "inner doc comments (`//!`) belong at the very start of a file or definition",
            )
        })
    }

    fn file(mut self) -> Res<File> {
        let mut file = File {
            inner_docs: self.inner_docs(),
            imports: Vec::new(),
            definitions: Vec::new(),
            physicals: Vec::new(),
        };
        loop {
            self.skip_separators();
            if let Some(e) = self.misplaced_inner_doc() {
                return Err(e);
            }
            let docs = self.outer_docs();
            let t = self.peek().clone();
            match &t.tok {
                Tok::Eof if docs.is_empty() => return Ok(file),
                Tok::Ident(k) if k == "import" && docs.is_empty() => {
                    file.imports.push(self.import()?)
                }
                Tok::Ident(k) if DefKind::from_keyword(k).is_some() => {
                    file.definitions.push(self.definition(docs)?)
                }
                Tok::Ident(k)
                    if k == "physical"
                        && matches!(self.peek_second(), Tok::Ident(w) if w == "for") =>
                {
                    file.physicals.push(self.physical(docs)?)
                }
                // Docs only get the blame when the next thing really is not a definition.
                Tok::Eof | Tok::Ident(_) if !docs.is_empty() && is_import_or_end(&t.tok) => {
                    return Err(ParseError::new(
                        docs.first.expect("docs is not empty"),
                        "this doc comment is not followed by a definition",
                    ));
                }
                _ => {
                    return Err(unexpected(
                        &t,
                        "`import`, `board`, `system`, `panel`, `part`, `interface`, `module`, `enum`, `generic` or `physical for`",
                    ));
                }
            }
            self.end_statement(false)?;
        }
    }

    fn import(&mut self) -> Res<Import> {
        let start = self.bump().span.start;
        let mut path = vec![self.ident("a module name after `import`")?];
        let mut names = None;
        while self.eat(&Tok::ColonColon) {
            if self.eat(&Tok::LBrace) {
                names = Some(self.name_list()?);
                break;
            }
            path.push(self.ident("a name after `::`")?);
        }
        Ok(Import {
            path,
            names,
            span: self.span_from(start),
        })
    }

    fn name_list(&mut self) -> Res<Vec<Ident>> {
        let mut names = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace) {
                return Ok(names);
            }
            names.push(self.ident("a name to import")?);
            self.skip_newlines();
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RBrace, "`,` or `}`")?;
                return Ok(names);
            }
        }
    }

    fn definition(&mut self, docs: Docs) -> Res<Definition> {
        let keyword = self.bump();
        let Tok::Ident(word) = &keyword.tok else {
            unreachable!("caller checked the keyword")
        };
        let kind = DefKind::from_keyword(word).expect("caller checked the keyword");
        let name = self.ident(&format!("a name for the {word}"))?;
        let params = if self.at(&Tok::Lt) {
            self.type_params(word)?
        } else {
            Vec::new()
        };
        let mut provides = Vec::new();
        if matches!(&self.peek().tok, Tok::Ident(w) if w == "provides") {
            let at = self.bump();
            if !matches!(kind, DefKind::Part | DefKind::Module | DefKind::Interface) {
                return Err(ParseError::new(
                    at.span,
                    format!(
                        "only a part, a module or an interface provides a generic; a {word} does not (`part X provides G`, `module X provides G`, `interface X provides G`)"
                    ),
                ));
            }
            loop {
                provides.push(self.ident("the name of a generic after `provides`")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
                self.skip_newlines();
            }
        }
        let expected = if provides.is_empty() {
            format!("`{{` after the {word} name")
        } else {
            format!("`{{` or `,` after the generics the {word} provides")
        };
        let open = self.expect(&Tok::LBrace, &expected)?;
        let inner_docs = self.inner_docs();
        let (items, variants) = if kind == DefKind::Enum {
            (Vec::new(), self.variants(open.span)?)
        } else {
            let outer = std::mem::replace(&mut self.in_generic, kind == DefKind::Generic);
            let items = self.block(open.span);
            self.in_generic = outer;
            (items?, Vec::new())
        };
        Ok(Definition {
            docs: docs.lines,
            inner_docs,
            kind,
            name,
            provides,
            params,
            items,
            variants,
            span: self.span_from(keyword.span.start),
        })
    }

    /// The variants of an `enum`, up to and including the closing `}`: names separated by
    /// commas or line ends, each with its `///` lines.
    fn variants(&mut self, open: Span) -> Res<Vec<Variant>> {
        let mut variants: Vec<Variant> = Vec::new();
        loop {
            while matches!(self.peek().tok, Tok::Newline | Tok::Semi | Tok::Comma) {
                self.pos += 1;
            }
            let docs = self.outer_docs();
            match self.peek().tok {
                Tok::RBrace | Tok::Eof if !docs.is_empty() => {
                    return Err(ParseError::new(
                        docs.first.expect("docs is not empty"),
                        "this doc comment is not followed by a variant",
                    ));
                }
                Tok::RBrace => {
                    self.bump();
                    return Ok(variants);
                }
                Tok::Eof => return Err(ParseError::new(open, "this `{` is never closed")),
                _ => {}
            }
            let name = self.ident("the name of a variant")?;
            if variants.iter().any(|v| v.name.name == name.name) {
                return Err(ParseError::new(
                    name.span,
                    format!("the variant `{}` is written twice", name.name),
                ));
            }
            variants.push(Variant {
                docs: docs.lines,
                name,
            });
            match self.peek().tok {
                Tok::Comma | Tok::Newline | Tok::Semi | Tok::RBrace => {}
                _ => {
                    return Err(unexpected(
                        self.peek(),
                        "`,`, a new line or `}` after a variant",
                    ));
                }
            }
        }
    }

    /// `physical for Board { ... }`.
    fn physical(&mut self, docs: Docs) -> Res<Physical> {
        let keyword = self.bump();
        self.bump(); // `for`
        let board = self.ident("the name of the board after `physical for`")?;
        let open = self.expect(&Tok::LBrace, "`{` after the board name")?;
        let inner_docs = self.inner_docs();
        let items = self.block(open.span)?;
        Ok(Physical {
            docs: docs.lines,
            inner_docs,
            board,
            items,
            span: self.span_from(keyword.span.start),
        })
    }

    /// Items up to and including the closing `}`.
    fn block(&mut self, open: Span) -> Res<Vec<Item>> {
        let mut items = Vec::new();
        loop {
            self.skip_separators();
            if let Some(e) = self.misplaced_inner_doc() {
                return Err(e);
            }
            let docs = self.outer_docs();
            match self.peek().tok {
                Tok::RBrace | Tok::Eof if !docs.is_empty() => {
                    return Err(ParseError::new(
                        docs.first.expect("docs is not empty"),
                        "this doc comment is not followed by a declaration",
                    ));
                }
                Tok::RBrace => {
                    self.bump();
                    return Ok(items);
                }
                Tok::Eof => return Err(ParseError::new(open, "this `{` is never closed")),
                _ => {}
            }
            items.push(self.item(docs)?);
            self.end_statement(true)?;
        }
    }

    /// The token after the next one, without consuming anything.
    fn peek_second(&self) -> &Tok {
        &self.toks[(self.pos + 1).min(self.toks.len() - 1)].tok
    }

    fn item(&mut self, docs: Docs) -> Res<Item> {
        let start = self.peek().span.start;
        let doc_span = docs.first;
        let docs = docs.lines;
        let only_decls = |item: Item| -> Res<Item> {
            match (doc_span, &item) {
                (
                    Some(at),
                    Item::Bridge(_)
                    | Item::Dc(_)
                    | Item::Catalog(_)
                    | Item::Pins(_)
                    | Item::Tie(_)
                    | Item::Unused(_)
                    | Item::Peripheral(_)
                    | Item::Shared(_)
                    | Item::Lines(_)
                    | Item::Case(_)
                    | Item::Support(_)
                    | Item::Row(_)
                    | Item::Polygon(_)
                    | Item::Constraint(_)
                    | Item::Connect(_)
                    | Item::JoinMap(_),
                ) => Err(ParseError::new(
                    at,
                    "doc comments can only be attached to declarations",
                )),
                _ => Ok(item),
            }
        };

        // `bridge`, `catalog`, `tie`, `unused`, `pins`, `lines`, `peripheral`, `shared` and `case` are keywords at the
        // start of a statement.
        if let Tok::Ident(word) = self.peek().tok.clone() {
            let starts_name = matches!(
                self.peek_second(),
                Tok::LeadingDot | Tok::Ident(_) | Tok::QuotedIdent(_)
            );
            match word.as_str() {
                "bridge" if starts_name => return only_decls(self.bridge(start)?),
                "dc" if matches!(self.peek_second(), Tok::Ident(_)) => {
                    return only_decls(self.dc(start)?);
                }
                "catalog" if starts_name => return only_decls(self.catalog(start)?),
                "tie" if starts_name => return only_decls(self.tie(start)?),
                "unused" if starts_name => return only_decls(self.unused(start)?),
                "peripheral" if matches!(self.peek_second(), Tok::Ident(_)) => {
                    return only_decls(self.peripheral(start)?);
                }
                "shared" if matches!(self.peek_second(), Tok::Ident(_)) => {
                    return only_decls(self.shared(start)?);
                }
                "lines" if matches!(self.peek_second(), Tok::LBrace | Tok::Ident(_)) => {
                    return only_decls(self.lines(start)?);
                }
                "pins" if matches!(self.peek_second(), Tok::LBrace | Tok::Str(_)) => {
                    return only_decls(self.pins(start)?);
                }
                "row" if matches!(self.peek_second(), Tok::LeadingDot) => {
                    return only_decls(self.row(start)?);
                }
                "polygon" if matches!(self.peek_second(), Tok::LParen) => {
                    return only_decls(self.polygon(start)?);
                }
                "support" if matches!(self.peek_second(), Tok::LBrace) => {
                    self.bump();
                    let open = self.bump();
                    let items = self.block(open.span)?;
                    return only_decls(Item::Support(Support {
                        items,
                        span: self.span_from(start),
                    }));
                }
                "case" if matches!(self.peek_second(), Tok::LBrace) => {
                    self.bump();
                    let open = self.bump();
                    let items = self.block(open.span)?;
                    return only_decls(Item::Case(Case {
                        items,
                        span: self.span_from(start),
                    }));
                }
                _ => {}
            }
        }
        let lhs = self.expr()?;
        let t = self.peek().clone();
        match t.tok {
            Tok::Colon => {
                let name = as_path(lhs, "a name before `:`")?;
                self.bump();
                let count = self.group_count()?;
                let ty = if self.at(&Tok::LeadingDot) && count.is_none() {
                    self.request_type()?
                } else {
                    self.type_path()?
                };
                // `optional` after the type is a modifier of the declaration (ADR 0074), the
                // word a pin row also uses. It is read as a plain word, so a member or a type
                // may still be called `optional`.
                let is_optional = matches!(&self.peek().tok, Tok::Ident(w) if w == "optional");
                let optional = is_optional.then(|| self.bump().span);
                let body = if self.at(&Tok::LBrace) {
                    let open = self.bump();
                    Some(self.block(open.span)?)
                } else {
                    None
                };
                if let Some(count) = count
                    && name.segments.len() > 1
                {
                    return Err(ParseError::new(
                        count.span,
                        format!(
                            "a group is declared by one name of its own, such as `.{}: {} x {}`, not by a path into a member",
                            name.segments.last().expect("a path has a segment").name,
                            count.n,
                            ty.segments.last().map_or("Capacitor", |s| s.name.as_str())
                        ),
                    ));
                }
                Ok(Item::Decl(Decl {
                    docs,
                    name,
                    ty,
                    count,
                    optional,
                    body,
                    span: self.span_from(start),
                }))
            }
            Tok::Eq => Err(ParseError::new(
                t.span,
                "there is no `=`: use `&=` to narrow a value, or `==` for an equation that must hold",
            )),
            Tok::AmpEq | Tok::QuestionEq | Tok::EqEq | Tok::GtEq | Tok::LtEq => {
                let op = match t.tok {
                    Tok::AmpEq => CmpOp::Narrow,
                    Tok::QuestionEq => CmpOp::Default,
                    Tok::EqEq => CmpOp::EqEq,
                    Tok::GtEq => CmpOp::Ge,
                    _ => CmpOp::Le,
                };
                self.bump();
                self.skip_newlines();
                let rhs = self.expr()?;
                // `at` and a current after the value: the load a level is measured at.
                let at = if matches!(&self.peek().tok, Tok::Ident(w) if w == "at") {
                    self.bump();
                    self.skip_newlines();
                    Some(self.expr()?)
                } else {
                    None
                };
                only_decls(Item::Constraint(Constraint {
                    lhs,
                    op,
                    rhs,
                    at,
                    span: self.span_from(start),
                }))
            }
            Tok::Tilde | Tok::TildeArrow => {
                let first = as_path(lhs, "a name before `~`")?;
                let mut rest = Vec::new();
                let mut groups = Vec::new();
                loop {
                    let op = match self.peek().tok {
                        Tok::Tilde => ConnectOp::Same,
                        Tok::TildeArrow => ConnectOp::Series,
                        _ => break,
                    };
                    self.bump();
                    self.skip_newlines();
                    let operand_start = self.peek().span.start;
                    if let Some(count) = self.group_count()? {
                        let ty = self.type_path()?;
                        let body = if self.at(&Tok::LBrace) {
                            let open = self.bump();
                            Some(self.block(open.span)?)
                        } else {
                            None
                        };
                        let span = self.span_from(operand_start);
                        groups.push(InlineGroup {
                            at: rest.len(),
                            count,
                            ty,
                            body,
                            span,
                        });
                        // A stand-in until the chain's ends are known; named below.
                        rest.push((
                            op,
                            Path {
                                leading_dot: true,
                                segments: Vec::new(),
                                span,
                            },
                        ));
                        continue;
                    }
                    rest.push((op, self.path()?));
                    let map_follows = self.at(&Tok::LBrace)
                        || matches!(&self.peek().tok, Tok::Ident(w) if w == "except");
                    if map_follows {
                        if rest.len() != 1 || op != ConnectOp::Same {
                            return Err(ParseError::new(
                                self.peek().span,
                                "a rename map or `except` follows a join of two ports with one `~`: write `.a ~ .b { x = y }`",
                            ));
                        }
                        let right = rest.pop().expect("one operand").1;
                        return only_decls(self.member_map(first, right, start)?);
                    }
                }
                for group in &groups {
                    name_inline_group(&first, &mut rest, group)?;
                }
                only_decls(Item::Connect(Connect {
                    first,
                    rest,
                    groups,
                    span: self.span_from(start),
                }))
            }
            _ => Err(unexpected(
                &t,
                "`:`, `&=`, `?=`, `==`, `>=`, `<=`, `~` or `~>`",
            )),
        }
    }

    /// `{ a = b } except { c }` after `.left ~ .right` (ADR 0117).
    fn member_map(&mut self, left: Path, right: Path, start: usize) -> Res<Item> {
        let mut renames = Vec::new();
        if self.at(&Tok::LBrace) {
            let open = self.bump();
            loop {
                self.skip_separators();
                let t = self.peek().clone();
                match &t.tok {
                    Tok::RBrace => {
                        self.bump();
                        break;
                    }
                    Tok::Eof => {
                        return Err(ParseError::new(open.span, "this `{` is never closed"));
                    }
                    Tok::Ident(_) => {
                        let from = self.ident("a member of the left side")?;
                        self.expect(
                            &Tok::Eq,
                            "`=` and the member of the right side it is joined to",
                        )?;
                        let to = self.ident("a member of the right side after `=`")?;
                        renames.push((from, to));
                    }
                    _ => {
                        return Err(unexpected(
                            &t,
                            "a rename such as `gpio4 = io4` (a member of the left side, `=`, a member of the right side) or `}`",
                        ));
                    }
                }
            }
            if renames.is_empty() {
                return Err(ParseError::new(
                    open.span,
                    "an empty rename map; leave it out when every name matches",
                ));
            }
        }
        let mut except = Vec::new();
        if matches!(&self.peek().tok, Tok::Ident(w) if w == "except") {
            self.bump();
            let open = self.expect(&Tok::LBrace, "`{` and the members left out")?;
            loop {
                self.skip_separators();
                let t = self.peek().clone();
                match &t.tok {
                    Tok::RBrace => {
                        self.bump();
                        break;
                    }
                    Tok::Eof => {
                        return Err(ParseError::new(open.span, "this `{` is never closed"));
                    }
                    Tok::Ident(_) => except.push(self.ident("a member left out")?),
                    _ => {
                        return Err(unexpected(
                            &t,
                            "the name of a member left out, such as `gnd`, or `}`",
                        ));
                    }
                }
            }
            if except.is_empty() {
                return Err(ParseError::new(
                    open.span,
                    "an empty `except`; leave it out when no member is left out",
                ));
            }
        }
        Ok(Item::JoinMap(JoinMap {
            left,
            right,
            renames,
            except,
            span: self.span_from(start),
        }))
    }

    /// `row .a1 .. .a12 along x pitch 2.54mm +/- 0.05mm`.
    fn row(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let first = self.path()?;
        self.expect(
            &Tok::DotDot,
            "`..` between the first and the last part of the row",
        )?;
        self.skip_newlines();
        let last = self.path()?;
        let t = self.peek().clone();
        if !matches!(&t.tok, Tok::Ident(w) if w == "along") {
            return Err(unexpected(&t, "`along x` or `along y` after the last part"));
        }
        self.bump();
        let t = self.peek().clone();
        let axis = match &t.tok {
            Tok::Ident(w) if w == "x" => Axis::X,
            Tok::Ident(w) if w == "y" => Axis::Y,
            _ => return Err(unexpected(&t, "`x` or `y` after `along`")),
        };
        let axis_span = self.bump().span;
        let t = self.peek().clone();
        if !matches!(&t.tok, Tok::Ident(w) if w == "pitch") {
            return Err(unexpected(
                &t,
                "`pitch` and the distance between neighbours after the axis",
            ));
        }
        self.bump();
        let pitch = self.tolerance()?;
        Ok(Item::Row(Row {
            first,
            last,
            axis,
            axis_span,
            pitch,
            span: self.span_from(start),
        }))
    }

    /// `polygon (x, y) round R (x, y) arc through (x, y) (x, y) ...`. The run may go on over
    /// newlines: a line that begins with `(`, `round` or `arc` continues it.
    fn polygon(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let mut corners = Vec::new();
        loop {
            let corner_start = self.peek().span.start;
            let at = self.point("a corner `(x, y)`")?;
            let round = if self.continues("round") {
                self.skip_newlines();
                self.bump();
                Some(self.tolerance()?)
            } else {
                None
            };
            let through = if self.continues("arc") {
                self.skip_newlines();
                self.bump();
                let t = self.peek().clone();
                if !matches!(&t.tok, Tok::Ident(w) if w == "through") {
                    return Err(unexpected(&t, "`through (x, y)` after `arc`"));
                }
                self.bump();
                Some(self.point("the point `(x, y)` the arc passes through")?)
            } else {
                None
            };
            corners.push(PolygonCorner {
                at,
                round,
                through,
                span: self.span_from(corner_start),
            });
            if !self.continues("(") {
                break;
            }
            self.skip_newlines();
        }
        Ok(Item::Polygon(Polygon {
            corners,
            span: self.span_from(start),
        }))
    }

    /// Whether the polygon goes on with `word` (or `(`), on this line or at the start of the
    /// next ones.
    fn continues(&self, word: &str) -> bool {
        let mut k = self.pos;
        while self.toks[k].tok == Tok::Newline {
            k += 1;
        }
        match &self.toks[k].tok {
            Tok::LParen => word == "(",
            Tok::Ident(w) => w == word,
            _ => false,
        }
    }

    /// `(x, y)`.
    fn point(&mut self, what: &str) -> Res<Point> {
        let open = self.expect(&Tok::LParen, what)?;
        self.depth += 1;
        let x = self.expr();
        let x = x.and_then(|x| {
            self.expect(&Tok::Comma, "`,` between x and y")?;
            Ok(x)
        });
        let point = x.and_then(|x| {
            let y = self.expr()?;
            Ok((x, y))
        });
        self.depth -= 1;
        let (x, y) = point?;
        self.expect(&Tok::RParen, "`)` after the point's y")?;
        Ok(Point {
            x,
            y,
            span: self.span_from(open.span.start),
        })
    }

    fn bridge(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let from = self.path()?;
        self.expect(&Tok::TildeArrow, "`~>` between the two terminals")?;
        let to = self.path()?;
        Ok(Item::Bridge(Bridge {
            from,
            to,
            span: self.span_from(start),
        }))
    }

    /// `dc KIND OPERAND...`: the operands are paths on one line.
    fn dc(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let kind = self.ident("the kind of element after `dc`")?;
        let mut operands = Vec::new();
        while matches!(
            self.peek().tok,
            Tok::LeadingDot | Tok::Ident(_) | Tok::QuotedIdent(_)
        ) {
            operands.push(self.path()?);
        }
        Ok(Item::Dc(Dc {
            kind,
            operands,
            span: self.span_from(start),
        }))
    }

    /// `unused .processor.usb`: one interface of a part or module, by its path.
    fn unused(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let path = self.path()?;
        if matches!(
            self.peek().tok,
            Tok::Tilde | Tok::TildeArrow | Tok::Colon | Tok::AmpEq | Tok::Ident(_)
        ) {
            return Err(ParseError::new(
                self.peek().span,
                "`unused` names one interface and nothing else: `unused .processor.usb`",
            ));
        }
        Ok(Item::Unused(Unused {
            path,
            span: self.span_from(start),
        }))
    }

    fn tie(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let a = self.path()?;
        self.expect(&Tok::Tilde, "`~` between the two pins of a `tie`")?;
        self.skip_newlines();
        let b = self.path()?;
        if self.at(&Tok::Tilde) || self.at(&Tok::TildeArrow) {
            return Err(ParseError::new(
                self.peek().span,
                "a `tie` joins exactly two pins: `tie .a ~ .b`",
            ));
        }
        Ok(Item::Tie(Tie {
            a,
            b,
            span: self.span_from(start),
        }))
    }

    /// `peripheral NAME [on CLASS] { FUNCTION[:kind] ... }`.
    fn peripheral(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let name = self.ident("the name of the peripheral")?;
        let on = if matches!(&self.peek().tok, Tok::Ident(w) if w == "on") {
            self.bump();
            Some(self.ident("a function after `on`, such as `on APP_GPIO`")?)
        } else {
            None
        };
        let bus = if matches!(&self.peek().tok, Tok::Ident(w) if w == "as") {
            let at = self.peek().span.start;
            self.bump();
            let interface = self.type_path()?;
            let role = match self.peek().tok.clone() {
                Tok::Ident(_) if matches!(self.peek_second(), Tok::ColonColon) => {
                    let role_start = self.peek().span.start;
                    let mut segments = vec![self.ident("an enum name")?];
                    while self.eat(&Tok::ColonColon) {
                        segments.push(self.ident("a name after `::`")?);
                    }
                    let variant = segments.pop().expect("a path has a segment");
                    Some(Expr::Variant {
                        ty: TypePath {
                            segments,
                            args: Vec::new(),
                            of_member: false,
                        },
                        variant,
                        span: self.span_from(role_start),
                    })
                }
                _ => None,
            };
            Some(PeripheralBus {
                interface,
                role,
                span: self.span_from(at),
            })
        } else {
            None
        };
        let open = self.expect(
            &Tok::LBrace,
            match (&on, &bus) {
                (_, Some(_)) => "`{` after the bus that the peripheral is an end of",
                (Some(_), None) => "`{` or `as` after the function that the peripheral is on",
                (None, None) => "`{`, `on` or `as` after the name of the peripheral",
            },
        )?;
        let mut functions: Vec<PinFunction> = Vec::new();
        loop {
            self.skip_separators();
            let t = self.peek().clone();
            match &t.tok {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => return Err(ParseError::new(open.span, "this `{` is never closed")),
                Tok::DocOuter(_) | Tok::DocInner(_) => {
                    return Err(ParseError::new(
                        t.span,
                        "a peripheral takes `//` notes, not doc comments",
                    ));
                }
                Tok::Ident(_) => {
                    let function = self.ident("a function name")?;
                    let kind = self.function_kind(&function)?;
                    let line =
                        if matches!(&self.peek().tok, Tok::Ident(w) if w == "as") {
                            self.bump();
                            Some(self.ident(
                                "the line of the bus that the function is, such as `as sclk`",
                            )?)
                        } else {
                            None
                        };
                    functions.push(PinFunction {
                        name: function,
                        kind,
                        class: None,
                        line,
                    });
                }
                Tok::Str(_) | Tok::Number(_) | Tok::QuotedIdent(_) => {
                    return Err(ParseError::new(
                        t.span,
                        "a function is a plain name such as UARTE0_TX, without quotes",
                    ));
                }
                _ => return Err(unexpected(&t, "a function name or `}`")),
            }
        }
        if functions.is_empty() {
            return Err(ParseError::new(
                name.span,
                format!(
                    "`peripheral {}` has no functions; list at least one in the braces, such as `{}_TX:output`",
                    name.name, name.name
                ),
            ));
        }
        Ok(Item::Peripheral(Peripheral {
            name,
            on,
            bus,
            functions,
            span: self.span_from(start),
        }))
    }

    /// `limit N` after the name of a `shared` resource or the role of a `lines` table, if there
    /// is one: a whole number of at least 1 with no unit.
    fn limit_clause(&mut self) -> Res<Option<(u32, Span)>> {
        if !matches!(&self.peek().tok, Tok::Ident(w) if w == "limit") {
            return Ok(None);
        }
        self.bump();
        let t = self.peek().clone();
        match &t.tok {
            Tok::Number(n) if n.unit.is_none() => {
                let whole =
                    n.value.fract() == 0.0 && n.value >= 1.0 && n.value <= f64::from(u32::MAX);
                if !whole {
                    return Err(ParseError::new(
                        t.span,
                        format!(
                            "a limit is a whole number of at least 1, such as `limit 2`; `{}` is not",
                            n.text
                        ),
                    ));
                }
                self.bump();
                Ok(Some((n.value as u32, t.span)))
            }
            Tok::Number(n) => Err(ParseError::new(
                t.span,
                format!(
                    "a limit is a count, a whole number with no unit, such as `limit 2`; `{}` has one",
                    n.text
                ),
            )),
            _ => Err(unexpected(
                &t,
                "a whole number after `limit`, such as `limit 2`",
            )),
        }
    }

    /// `shared NAME [limit N] { MEMBER ... }`.
    fn shared(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let name = self.ident("the name of the shared resource")?;
        let limit = self.limit_clause()?;
        let open = self.expect(
            &Tok::LBrace,
            if limit.is_some() {
                "`{` after the limit"
            } else {
                "`{` or `limit` after the name of the shared resource"
            },
        )?;
        let mut members: Vec<SharedMember> = Vec::new();
        loop {
            self.skip_separators();
            let t = self.peek().clone();
            match &t.tok {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => return Err(ParseError::new(open.span, "this `{` is never closed")),
                Tok::DocOuter(_) | Tok::DocInner(_) => {
                    return Err(ParseError::new(
                        t.span,
                        "a shared resource takes `//` notes, not doc comments",
                    ));
                }
                Tok::Ident(_) => members.push(SharedMember::Name(self.ident("a member")?)),
                Tok::LeadingDot => members.push(SharedMember::Pin(self.path()?)),
                _ => {
                    return Err(unexpected(
                        &t,
                        "the name of a peripheral or a function, a pin such as `.p10`, or `}`",
                    ));
                }
            }
        }
        if members.is_empty() {
            return Err(ParseError::new(
                name.span,
                format!(
                    "`shared {}` has no members; list the peripherals, functions or pins that share it in braces",
                    name.name
                ),
            ));
        }
        Ok(Item::Shared(Shared {
            name,
            limit,
            members,
            span: self.span_from(start),
        }))
    }

    /// `lines [ENUM::Variant] [limit N] { .line kind [single] [crosses .other] [optional | required] ... }`.
    fn lines(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let role = match self.peek().tok.clone() {
            Tok::Ident(_) if matches!(self.peek_second(), Tok::ColonColon) => {
                let role_start = self.peek().span.start;
                let mut segments = vec![self.ident("an enum name")?];
                while self.eat(&Tok::ColonColon) {
                    segments.push(self.ident("a name after `::`")?);
                }
                let variant = segments.pop().expect("a path has a segment");
                Some(Expr::Variant {
                    ty: TypePath {
                        segments,
                        args: Vec::new(),
                        of_member: false,
                    },
                    variant,
                    span: self.span_from(role_start),
                })
            }
            _ => None,
        };
        let limit = self.limit_clause()?;
        let open = self.expect(
            &Tok::LBrace,
            match (&role, &limit) {
                (_, Some(_)) => "`{` after the limit",
                (Some(_), None) => "`{` or `limit` after the role of the lines",
                (None, None) => "`{`, a role such as `SpiRole::Master` or `limit` after `lines`",
            },
        )?;
        let mut rows = Vec::new();
        loop {
            self.skip_separators();
            let t = self.peek().clone();
            match t.tok {
                Tok::DocOuter(_) | Tok::DocInner(_) => {
                    return Err(ParseError::new(
                        t.span,
                        "a `lines` table takes `//` notes, not doc comments",
                    ));
                }
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => return Err(ParseError::new(open.span, "this `{` is never closed")),
                _ => {
                    rows.push(self.line_row()?);
                    self.end_statement(true)?;
                }
            }
        }
        if rows.is_empty() {
            return Err(ParseError::new(
                self.span_from(start),
                "`lines` has no rows; list the lines of the role, such as `.tx output`",
            ));
        }
        Ok(Item::Lines(Lines {
            role,
            limit,
            rows,
            span: self.span_from(start),
        }))
    }

    /// `.line kind [single] [crosses .other] [optional | required]`, the words in any order.
    fn line_row(&mut self) -> Res<LineRow> {
        let start = self.peek().span.start;
        if !self.at(&Tok::LeadingDot) {
            let t = self.peek().clone();
            return Err(unexpected(
                &t,
                "a line such as `.sclk` at the start of a row",
            ));
        }
        let pin = self.path()?;
        let pin_text = pin.to_source();
        let t = self.peek().clone();
        let kind = match &t.tok {
            Tok::Ident(word) => PinKind::from_word(word).ok_or_else(|| {
                ParseError::new(
                    t.span,
                    format!("unknown pin kind `{word}`; the kinds are {}", kind_list()),
                )
            })?,
            _ => {
                return Err(unexpected(
                    &t,
                    &format!("a pin kind ({}) after `{pin_text}`", kind_list()),
                ));
            }
        };
        self.bump();
        let mut requirement = None;
        let mut single = None;
        let mut crosses = None;
        loop {
            let t = self.peek().clone();
            let Tok::Ident(word) = &t.tok else { break };
            match word.as_str() {
                "optional" | "required" => {
                    if requirement.is_some() {
                        return Err(ParseError::new(
                            t.span,
                            format!("`{pin_text}` already says whether it must be connected"),
                        ));
                    }
                    let r = if word == "optional" {
                        Requirement::Optional
                    } else {
                        Requirement::Required
                    };
                    requirement = Some((r, t.span));
                    self.bump();
                }
                "single" => {
                    if single.is_some() {
                        return Err(ParseError::new(
                            t.span,
                            format!("`{pin_text}` says `single` twice"),
                        ));
                    }
                    single = Some(t.span);
                    self.bump();
                }
                "crosses" => {
                    if crosses.is_some() {
                        return Err(ParseError::new(
                            t.span,
                            format!("`{pin_text}` says `crosses` twice"),
                        ));
                    }
                    self.bump();
                    if !self.at(&Tok::LeadingDot) {
                        let next = self.peek().clone();
                        return Err(unexpected(
                            &next,
                            "the line it crosses to, such as `crosses .rx`",
                        ));
                    }
                    crosses = Some(self.path()?);
                }
                _ => break,
            }
        }
        Ok(LineRow {
            pin,
            kind,
            requirement,
            single,
            crosses,
            span: self.span_from(start),
        })
    }

    /// `pins "PACKAGE" { rows }` or `pins { rows }`.
    fn pins(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let package = match self.peek().tok.clone() {
            Tok::Str(name) => {
                let span = self.bump().span;
                if name.is_empty() {
                    return Err(ParseError::new(span, "a package name cannot be empty"));
                }
                Some((name, span))
            }
            _ => None,
        };
        let what = if package.is_some() {
            "`{` after the package name"
        } else {
            "`{` or a package name in quotes after `pins`"
        };
        let open = self.expect(&Tok::LBrace, what)?;
        let mut rows = Vec::new();
        loop {
            self.skip_separators();
            let t = self.peek().clone();
            match t.tok {
                Tok::DocOuter(_) | Tok::DocInner(_) => {
                    return Err(ParseError::new(
                        t.span,
                        "a pin table takes `//` notes, not doc comments",
                    ));
                }
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Eof => return Err(ParseError::new(open.span, "this `{` is never closed")),
                _ => {
                    rows.push(self.pin_row()?);
                    self.end_statement(true)?;
                }
            }
        }
        Ok(Item::Pins(Pins {
            package,
            rows,
            span: self.span_from(start),
        }))
    }

    /// `.pin kind "NAME" "pad" ["pad" ...] [fn FUNCTION ...] [optional | required]`
    fn pin_row(&mut self) -> Res<PinRow> {
        let start = self.peek().span.start;
        if !self.at(&Tok::LeadingDot) {
            let t = self.peek().clone();
            return Err(unexpected(
                &t,
                "a pin such as `.sense` at the start of a row",
            ));
        }
        let pin = self.path()?;
        let pin_text = pin.to_source();

        let t = self.peek().clone();
        let kind = match &t.tok {
            Tok::Ident(word) => PinKind::from_word(word).ok_or_else(|| {
                ParseError::new(
                    t.span,
                    format!("unknown pin kind `{word}`; the kinds are {}", kind_list()),
                )
            })?,
            _ => {
                return Err(unexpected(
                    &t,
                    &format!("a pin kind ({}) after `{pin_text}`", kind_list()),
                ));
            }
        };
        self.bump();

        let t = self.peek().clone();
        // A generic's row may stop here: the kind is all a provider's row must agree on.
        let contract = self.in_generic
            && match &t.tok {
                Tok::Newline | Tok::Semi | Tok::RBrace | Tok::Eof => true,
                Tok::Ident(w) => w == "optional" || w == "required",
                _ => false,
            };
        if contract {
            let requirement = match &t.tok {
                Tok::Ident(w) if w == "optional" => Some((Requirement::Optional, t.span)),
                Tok::Ident(w) if w == "required" => Some((Requirement::Required, t.span)),
                _ => None,
            };
            if requirement.is_some() {
                self.bump();
            }
            return Ok(PinRow {
                pin,
                kind,
                name: String::new(),
                name_span: t.span,
                pads: Vec::new(),
                functions: Vec::new(),
                requirement,
                span: self.span_from(start),
            });
        }
        let (name, name_span) = match &t.tok {
            Tok::Str(name) => (name.clone(), t.span),
            _ => {
                return Err(unexpected(
                    &t,
                    &format!(
                        "the datasheet name of `{pin_text}` as a quoted string such as \"SENSE\""
                    ),
                ));
            }
        };
        self.bump();

        let mut pads: Vec<Pad> = Vec::new();
        loop {
            let t = self.peek().clone();
            match &t.tok {
                Tok::Str(text) => {
                    self.bump();
                    pads.push(Pad {
                        text: text.clone(),
                        span: t.span,
                    });
                }
                Tok::Number(n) => {
                    return Err(ParseError::new(
                        t.span,
                        format!(
                            "a pad is text: write \"{}\" with quotes, not `{}`",
                            n.text, n.text
                        ),
                    ));
                }
                _ if pads.is_empty() => {
                    return Err(unexpected(
                        &t,
                        &format!("a pad of `{pin_text}` as a quoted string such as \"3\""),
                    ));
                }
                _ => break,
            }
        }

        let mut functions = Vec::new();
        if matches!(&self.peek().tok, Tok::Ident(w) if w == "fn") {
            self.bump();
            loop {
                let t = self.peek().clone();
                match &t.tok {
                    Tok::Ident(w) if w != "optional" && w != "required" => {
                        let name = self.ident("a function name")?;
                        let kind = self.function_kind(&name)?;
                        // `class` right after a function's name (and its kind) is that function's
                        // marker, whatever comes before. So a function that is itself called
                        // `class` is a name only where a name is read: first after `fn`, or after a
                        // function that already has its marker (`fn GPIO class class`).
                        let class = match &self.peek().tok {
                            Tok::Ident(w) if w == "class" => Some(self.bump().span),
                            _ => None,
                        };
                        functions.push(PinFunction {
                            name,
                            kind,
                            class,
                            line: None,
                        });
                    }
                    Tok::Str(_) | Tok::Number(_) | Tok::QuotedIdent(_) => {
                        return Err(ParseError::new(
                            t.span,
                            "a function is a plain name such as UART1_TX, without quotes",
                        ));
                    }
                    _ => break,
                }
            }
            if functions.is_empty() {
                return Err(unexpected(
                    &self.peek().clone(),
                    "a function name after `fn`, such as `fn GPIO UART1_TX`",
                ));
            }
        }

        let t = self.peek().clone();
        let requirement = match &t.tok {
            Tok::Ident(w) if w == "optional" => Some((Requirement::Optional, t.span)),
            Tok::Ident(w) if w == "required" => Some((Requirement::Required, t.span)),
            _ => None,
        };
        if requirement.is_some() {
            self.bump();
        }

        Ok(PinRow {
            pin,
            kind,
            name,
            name_span,
            pads,
            functions,
            requirement,
            span: self.span_from(start),
        })
    }

    /// The kind after a function's name, `UART1_TX:output`, if it has one. The colon touches
    /// both words, so that a function's kind is not mistaken for a later statement.
    fn function_kind(&mut self, name: &Ident) -> Res<Option<(PinKind, Span)>> {
        let colon = self.peek().clone();
        if colon.tok != Tok::Colon {
            return Ok(None);
        }
        if colon.span.start != name.span.end {
            return Err(ParseError::new(
                colon.span,
                format!(
                    "write the kind of function `{}` against its name, as `{}:output` (no spaces)",
                    name.name, name.name
                ),
            ));
        }
        self.bump();
        let t = self.peek().clone();
        let kind = match &t.tok {
            Tok::Ident(word) if t.span.start == colon.span.end => PinKind::from_word(word)
                .ok_or_else(|| {
                    ParseError::new(
                        t.span,
                        format!("unknown pin kind `{word}`; the kinds are {}", kind_list()),
                    )
                })?,
            _ => {
                return Err(unexpected(
                    &t,
                    &format!("a pin kind ({}) right after `{}:`", kind_list(), name.name),
                ));
            }
        };
        self.bump();
        Ok(Some((kind, t.span)))
    }

    fn catalog(&mut self, start: usize) -> Res<Item> {
        self.bump();
        let name = self.ident("a catalog name")?;
        let key = if matches!(&self.peek().tok, Tok::Ident(w) if w == "by") {
            self.bump();
            Some(self.path()?)
        } else {
            None
        };
        self.expect(
            &Tok::LBrace,
            if key.is_some() {
                "`{` after the key of the catalog"
            } else {
                "`{` or `by` after the catalog name"
            },
        )?;
        let mut fields = Vec::new();
        let mut optional = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace) {
                break;
            }
            fields.push(self.path()?);
            // `optional` after a column is a modifier of the column (ADR 0078), the word of
            // a member (ADR 0074) and of a `pins` row (ADR 0016). It must be on the line of
            // the column: a column is never called `optional` without its leading dot.
            let is_optional = matches!(&self.peek().tok, Tok::Ident(w) if w == "optional");
            optional.push(is_optional.then(|| self.bump().span));
            self.skip_newlines();
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RBrace, "`,` or `}`")?;
                break;
            }
        }
        Ok(Item::Catalog(Catalog {
            name,
            key,
            fields,
            optional,
            span: self.span_from(start),
        }))
    }

    /// `.u.GPIO` after the colon of a declaration: a request for a pin of the part `.u` that
    /// offers the capability `GPIO` (the last name). It needs a part and a capability.
    fn request_type(&mut self) -> Res<TypePath> {
        let path = self.path()?;
        if path.segments.len() < 2 {
            return Err(ParseError::new(
                path.span,
                "a pin request names a part and a capability, such as `.u.GPIO`",
            ));
        }
        Ok(TypePath {
            segments: path.segments,
            args: Vec::new(),
            of_member: true,
        })
    }

    fn type_path(&mut self) -> Res<TypePath> {
        let mut segments = vec![self.ident("a type name")?];
        while self.eat(&Tok::ColonColon) {
            segments.push(self.ident("a name after `::`")?);
        }
        // `Buck<Lt8641>`: the parts for a module's type parameters (ADR 0115).
        let mut args = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                args.push(self.type_path()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::Gt, "`>` or `,` after a type argument")?;
        }
        Ok(TypePath {
            segments,
            args,
            of_member: false,
        })
    }

    /// `<Ic: BuckIc, ...>` after a module's name, from the `<` on: each parameter with the
    /// generic that bounds it (ADR 0115).
    fn type_params(&mut self, word: &str) -> Res<Vec<TypeParam>> {
        let open = self.bump();
        if word != "module" {
            return Err(ParseError::new(
                open.span,
                format!(
                    "only a module takes type parameters; a {word} does not (`module Buck<Ic: BuckIc>`)"
                ),
            ));
        }
        let mut params = Vec::new();
        loop {
            let name = self.ident("the name of a type parameter, such as `Ic`")?;
            self.expect(
                &Tok::Colon,
                &format!(
                    "`:` and the generic that bounds `{}`, as in `{}: BuckIc`",
                    name.name, name.name
                ),
            )?;
            let bound = self.ident("the name of the generic that bounds the parameter")?;
            params.push(TypeParam { name, bound });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::Gt, "`>` or `,` after a type parameter")?;
        Ok(params)
    }

    /// `4 x` before a type: the count of a group (ADR 0119), or `None` when the next token is
    /// not a number followed by `x`.
    fn group_count(&mut self) -> Res<Option<Count>> {
        let t = self.peek().clone();
        let Tok::Number(n) = &t.tok else {
            return Ok(None);
        };
        if n.unit.as_deref() == Some("x") {
            return Err(ParseError::new(
                t.span,
                format!(
                    "write the count of a group with a space before and after the `x`: `{} x Capacitor`",
                    n.text.trim_end_matches('x')
                ),
            ));
        }
        if !matches!(self.peek_second(), Tok::Ident(w) if w == "x") {
            return Ok(None);
        }
        self.bump();
        let x = self.bump();
        let span = Span::new(t.span.start, x.span.end);
        let whole = n.unit.is_none() && n.value.fract() == 0.0 && n.value >= 0.0;
        if !whole {
            return Err(ParseError::new(
                t.span,
                format!(
                    "the count of a group is a whole number with no unit, such as `4 x`, not `{}`",
                    n.text
                ),
            ));
        }
        if n.value < 2.0 {
            return Err(ParseError::new(
                span,
                format!(
                    "a group has at least two members; `{} x` is not one, so declare the member alone (`.c: Capacitor`)",
                    n.text
                ),
            ));
        }
        if n.value > f64::from(MAX_GROUP) {
            return Err(ParseError::new(
                t.span,
                format!(
                    "a group has at most {MAX_GROUP} members, and `{}` is more",
                    n.text
                ),
            ));
        }
        Ok(Some(Count {
            n: n.value as u32,
            span,
        }))
    }

    /// `[2]` touching the name before it: the index of one member of a group (ADR 0119),
    /// which becomes part of the name (`decoupling[2]`).
    fn index_suffix(&mut self, ident: &mut Ident) -> Res<()> {
        let open = self.peek().clone();
        if open.tok != Tok::LBracket || open.span.start != ident.span.end {
            return Ok(());
        }
        self.bump();
        let t = self.peek().clone();
        let index = match &t.tok {
            Tok::Number(n) if n.unit.is_none() && n.value.fract() == 0.0 && n.value >= 0.0 => {
                n.value as u64
            }
            _ => {
                return Err(unexpected(
                    &t,
                    "the index of a member of a group, a whole number such as `[0]`",
                ));
            }
        };
        self.bump();
        let close = self.expect(&Tok::RBracket, "`]` after the index")?;
        ident.name = format!("{}[{index}]", ident.name);
        ident.span = Span::new(ident.span.start, close.span.end);
        Ok(())
    }

    fn path(&mut self) -> Res<Path> {
        let start = self.peek().span.start;
        let leading_dot = self.eat(&Tok::LeadingDot);
        let mut first = self.ident("a name")?;
        self.index_suffix(&mut first)?;
        let mut segments = vec![first];
        while self.eat(&Tok::Dot) {
            let mut next = self.ident("a name after `.`")?;
            self.index_suffix(&mut next)?;
            segments.push(next);
        }
        Ok(Path {
            leading_dot,
            segments,
            span: self.span_from(start),
        })
    }

    // expr := range
    // range := tol ("..", tol)?       tol := add ("+/-" add)?
    // add := mul (("+" | "-") mul)*   mul := unary (("*" | "/") unary)*
    fn expr(&mut self) -> Res<Expr> {
        let lo = self.tolerance()?;
        if self.eat(&Tok::DotDot) {
            self.skip_newlines();
            let hi = self.tolerance()?;
            let span = Span::new(lo.span().start, hi.span().end);
            return Ok(Expr::Range {
                lo: Box::new(lo),
                hi: Box::new(hi),
                span,
            });
        }
        Ok(lo)
    }

    fn tolerance(&mut self) -> Res<Expr> {
        let center = self.additive()?;
        if self.eat(&Tok::PlusMinus) {
            self.skip_newlines();
            let tol = self.additive()?;
            let span = Span::new(center.span().start, tol.span().end);
            return Ok(Expr::Tolerance {
                center: Box::new(center),
                tol: Box::new(tol),
                span,
            });
        }
        Ok(center)
    }

    fn additive(&mut self) -> Res<Expr> {
        let mut lhs = self.multiplicative()?;
        loop {
            let op = match self.peek().tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(lhs),
            };
            self.bump();
            self.skip_newlines();
            let rhs = self.multiplicative()?;
            lhs = binary(op, lhs, rhs);
        }
    }

    fn multiplicative(&mut self) -> Res<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek().tok {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                _ => return Ok(lhs),
            };
            self.bump();
            self.skip_newlines();
            let rhs = self.unary()?;
            lhs = binary(op, lhs, rhs);
        }
    }

    fn unary(&mut self) -> Res<Expr> {
        if self.at(&Tok::Minus) {
            let start = self.bump().span.start;
            let inner = self.unary()?;
            return Ok(Expr::Neg(Box::new(inner), self.span_from(start)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Res<Expr> {
        let t = self.peek().clone();
        match t.tok {
            Tok::Number(n) => {
                self.bump();
                Ok(Expr::Number(n, t.span))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Str(s, t.span))
            }
            // `Enum::Variant`, with a module path before it if the enum is qualified.
            Tok::Ident(_) if matches!(self.peek_second(), Tok::ColonColon) => {
                let start = t.span.start;
                let mut segments = vec![self.ident("a type name")?];
                while self.eat(&Tok::ColonColon) {
                    segments.push(self.ident("a name after `::`")?);
                }
                let variant = segments.pop().expect("a path has a segment");
                Ok(Expr::Variant {
                    ty: TypePath {
                        segments,
                        args: Vec::new(),
                        of_member: false,
                    },
                    variant,
                    span: self.span_from(start),
                })
            }
            // `name(args)`: a call. A name followed at once by `(` is not a path.
            Tok::Ident(_) if matches!(self.peek_second(), Tok::LParen) => {
                let start = t.span.start;
                let name = self.ident("a function name")?;
                self.bump();
                self.depth += 1;
                let mut args = Vec::new();
                if !self.at(&Tok::RParen) {
                    loop {
                        args.push(self.expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&Tok::RParen, "`,` or `)`")?;
                self.depth -= 1;
                Ok(Expr::Call {
                    name,
                    args,
                    span: self.span_from(start),
                })
            }
            Tok::LeadingDot | Tok::Ident(_) | Tok::QuotedIdent(_) => Ok(Expr::Path(self.path()?)),
            Tok::LParen => {
                self.bump();
                self.depth += 1;
                let inner = self.expr()?;
                let close = self.expect(&Tok::RParen, "`)`")?;
                self.depth -= 1;
                Ok(Expr::Paren(
                    Box::new(inner),
                    Span::new(t.span.start, close.span.end),
                ))
            }
            _ => Err(unexpected(&t, "a value")),
        }
    }
}

/// `power, input, output, bidirectional, passive, no_connect, tri_state and open_drain`
fn kind_list() -> String {
    let words: Vec<&str> = PinKind::ALL.iter().map(|k| k.word()).collect();
    let (last, init) = words.split_last().expect("there are kinds");
    format!("{} and {last}", init.join(", "))
}

fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = Span::new(lhs.span().start, rhs.span().end);
    Expr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span,
    }
}

/// The most members a group may have (ADR 0119): more is a mistake in the count.
pub const MAX_GROUP: u32 = 1000;

/// Give a group written in a chain its name, from its kind and the chain's two ends, and check
/// that it stands between two ends: `.v.pos ~> 2 x Capacitor ~> .v.neg` is the group
/// `capacitor_v_pos_v_neg` (ADR 0119).
fn name_inline_group(first: &Path, rest: &mut [(ConnectOp, Path)], group: &InlineGroup) -> Res<()> {
    let shape = "a group written in a chain stands alone between the chain's two ends, each member in parallel: `.v.pos ~> 2 x Capacitor { ... } ~> .v.neg`";
    if rest.len() != 2 || group.at != 0 || rest.iter().any(|(op, _)| *op != ConnectOp::Series) {
        return Err(ParseError::new(group.span, shape));
    }
    let end = &rest[1].1;
    if end.segments.is_empty() {
        return Err(ParseError::new(group.span, shape));
    }
    let kind = group.ty.segments.last().expect("a type has a name");
    let mut name = snake_case(&kind.name);
    for segment in first.segments.iter().chain(&end.segments) {
        name.push('_');
        name.push_str(&segment.name);
    }
    rest[0].1.segments = vec![Ident {
        name,
        quoted: false,
        span: group.span,
    }];
    Ok(())
}

/// `ferrite_bead` for `FerriteBead`.
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn as_path(expr: Expr, what: &str) -> Res<Path> {
    match expr {
        Expr::Path(p) => Ok(p),
        other => Err(ParseError::new(other.span(), format!("expected {what}"))),
    }
}

fn unexpected(found: &Token, what: &str) -> ParseError {
    ParseError::new(
        found.span,
        format!("expected {what}, found {}", describe(&found.tok)),
    )
}

fn describe(tok: &Tok) -> String {
    let symbol = |s: &str| format!("`{s}`");
    match tok {
        Tok::Ident(s) => symbol(s),
        Tok::QuotedIdent(s) => format!("`'{s}'`"),
        Tok::Number(n) => symbol(&n.text),
        Tok::Str(_) => "a string".into(),
        Tok::LBrace => symbol("{"),
        Tok::RBrace => symbol("}"),
        Tok::LParen => symbol("("),
        Tok::RParen => symbol(")"),
        Tok::LBracket => symbol("["),
        Tok::RBracket => symbol("]"),
        Tok::Comma => symbol(","),
        Tok::Semi => symbol(";"),
        Tok::Colon => symbol(":"),
        Tok::ColonColon => symbol("::"),
        Tok::Dot | Tok::LeadingDot => symbol("."),
        Tok::DotDot => symbol(".."),
        Tok::Tilde => symbol("~"),
        Tok::TildeArrow => symbol("~>"),
        Tok::AmpEq => symbol("&="),
        Tok::QuestionEq => symbol("?="),
        Tok::Eq => symbol("="),
        Tok::EqEq => symbol("=="),
        Tok::GtEq => symbol(">="),
        Tok::LtEq => symbol("<="),
        Tok::Lt => symbol("<"),
        Tok::Gt => symbol(">"),
        Tok::Plus => symbol("+"),
        Tok::Minus => symbol("-"),
        Tok::Star => symbol("*"),
        Tok::Slash => symbol("/"),
        Tok::PlusMinus => symbol("+/-"),
        Tok::DocOuter(_) => "a doc comment".into(),
        Tok::DocInner(_) => "an inner doc comment".into(),
        Tok::Newline => "end of line".into(),
        Tok::Eof => "end of file".into(),
    }
}

fn is_import_or_end(tok: &Tok) -> bool {
    matches!(tok, Tok::Eof) || matches!(tok, Tok::Ident(k) if k == "import")
}
