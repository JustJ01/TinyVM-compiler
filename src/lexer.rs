use crate::token::Token;

pub struct Lexer {
    input: Vec<char>,
    pos: usize,

    indent_stack: Vec<usize>,
    pending_dedents: usize,
    line_start: bool,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            input: source.chars().collect(),
            pos: 0,
            indent_stack: vec![0],
            pending_dedents: 0,
            line_start: true,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    // Only skip spaces/tabs INSIDE a line (not indentation)
    fn skip_inline_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self) -> i32 {
        let mut num = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        num.parse().unwrap()
    }

    fn read_ident(&mut self) -> String {
        let mut ident = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        ident
    }

    pub fn next_token(&mut self) -> Token {

        // --- Emit pending dedents first ---
        if self.pending_dedents > 0 {
            self.pending_dedents -= 1;
            return Token::Dedent;
        }

        // --- Handle indentation at start of line ---
        if self.line_start {
            let mut spaces = 0;

            while let Some(' ') = self.peek() {
                spaces += 1;
                self.advance();
            }

            self.line_start = false;

            let current = *self.indent_stack.last().unwrap();

            if spaces > current {
                self.indent_stack.push(spaces);
                return Token::Indent;
            }

            if spaces < current {
                while let Some(&top) = self.indent_stack.last() {
                    if spaces < top {
                        self.indent_stack.pop();
                        self.pending_dedents += 1;
                    } else {
                        break;
                    }
                }

                if self.pending_dedents > 0 {
                    self.pending_dedents -= 1;
                    return Token::Dedent;
                }
            }
        }

        // --- Skip spaces inside line ---
        self.skip_inline_whitespace();

        match self.peek() {

            // -------- NEWLINE --------
            Some('\n') => {
                self.advance();
                self.line_start = true;
                Token::Newline
            }

            // Windows CRLF support
            Some('\r') => {
                self.advance();
                if self.peek() == Some('\n') {
                    self.advance();
                }
                self.line_start = true;
                Token::Newline
            }

            // -------- OPERATORS --------
            Some('+') => { self.advance(); Token::Plus }
            Some('-') => { self.advance(); Token::Minus }
            Some('*') => { self.advance(); Token::Star }
            Some('/') => { self.advance(); Token::Slash }
            Some('(') => { self.advance(); Token::LParen }
            Some(')') => { self.advance(); Token::RParen }
            Some(':') => { self.advance(); Token::Colon }

            Some('=') => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Assign
                }
            }

            Some('<') => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Le
                } else {
                    Token::Lt
                }
            }

            Some('>') => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Ge
                } else {
                    Token::Gt
                }
            }

            // -------- NUMBERS --------
            Some(ch) if ch.is_ascii_digit() => {
                let value = self.read_number();
                Token::Int(value)
            }

            // -------- IDENTIFIERS / KEYWORDS --------
            Some(ch) if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_ident();

                match ident.as_str() {
                    "while" => Token::While,
                    "print" => Token::Print,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "elseif" => Token::ElseIf,
                    _ => Token::Ident(ident),
                }
            }

            // -------- EOF handling with remaining dedents --------
            None => {
                if self.indent_stack.len() > 1 {
                    self.indent_stack.pop();
                    return Token::Dedent;
                }
                Token::Eof
            }

            // -------- UNKNOWN --------
            Some(ch) => panic!("Unexpected character: {:?}", ch),
        }
    }
}
