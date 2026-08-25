use serde::{Deserialize, Serialize};
use thiserror::Error;
use voidspace_index::NodeSnapshot;
use voidspace_model::{NodeFlags, NodeKind};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Predicate(Predicate),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Field {
    Name,
    Ext,
    Path,
    Type,
    Size,
    Allocated,
    Attr,
    Bare,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Operator {
    Contains,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Glob,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Predicate {
    pub field: Field,
    pub operator: Operator,
    pub value: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("filter error at character {offset}: {message}")]
pub struct FilterError {
    pub offset: usize,
    pub message: String,
}

#[derive(Clone, Copy)]
pub struct FilterContext<'a> {
    pub node: &'a NodeSnapshot,
    pub path: &'a str,
}

pub fn parse(input: &str) -> Result<Expr, FilterError> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Ok(Expr::Predicate(Predicate {
            field: Field::Bare,
            operator: Operator::Contains,
            value: String::new(),
        }));
    }
    let mut parser = Parser { tokens, cursor: 0 };
    let expression = parser.parse_or()?;
    if parser.cursor != parser.tokens.len() {
        return Err(parser.error("unexpected token"));
    }
    Ok(expression)
}

pub fn matches(expression: &Expr, context: FilterContext<'_>) -> bool {
    match expression {
        Expr::Predicate(predicate) => matches_predicate(predicate, context),
        Expr::Not(inner) => !matches(inner, context),
        Expr::And(left, right) => matches(left, context) && matches(right, context),
        Expr::Or(left, right) => matches(left, context) || matches(right, context),
    }
}

fn matches_predicate(predicate: &Predicate, context: FilterContext<'_>) -> bool {
    let name = context.node.name.display_escaped();
    match predicate.field {
        Field::Name | Field::Bare => compare_text(&name, predicate.operator, &predicate.value),
        Field::Ext => {
            let extension = name.rsplit_once('.').map_or("", |(_, ext)| ext);
            compare_text(extension, predicate.operator, &predicate.value)
        }
        Field::Path => compare_text(context.path, predicate.operator, &predicate.value),
        Field::Type => {
            let kind = match context.node.kind {
                NodeKind::File => "file",
                NodeKind::Directory => "folder",
                NodeKind::Stream => "stream",
                NodeKind::FreeSpace => "free",
                NodeKind::Unknown => "unknown",
            };
            compare_text(kind, predicate.operator, &predicate.value)
        }
        Field::Size => compare_number(context.node.logical, predicate),
        Field::Allocated => compare_number(context.node.allocated, predicate),
        Field::Attr => {
            let flag = match predicate.value.to_ascii_lowercase().as_str() {
                "readonly" => NodeFlags::READONLY,
                "hidden" => NodeFlags::HIDDEN,
                "system" => NodeFlags::SYSTEM,
                "compressed" => NodeFlags::COMPRESSED,
                "sparse" => NodeFlags::SPARSE,
                "restricted" => NodeFlags::RESTRICTED,
                "reparse" => NodeFlags::REPARSE,
                "shared" => NodeFlags::SHARED_ALLOCATION,
                _ => return false,
            };
            let present = context.node.flags.contains(flag);
            if predicate.operator == Operator::NotEqual {
                !present
            } else {
                present
            }
        }
    }
}

fn compare_text(actual: &str, operator: Operator, expected: &str) -> bool {
    let actual = actual.to_lowercase();
    let expected = expected.to_lowercase();
    match operator {
        Operator::Contains => actual.contains(&expected),
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::Glob => glob_match(expected.as_bytes(), actual.as_bytes()),
        _ => false,
    }
}

fn compare_number(actual: u64, predicate: &Predicate) -> bool {
    let Ok(expected) = parse_bytes(&predicate.value) else {
        return false;
    };
    match predicate.operator {
        Operator::Equal => actual == expected,
        Operator::NotEqual => actual != expected,
        Operator::Greater => actual > expected,
        Operator::GreaterEqual => actual >= expected,
        Operator::Less => actual < expected,
        Operator::LessEqual => actual <= expected,
        _ => false,
    }
}

pub fn parse_bytes(value: &str) -> Result<u64, FilterError> {
    let split = value
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(value.len());
    let number: f64 = value[..split].parse().map_err(|_| FilterError {
        offset: 0,
        message: "invalid size number".into(),
    })?;
    let suffix = value[split..].to_ascii_lowercase();
    let multiplier = match suffix.as_str() {
        "" | "b" => 1_f64,
        "kb" => 1_000_f64,
        "mb" => 1_000_000_f64,
        "gb" => 1_000_000_000_f64,
        "tb" => 1_000_000_000_000_f64,
        "kib" => 1_024_f64,
        "mib" => 1_048_576_f64,
        "gib" => 1_073_741_824_f64,
        "tib" => 1_099_511_627_776_f64,
        _ => {
            return Err(FilterError {
                offset: split,
                message: "unknown size suffix".into(),
            });
        }
    };
    let bytes = number * multiplier;
    if !bytes.is_finite() || bytes < 0.0 || bytes > u64::MAX as f64 {
        return Err(FilterError {
            offset: 0,
            message: "size is out of range".into(),
        });
    }
    Ok(bytes.round() as u64)
}

