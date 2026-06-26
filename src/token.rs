#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    While,
    Print,
    Native,
    Break,
    Continue,
    And,
    Or,
    Not,

    // Identifiers and literals
    Ident(String),
    Int(i32),
    Str(String),

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
    LBracket,
    RBracket,
    Colon,

    Indent,
    Dedent,

    Func,
    Return,
    Comma,
    Hash,

    // Newlines
    Newline,
    Eof,
}
