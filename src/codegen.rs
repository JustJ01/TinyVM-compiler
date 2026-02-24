use crate::{
    ast::{BinOp, Expr, Stmt},
    symbol::SymbolTable,
};

pub struct CodeGen {
    pub code: Vec<u8>,
    pub functions: std::collections::HashMap<String, usize>,
    pub pending_calls: Vec<(usize, String)>,
}

impl CodeGen {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            functions: std::collections::HashMap::new(),
            pending_calls: Vec::new(),
        }
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
        // compile main first, skip functions
        for stmt in program {
            if !matches!(stmt, Stmt::Func { .. }) {
                self.gen_stmt(stmt, symbols);
            }
        }

        self.emit(0xFF); // HALT

        // compile functions after main
        for stmt in program {
            if let Stmt::Func { name, .. } = stmt {
                let addr = self.code.len();
                self.functions.insert(name.clone(), addr);
                self.gen_stmt(stmt, symbols);
            }
        }

        // patch CALL targets
        for (pos, name) in &self.pending_calls {
            let addr = self.functions[name];
            self.code[*pos] = addr as u8;
        }

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

            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                // 1️⃣ Evaluate condition
                self.gen_expr(condition, symbols);

                // 2️⃣ Jump to ELSE if false
                self.emit(0x41); // JMP_IF_FALSE
                let else_jump_pos = self.code.len();
                self.emit(0); // placeholder

                // 3️⃣ THEN block
                for stmt in then_body {
                    self.gen_stmt(stmt, symbols);
                }

                // 4️⃣ Jump to END of THIS IF
                self.emit(0x40); // JMP
                let end_jump_pos = self.code.len();
                self.emit(0); // placeholder

                // 5️⃣ Patch ELSE target (start of else_body)
                let else_start = self.code.len();
                self.patch_u8(else_jump_pos, else_start);

                // 6️⃣ Generate ELSE / ELSEIF body
                for stmt in else_body {
                    self.gen_stmt(stmt, symbols);
                }

                // 7️⃣ Patch END target (after entire IF)
                let end = self.code.len();
                self.patch_u8(end_jump_pos, end);
            }

            Stmt::Func {
                name: _,
                params,
                body,
            } => {
                // parameters already on stack — store into memory slots
                for (i, p) in params.iter().enumerate().rev() {
                    let slot = symbols.lookup(p);
                    self.emit(0x21);
                    self.emit_u8(slot);
                }

                for stmt in body {
                    self.gen_stmt(stmt, symbols);
                }

                // implicit return 0 if none
                self.emit(0x01);
                self.emit(0);
                self.emit(0x61); // RET
            }

            Stmt::Return(expr) => {
                self.gen_expr(expr, symbols);
                self.emit(0x61); // RET
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
                    BinOp::Le => self.emit(0x34),
                    BinOp::Ge => self.emit(0x35),
                }
            }

            Expr::Call { name, args } => {
                for arg in args {
                    self.gen_expr(arg, symbols);
                }

                self.emit(0x60); // CALL
                let pos = self.code.len();
                self.emit(0); // placeholder

                self.pending_calls.push((pos, name.clone()));
            }

            Expr::NativeCall { id, arg } => {
                self.gen_expr(arg, symbols);
                self.emit(0x50); // CALL_NATIVE
                self.emit_u8(*id as usize);
            }
        }
    }
}
