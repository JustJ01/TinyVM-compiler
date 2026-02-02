use crate::{
    ast::{BinOp, Expr, Stmt},
    token::Token,
};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();

        while !matches!(self.peek(), Token::Eof) {
            if matches!(self.peek(), Token::Newline) {
                self.advance();
                continue;
            }
            stmts.push(self.parse_stmt());
        }

        stmts
    }

    fn parse_stmt(&mut self) -> Stmt {
        match self.peek() {
            Token::While => self.parse_while(),
            Token::Print => self.parse_print(),
            Token::Ident(_) => self.parse_assign(),
            _ => panic!("Unexpected token: {:?}", self.peek()),
        }
    }

    fn parse_print(&mut self) -> Stmt {
        self.advance(); // print
        let expr = self.parse_expr();
        self.consume_newline();
        Stmt::Print(expr)
    }

    fn parse_assign(&mut self) -> Stmt {
        let name = if let Token::Ident(s) = self.advance() {
            s.clone()
        } else {
            unreachable!()
        };

        self.expect(Token::Assign);
        let value = self.parse_expr();
        self.consume_newline();

        Stmt::Assign { name, value }
    }

    fn parse_while(&mut self) -> Stmt {
        self.advance(); // while
        let condition = self.parse_expr();
        self.expect(Token::Colon);
        self.expect(Token::Newline);

        let mut body = Vec::new();
        while !matches!(self.peek(), Token::Eof | Token::While) {
            body.push(self.parse_stmt());
        }

        Stmt::While { condition, body }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_equality()
    }

    fn parse_equality(&mut self) -> Expr {
        let mut expr = self.parse_comparison();

        while matches!(self.peek(), Token::EqEq) {
            self.advance();
            let right = self.parse_comparison();
            expr = Expr::Binary {
                left: Box::new(expr),
                op: BinOp::EqEq,
                right: Box::new(right),
            };
        }

        expr
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_term();

        while matches!(self.peek(), Token::Lt | Token::Gt) {
            let op = match self.advance() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                _ => unreachable!(),
            };
            let right = self.parse_term();
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        expr
    }

    fn parse_term(&mut self) -> Expr {
        let mut expr = self.parse_factor();

        while matches!(self.peek(), Token::Plus | Token::Minus) {
            let op = match self.advance() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_factor();
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        expr
    }

    fn parse_factor(&mut self) -> Expr {
        let mut expr = self.parse_primary();

        while matches!(self.peek(), Token::Star | Token::Slash) {
            let op = match self.advance() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_primary();
            expr = Expr::Binary {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.advance() {
            Token::Int(n) => Expr::Int(*n),
            Token::Ident(s) => Expr::Var(s.clone()),
            Token::LParen => {
                let expr = self.parse_expr();
                self.expect(Token::RParen);
                expr
            }
            t => panic!("Unexpected token in expression: {:?}", t),
        }
    }

    fn expect(&mut self, token: Token) {
        if self.peek() == &token {
            self.advance();
        } else {
            panic!("Expected {:?}, got {:?}", token, self.peek());
        }
    }

    fn consume_newline(&mut self) {
        if matches!(self.peek(), Token::Newline) {
            self.advance();
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Token {
        self.pos += 1;
        &self.tokens[self.pos - 1]
    }

    fn match_token(&mut self, expected: &Token) -> bool {
        if self.peek() == expected {
            self.advance();
            true
        } else {
            false
        }
    }
}
