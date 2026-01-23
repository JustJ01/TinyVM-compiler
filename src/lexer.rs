use crate::token::Token;

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            input: source.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        self.pos += 1;
        ch
    }

    fn skip_whitespace(&mut self) {
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

        num.parse::<i32>().unwrap()
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
    self.skip_whitespace();

    match self.peek() {
        Some(ch) => match ch {
            '\n' => {
                self.advance();
                Token::Newline
            }

            '\r' => {
                self.advance(); // Windows CRLF support
                self.next_token()
            }

            '+' => {
                self.advance();
                Token::Plus
            }

            '-' => {
                self.advance();
                Token::Minus
            }

            '*' => {
                self.advance();
                Token::Star
            }

            '/' => {
                self.advance();
                Token::Slash
            }

            '(' => {
                self.advance();
                Token::LParen
            }
            ')' => {
                self.advance();
                Token::RParen
            }

            ':' => {
                self.advance();
                Token::Colon
            }

            '=' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Assign
                }
            }

            '<' => {
                self.advance();
                Token::Lt
            }

            '>' => {
                self.advance();
                Token::Gt
            }

            ch if ch.is_ascii_digit() => {
                let value = self.read_number();
                Token::Int(value)
            }

            ch if ch.is_alphabetic() || ch == '_' => {
                let ident = self.read_ident();

                match ident.as_str() {
                    "while" => Token::While,
                    "print" => Token::Print,
                    _ => Token::Ident(ident),
                }
            }

            _ => {
                panic!("Unexpected character: {:?}", ch);
                }
            },

            None => Token::Eof,
        }
    }
        
}

