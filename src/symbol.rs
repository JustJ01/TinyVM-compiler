use std::collections::HashMap;

use crate::ast::{Expr, Stmt};

#[derive(Debug)]
pub struct SymbolTable {
    symbols: HashMap<String, usize>,
    next_slot: usize,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            next_slot: 0,
        }
    }

    pub fn declare(&mut self, name: &str) -> usize {
        if let Some(&slot) = self.symbols.get(name) {
            slot
        } else {
            let slot = self.next_slot;
            self.symbols.insert(name.to_string(), slot);
            self.next_slot += 1;
            slot
        }
    }

    pub fn lookup(&self, name: &str) -> usize {
        *self
            .symbols
            .get(name)
            .unwrap_or_else(|| panic!("Semantic error: variable '{}' used before assignment", name))
    }

    pub fn all(&self) -> &HashMap<String, usize> {
        &self.symbols
    }
}

pub fn build_symbol_table(program: &[Stmt]) -> SymbolTable {
    let mut table = SymbolTable::new();

    for stmt in program {
        visit_stmt(stmt, &mut table);
    }

    table
}

fn visit_stmt(stmt: &Stmt, table: &mut SymbolTable) {
    match stmt {
        Stmt::Assign { name, value } => {
            table.declare(name);
            visit_expr(value, table);
        }

        Stmt::Print(expr) => {
            visit_expr(expr, table);
        }

        Stmt::While { condition, body } => {
            visit_expr(condition, table);
            for stmt in body {
                visit_stmt(stmt, table);
            }
        }

        Stmt::If {
            condition,
            then_body,
            else_body,
        } => {
            visit_expr(condition, table);

            for stmt in then_body {
                visit_stmt(stmt, table);
            }

            for stmt in else_body {
                visit_stmt(stmt, table);
            }
        }

        Stmt::Func { params, body, .. } => {
            // declare parameters as variables
            for p in params {
                table.declare(p);
            }

            for stmt in body {
                visit_stmt(stmt, table);
            }
        }

        Stmt::Return(expr) => {
            visit_expr(expr, table);
        }
    }
}

fn visit_expr(expr: &Expr, table: &mut SymbolTable) {
    match expr {
        Expr::Int(_) => {}

        Expr::Var(name) => {
            table.lookup(name);
        }

        Expr::Binary { left, right, .. } => {
            visit_expr(left, table);
            visit_expr(right, table);
        }

        Expr::Call { args, .. } => {
            for arg in args {
                visit_expr(arg, table);
            }
        }

        Expr::NativeCall { arg, .. } => {
            visit_expr(arg, table);
        }
    }
}
