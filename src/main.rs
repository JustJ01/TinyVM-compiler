mod lexer;
mod token;

use lexer::Lexer;
use token::Token;
use std::{env, fs};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: cargo run -- <source_file>");
        return;
    }

    let file_path = &args[1];

    let source = fs::read_to_string(file_path)
        .expect("Failed to read source file");

    let mut lexer = Lexer::new(&source);

    loop {
        let token = lexer.next_token();
        println!("{:?}", token);

        if token == Token::Eof {
            break;
        }
    }
}
