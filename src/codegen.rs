use crate::{
    ast::{BinOp, Expr, Stmt, UnaryOp},
    symbol::SymbolTable,
    typecheck::{infer_expression_type, Type},
};
use std::collections::HashMap;

pub struct CodeGen {
    pub code: Vec<u8>,
    pub functions: std::collections::HashMap<String, usize>,
    pub pending_calls: Vec<(usize, String)>,
    globals: HashMap<String, Type>,
    
    // Loop context for break/continue
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    start: usize,
    breaks: Vec<usize>,  // positions to patch to loop end
}

impl CodeGen {
    pub fn new(globals: HashMap<String, Type>) -> Self {
        Self {
            code: Vec::new(),
            functions: std::collections::HashMap::new(),
            pending_calls: Vec::new(),
            globals,
            loop_stack: Vec::new(),
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

            Stmt::ArrAssign { name, index, value } => {
                let slot = symbols.lookup(name);
                self.emit(0x20); // LOAD handle
                self.emit_u8(slot);
                self.gen_expr(index, symbols);
                self.gen_expr(value, symbols);
                self.emit(0x23); // ARRSTORE
            }

            Stmt::Print(expr) => {
                if infer_expression_type(expr, &self.globals) == Type::Str {
                    if let Expr::Str(s) = expr {
                        for &ch in s.as_bytes().iter().rev() {
                            self.emit(0x01);
                            self.emit_u8(ch as usize);
                        }
                        self.emit(0x08); // ARRLIT
                        self.emit_u8(s.len());
                    } else {
                        self.gen_expr(expr, symbols);
                    }
                    self.emit(0x52); // PRINT_STR
                } else {
                    self.gen_expr(expr, symbols);
                    self.emit(0x51);
                }
            }

            Stmt::While { condition, body } => {
                let loop_start = self.code.len();

                // Push loop context
                self.loop_stack.push(LoopContext {
                    start: loop_start,
                    breaks: Vec::new(),
                });

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

                // Pop loop context and patch all breaks
                if let Some(ctx) = self.loop_stack.pop() {
                    for break_pos in ctx.breaks {
                        self.patch_u8(break_pos, loop_end);
                    }
                }
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

            Stmt::Native { id, args } => {
                for a in args {
                    self.gen_expr(a, symbols);
                }
                if *id == 30 {
                    self.emit(0x07); // ARRLEN (string/array length)
                } else {
                    self.emit(0x50); // CALL_NATIVE
                    self.emit_u8(args.len());
                    self.emit_u8(*id as usize);
                }
            }

            Stmt::Break => {
                // Get loop start before mutable borrow
                let loop_end_placeholder = if let Some(ctx) = self.loop_stack.last() {
                    Some(ctx.start)
                } else {
                    None
                };

                if loop_end_placeholder.is_some() {
                    self.emit(0x40); // JMP
                    let pos = self.code.len();
                    self.emit(0); // placeholder
                    
                    // Now we can mutably borrow
                    if let Some(ctx) = self.loop_stack.last_mut() {
                        ctx.breaks.push(pos);
                    }
                } else {
                    panic!("break statement outside of loop");
                }
            }

            Stmt::Continue => {
                let loop_start = if let Some(ctx) = self.loop_stack.last() {
                    Some(ctx.start)
                } else {
                    None
                };

                if let Some(start) = loop_start {
                    self.emit(0x40); // JMP
                    self.emit_u8(start); // jump to loop start
                } else {
                    panic!("continue statement outside of loop");
                }
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

            Expr::Unary { op, operand } => {
                self.gen_expr(operand, symbols);
                match op {
                    UnaryOp::Not => {
                        // not x -> push 0, eq (x == 0)
                        self.emit(0x01);
                        self.emit(0);
                        self.emit(0x30);
                    }
                    UnaryOp::Neg => {
                        // -x -> push 0, sub (0 - x)
                        self.emit(0x01);
                        self.emit(0);
                        self.emit(0x03);
                    }
                }
            }

            Expr::Binary { left, op, right } => {
                let left_type = infer_expression_type(left, &self.globals);
                let right_type = infer_expression_type(right, &self.globals);
                let is_str = left_type == Type::Str || right_type == Type::Str;

                self.gen_expr(left, symbols);
                self.gen_expr(right, symbols);

                match op {
                    BinOp::Add => {
                        if is_str {
                            self.emit(0x55); // STRCONCAT
                        } else {
                            self.emit(0x02);
                        }
                    }
                    BinOp::Sub => self.emit(0x03),
                    BinOp::Mul => self.emit(0x04),
                    BinOp::Div => self.emit(0x05),
                    BinOp::Lt => {
                        if is_str { self.emit(0x53); self.emit(0x01); self.emit(0); self.emit(0x31); }
                        else { self.emit(0x31); }
                    }
                    BinOp::Gt => {
                        if is_str { self.emit(0x53); self.emit(0x01); self.emit(0); self.emit(0x32); }
                        else { self.emit(0x32); }
                    }
                    BinOp::EqEq => {
                        if is_str { self.emit(0x53); self.emit(0x01); self.emit(0); self.emit(0x30); }
                        else { self.emit(0x30); }
                    }
                    BinOp::Le => {
                        if is_str { self.emit(0x53); self.emit(0x01); self.emit(0); self.emit(0x34); }
                        else { self.emit(0x34); }
                    }
                    BinOp::Ge => {
                        if is_str { self.emit(0x53); self.emit(0x01); self.emit(0); self.emit(0x35); }
                        else { self.emit(0x35); }
                    }
                    BinOp::And => {
                        // short-circuit: left AND right
                        // if left is false, jump to 0
                        self.emit(0x41);
                        let jmp = self.code.len();
                        self.emit(0);
                        self.gen_expr(right, symbols);
                        self.patch_u8(jmp, self.code.len());
                    }
                    BinOp::Or => {
                        // short-circuit: left OR right
                        // if left is true, keep 1
                        self.emit(0x01);
                        self.emit(1);
                        self.emit(0x30); // eq -> left == 1
                        self.emit(0x41);
                        let jmp = self.code.len();
                        self.emit(0);
                        self.gen_expr(right, symbols);
                        self.patch_u8(jmp, self.code.len());
                    }
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

            Expr::NativeCall { id, args } => {
                for a in args {
                    self.gen_expr(a, symbols);
                }
                if *id == 30 {
                    self.emit(0x07); // ARRLEN (string/array length)
                } else {
                    self.emit(0x50); // CALL_NATIVE
                    self.emit_u8(args.len());
                    self.emit_u8(*id as usize);
                }
            }

            Expr::Str(s) => {
                for &ch in s.as_bytes().iter().rev() {
                    self.emit(0x01);
                    self.emit_u8(ch as usize);
                }
                self.emit(0x08); // ARRLIT
                self.emit_u8(s.len());
            }

            Expr::ArrLit(elements) => {
                // Push elements in reverse order so ArrLit stores them correctly
                for elem in elements.iter().rev() {
                    self.gen_expr(elem, symbols);
                }
                self.emit(0x08); // ARRLIT
                self.emit_u8(elements.len());
            }

            Expr::ArrIndex { arr, index } => {
                self.gen_expr(arr, symbols);
                self.gen_expr(index, symbols);
                self.emit(0x22); // ARRLOAD
            }
        }
    }
}
