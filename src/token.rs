#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    While,
    Print,

    // Identifiers and literals
    Ident(String),
    Int(i32),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,

    Lt,
    Gt,
    EqEq,

    Assign, // =

    // Delimiters
    LParen,
    RParen,
    Colon,

    // Newlines / structure
    Newline,
    Indent,
    Dedent,
    Eof,
}
