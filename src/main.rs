mod ast;
mod lexer;
mod parser;
mod token;

use lexer::Lexer;
use parser::Parser;
use std::{env, fs};
use token::Token;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo run -- <source_file>");
        return;
    }

    let file_path = &args[1];

    let source = fs::read_to_string(file_path).expect("Failed to read source file");

    let mut lexer = Lexer::new(&source);

    let mut tokens: Vec<Token> = Vec::new();

    loop {
        let token = lexer.next_token();
        tokens.push(token);

        if tokens[tokens.len() - 1] == Token::Eof {
            break;
        }
    }

    println!("Tokens: {:#?}", tokens);

    let mut parser = Parser::new(tokens);
    let parsed_program = parser.parse_program();

    println!("Parsed program: {:#?}", parsed_program);
}
