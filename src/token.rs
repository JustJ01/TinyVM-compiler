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
    Le,
    Ge,

    If,
    Else,
    ElseIf,

    Assign, // =

    // Delimiters
    LParen,
    RParen,
    Colon,

    Indent,
    Dedent,

    // Newlines
    Newline,
    Eof,
}