fn glob_match(pattern: &[u8], value: &[u8]) -> bool {
    let (mut p, mut v, mut star, mut retry) = (0, 0, None, 0);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            retry = v;
            p += 1;
        } else if let Some(star_position) = star {
            p = star_position + 1;
            retry += 1;
            v = retry;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Token {
    text: String,
    offset: usize,
}

fn tokenize(input: &str) -> Result<Vec<Token>, FilterError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '"' {
            let mut value = String::new();
            let mut closed = false;
            while let Some((_, next)) = chars.next() {
                match next {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => {
                        let Some((_, escaped)) = chars.next() else {
                            break;
                        };
                        value.push(escaped);
                    }
                    _ => value.push(next),
                }
            }
            if !closed {
                return Err(FilterError {
                    offset,
                    message: "unterminated quote".into(),
                });
            }
            tokens.push(Token {
                text: value,
                offset,
            });
            continue;
        }
        if "():~><=!".contains(ch) {
            let mut text = ch.to_string();
            if matches!(ch, '>' | '<' | '!') && chars.peek().is_some_and(|(_, next)| *next == '=') {
                text.push('=');
                chars.next();
            }
            tokens.push(Token { text, offset });
            continue;
        }
        let mut text = ch.to_string();
        while let Some((_, next)) = chars.peek() {
            if next.is_whitespace() || "():~><=!\"".contains(*next) {
                break;
            }
            text.push(*next);
            chars.next();
        }
        tokens.push(Token { text, offset });
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn parse_or(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.parse_and()?;
        while self.consume_keyword("OR") {
            left = Expr::Or(Box::new(left), Box::new(self.parse_and()?));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, FilterError> {
        let mut left = self.parse_unary()?;
        loop {
            let explicit = self.consume_keyword("AND");
            let implicit = self
                .peek()
                .is_some_and(|token| !token.text.eq_ignore_ascii_case("OR") && token.text != ")")
                && !explicit;
            if explicit || implicit {
                left = Expr::And(Box::new(left), Box::new(self.parse_unary()?));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, FilterError> {
        if self.consume_keyword("NOT") {
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        if self.consume("(") {
            let inner = self.parse_or()?;
            if !self.consume(")") {
                return Err(self.error("expected ')'"));
            }
            return Ok(inner);
        }
        self.parse_predicate()
    }

    fn parse_predicate(&mut self) -> Result<Expr, FilterError> {
        let first = self
            .next()
            .ok_or_else(|| self.error("expected predicate"))?;
        let field = field_from(&first.text);
        if field.is_none() {
            return Ok(Expr::Predicate(Predicate {
                field: Field::Bare,
                operator: Operator::Contains,
                value: first.text,
            }));
        }
        let operator_token = self.next().ok_or_else(|| self.error("expected operator"))?;
        let operator = operator_from(&operator_token.text).ok_or_else(|| FilterError {
            offset: operator_token.offset,
            message: "invalid operator".into(),
        })?;
        let value = self.next().ok_or_else(|| self.error("expected value"))?;
        Ok(Expr::Predicate(Predicate {
            field: field.unwrap(),
            operator,
            value: value.text,
        }))
    }

    fn consume(&mut self, expected: &str) -> bool {
        if self.peek().is_some_and(|token| token.text == expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn consume_keyword(&mut self, expected: &str) -> bool {
        if self
            .peek()
            .is_some_and(|token| token.text.eq_ignore_ascii_case(expected))
        {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned();
        self.cursor += usize::from(token.is_some());
        token
    }

    fn error(&self, message: &str) -> FilterError {
        FilterError {
            offset: self.peek().map_or(0, |token| token.offset),
            message: message.into(),
        }
    }
}

fn field_from(value: &str) -> Option<Field> {
    match value.to_ascii_lowercase().as_str() {
        "name" => Some(Field::Name),
        "ext" => Some(Field::Ext),
        "path" => Some(Field::Path),
        "type" => Some(Field::Type),
        "size" => Some(Field::Size),
        "allocated" => Some(Field::Allocated),
        "attr" => Some(Field::Attr),
        _ => None,
    }
}

fn operator_from(value: &str) -> Option<Operator> {
    match value {
        ":" => Some(Operator::Contains),
        "=" => Some(Operator::Equal),
        "!=" => Some(Operator::NotEqual),
        ">" => Some(Operator::Greater),
        ">=" => Some(Operator::GreaterEqual),
        "<" => Some(Operator::Less),
        "<=" => Some(Operator::LessEqual),
        "~" => Some(Operator::Glob),
        _ => None,
    }
}
