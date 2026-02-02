use crate::{
    ast::{BinOp, Expr, Stmt},
    symbol::SymbolTable,
};

pub struct CodeGen {
    pub code: Vec<u8>,
}

impl CodeGen {
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    fn emit(&mut self, byte: u8) {
        self.code.push(byte);
    }

    fn emit_u8(&mut self, value: usize) {
        self.code.push(value as u8);
    }

    fn patch_u8(&mut self, pos: usize, value: usize) {
        self.code[pos] = value as u8;
    }

    pub fn generate(mut self, program: &[Stmt], symbols: &SymbolTable) -> Vec<u8> {
        for stmt in program {
            self.gen_stmt(stmt, symbols);
        }

        self.emit(0xFF);
        self.code
    }

    fn gen_stmt(&mut self, stmt: &Stmt, symbols: &SymbolTable) {
        match stmt {
            Stmt::Assign { name, value } => {
                self.gen_expr(value, symbols);
                let slot = symbols.lookup(name);
                self.emit(0x21); 
                self.emit_u8(slot);
            }

            Stmt::Print(expr) => {
                self.gen_expr(expr, symbols);
                self.emit(0x51);
            }

            Stmt::While { condition, body } => {
                let loop_start = self.code.len();

                self.gen_expr(condition, symbols);
                self.emit(0x41); 
                let jmp_out_pos = self.code.len();
                self.emit(0); 

                for stmt in body {
                    self.gen_stmt(stmt, symbols);
                }

                self.emit(0x40); 
                self.emit_u8(loop_start);

                let loop_end = self.code.len();
                self.patch_u8(jmp_out_pos, loop_end);
            }
        }
    }

    fn gen_expr(&mut self, expr: &Expr, symbols: &SymbolTable) {
        match expr {
            Expr::Int(n) => {
                self.emit(0x01);
                self.emit_u8(*n as usize);
            }

            Expr::Var(name) => {
                let slot = symbols.lookup(name);
                self.emit(0x20); 
                self.emit_u8(slot);
            }

            Expr::Binary { left, op, right } => {
                self.gen_expr(left, symbols);
                self.gen_expr(right, symbols);

                match op {
                    BinOp::Add => self.emit(0x02),
                    BinOp::Sub => self.emit(0x03),
                    BinOp::Mul => self.emit(0x04),
                    BinOp::Div => self.emit(0x05),
                    BinOp::Lt => self.emit(0x31),
                    BinOp::Gt => self.emit(0x32),
                    BinOp::EqEq => self.emit(0x33),
                }
            }
        }
    }
}
