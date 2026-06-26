use crate::ast::{BinOp, Expr, Stmt, UnaryOp};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Array,
    Str,
    Unknown,
}

/// Check if an expression produces an array (sub-array) rather than a scalar,
/// given a set of variables known to hold nested arrays.
fn is_nested_array_expr(expr: &Expr, nested_vars: &HashSet<String>) -> bool {
    match expr {
        Expr::ArrLit(items) => items.iter().any(|item| matches!(item, Expr::ArrLit(_))),
        Expr::ArrIndex { .. } => false,
        Expr::NativeCall { .. } => false,
        Expr::Var(name) => nested_vars.contains(name),
        _ => false,
    }
}

/// Pre-scan to find variables that hold nested arrays (arrays of arrays).
fn collect_nested_array_vars(stmts: &[Stmt]) -> HashSet<String> {
    let mut nested = HashSet::new();
    loop {
        let mut changed = false;
        for stmt in stmts {
            if let Stmt::Assign { name, value } = stmt {
                if is_nested_array_expr(value, &nested) {
                    if nested.insert(name.clone()) {
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    nested
}

/// Public type inference helper for use by codegen and IR builder.
pub fn infer_expression_type(expr: &Expr, globals: &HashMap<String, Type>) -> Type {
    match expr {
        Expr::Int(_) => Type::Int,
        Expr::Str(_) => Type::Str,
        Expr::Var(name) => globals.get(name).cloned().unwrap_or(Type::Unknown),
        Expr::Binary { left, op, right } => {
            let l = infer_expression_type(left, globals);
            let r = infer_expression_type(right, globals);
            match op {
                BinOp::Add => {
                    if l == Type::Str || r == Type::Str { Type::Str } else { Type::Int }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div => Type::Int,
                BinOp::Lt | BinOp::Gt | BinOp::EqEq | BinOp::Le | BinOp::Ge => Type::Int,
                BinOp::And | BinOp::Or => Type::Int,
            }
        }
        Expr::Unary { .. } => Type::Int,
        Expr::Call { args, .. } => {
            for arg in args { infer_expression_type(arg, globals); }
            Type::Int
        }
        Expr::NativeCall { args, .. } => { for a in args { infer_expression_type(a, globals); } Type::Int }
        Expr::ArrLit(_) => Type::Array,
        Expr::ArrIndex { arr, index } => {
            let arr_type = infer_expression_type(arr, globals);
            infer_expression_type(index, globals);
            // Indexing an Array yields an element whose type could be Int or Array
            // (for nested arrays). Return Array to allow chained indexing.
            if arr_type == Type::Array { Type::Array } else { Type::Int }
        }
    }
}

pub struct TypeChecker {
    globals: HashMap<String, Type>,
    errors: Vec<String>,
    fn_signatures: HashMap<String, Vec<Type>>,
    fn_ret_types: HashMap<String, Type>,
    current_fn_ret_type: Option<Type>,
    nested_array_vars: HashSet<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
            errors: Vec::new(),
            fn_signatures: HashMap::new(),
            fn_ret_types: HashMap::new(),
            current_fn_ret_type: None,
            nested_array_vars: HashSet::new(),
        }
    }

    pub fn check_program(&mut self, stmts: &[Stmt]) -> bool {
        self.nested_array_vars = collect_nested_array_vars(stmts);
        self.check_stmts(stmts);
        if !self.errors.is_empty() {
            eprintln!("\nType checking failed with {} error(s):", self.errors.len());
            for err in &self.errors {
                eprintln!("  {}", err);
            }
        }
        self.errors.is_empty()
    }

    fn check_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign { name, value } => {
                let val_type = self.infer_expr(value);
                if let Some(expected) = self.globals.get(name) {
                    if *expected != Type::Unknown
                        && val_type != *expected
                        && val_type != Type::Unknown
                    {
                        self.error(&format!(
                            "variable '{}' used as {:?} but previously was {:?}",
                            name, val_type, expected
                        ));
                    }
                }
                if val_type != Type::Unknown {
                    let is_nested_arr = val_type == Type::Array
                        && is_nested_array_expr(value, &self.nested_array_vars);
                    if is_nested_arr {
                        self.nested_array_vars.insert(name.clone());
                    }
                    self.globals.insert(name.clone(), val_type);
                }
            }

            Stmt::ArrAssign { name, index, value } => {
                let idx_type = self.infer_expr(index);
                if idx_type != Type::Int && idx_type != Type::Unknown {
                    self.error(&format!("Array index must be Int, got {:?}", idx_type));
                }
                let val_type = self.infer_expr(value);
                if val_type != Type::Int && val_type != Type::Unknown {
                    self.error(&format!(
                        "Array element value must be Int, got {:?}",
                        val_type
                    ));
                }
                if let Some(var_type) = self.globals.get(name) {
                    if *var_type != Type::Array
                        && *var_type != Type::Str
                        && *var_type != Type::Unknown
                    {
                        self.error(&format!(
                            "Cannot index into {:?} variable '{}'",
                            var_type, name
                        ));
                    }
                }
            }

            Stmt::Print(expr) => {
                self.infer_expr(expr);
            }

            Stmt::While { condition, body } => {
                let cond_type = self.infer_expr(condition);
                if cond_type != Type::Int && cond_type != Type::Unknown {
                    self.error(&format!("While condition must be Int, got {:?}", cond_type));
                }
                self.check_stmts(body);
            }

            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond_type = self.infer_expr(condition);
                if cond_type != Type::Int && cond_type != Type::Unknown {
                    self.error(&format!("If condition must be Int, got {:?}", cond_type));
                }
                self.check_stmts(then_body);
                self.check_stmts(else_body);
            }

            Stmt::Func { name, params, body } => {
                let saved = self.globals.clone();
                let saved_ret = self.current_fn_ret_type.take();
                for param in params {
                    self.globals.insert(param.clone(), Type::Unknown);
                }
                self.check_stmts(body);
                let param_types: Vec<Type> = params
                    .iter()
                    .map(|p| self.globals.get(p).cloned().unwrap_or(Type::Unknown))
                    .collect();
                self.fn_signatures.insert(name.clone(), param_types);
                let ret_type = self.current_fn_ret_type.take().unwrap_or(Type::Int);
                self.fn_ret_types.insert(name.clone(), ret_type);
                self.globals = saved;
                self.current_fn_ret_type = saved_ret;
            }

            Stmt::Return(expr) => {
                let t = self.infer_expr(expr);
                if let Some(expected) = &self.current_fn_ret_type {
                    if *expected != Type::Unknown && t != Type::Unknown && t != *expected {
                        self.error(&format!(
                            "Return type mismatch: expected {:?}, got {:?}",
                            expected, t
                        ));
                    }
                } else {
                    self.current_fn_ret_type = Some(t);
                }
            }

            Stmt::Native { args, .. } => {
                for a in args { self.infer_expr(a); }
            }

            Stmt::Break | Stmt::Continue => {}
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            Expr::Int(_) => Type::Int,
            Expr::Str(_) => Type::Str,
            Expr::Var(name) => self.globals.get(name).cloned().unwrap_or(Type::Unknown),
            Expr::Binary { left, op, right } => {
                let l = self.infer_expr(left);
                let r = self.infer_expr(right);
                self.try_propagate_type(left, &r);
                self.try_propagate_type(right, &l);
                match op {
                    BinOp::Add => {
                        let both_str = l == Type::Str || r == Type::Str;
                        if both_str {
                            if l != Type::Unknown && r != Type::Unknown && l != r {
                                self.error(&format!("Cannot add {:?} and {:?}", l, r));
                            }
                            Type::Str
                        } else {
                            self.require_same("Arithmetic", &l, &r);
                            Type::Int
                        }
                    }
                    BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        if l == Type::Str || r == Type::Str {
                            self.error(&format!("{:?} not supported on strings", op));
                        } else {
                            self.require_same("Arithmetic", &l, &r);
                        }
                        Type::Int
                    }
                    BinOp::Lt | BinOp::Gt | BinOp::EqEq | BinOp::Le | BinOp::Ge => {
                        self.require_same("Comparison", &l, &r);
                        Type::Int
                    }
                    BinOp::And | BinOp::Or => {
                        if l != Type::Unknown && l != Type::Int {
                            self.error(&format!("Logical op requires Int, left is {:?}", l));
                        }
                        if r != Type::Unknown && r != Type::Int {
                            self.error(&format!("Logical op requires Int, right is {:?}", r));
                        }
                        Type::Int
                    }
                }
            }
            Expr::Unary { op, operand } => {
                let t = self.infer_expr(operand);
                match op {
                    UnaryOp::Not => {
                        if t != Type::Unknown && t != Type::Int {
                            self.error(&format!("'not' requires Int, got {:?}", t));
                        }
                        Type::Int
                    }
                    UnaryOp::Neg => {
                        if t != Type::Unknown && t != Type::Int {
                            self.error(&format!("'-' requires Int, got {:?}", t));
                        }
                        Type::Int
                    }
                }
            }
            Expr::Call { name, args } => {
                let arg_types: Vec<Type> = args.iter().map(|arg| self.infer_expr(arg)).collect();
                let expected_params: Option<Vec<Type>> = self.fn_signatures.get(name).cloned();
                if let Some(param_types) = expected_params {
                    if args.len() != param_types.len() {
                        self.error(&format!(
                            "Function '{}' expects {} arguments, got {}",
                            name,
                            param_types.len(),
                            args.len()
                        ));
                    } else {
                        for (i, (arg_type, expected)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                            if *expected != Type::Unknown && *arg_type != *expected && *arg_type != Type::Unknown {
                                self.error(&format!(
                                    "Argument {} to '{}' expected {:?}, got {:?}",
                                    i + 1,
                                    name,
                                    expected,
                                    arg_type
                                ));
                            }
                        }
                    }
                }
                self.fn_ret_types.get(name).cloned().unwrap_or(Type::Int)
            }
            Expr::NativeCall { args, .. } => {
                for a in args { self.infer_expr(a); }
                Type::Array
            }
            Expr::ArrLit(_) => Type::Array,
            Expr::ArrIndex { arr, index } => {
                let arr_type = self.infer_expr(arr);
                if arr_type != Type::Unknown
                    && arr_type != Type::Array
                    && arr_type != Type::Str
                {
                    self.error(&format!("Cannot index into {:?}", arr_type));
                }
                let idx_type = self.infer_expr(index);
                if idx_type != Type::Unknown && idx_type != Type::Int {
                    self.error(&format!("Array index must be Int, got {:?}", idx_type));
                }
                // Nested array: indexing into a known nested-array source
                // (e.g., a Var holding [[...],[...]]) yields a sub-array (Array).
                // Otherwise indexing yields a scalar (Int).
                if arr_type == Type::Array && is_nested_array_expr(arr, &self.nested_array_vars) {
                    Type::Array
                } else {
                    Type::Int
                }
            }
        }
    }

    fn try_propagate_type(&mut self, expr: &Expr, inferred: &Type) {
        if *inferred == Type::Unknown {
            return;
        }
        if let Expr::Var(name) = expr {
            if let Some(current) = self.globals.get(name) {
                if *current == Type::Unknown {
                    self.globals.insert(name.clone(), inferred.clone());
                }
            }
        }
    }

    fn require_same(&mut self, ctx: &str, l: &Type, r: &Type) {
        if *l != Type::Unknown && *r != Type::Unknown && l != r {
            self.error(&format!(
                "{} requires same types, got {:?} and {:?}",
                ctx, l, r
            ));
        }
    }

    fn error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn get_fn_ret_type(&self, name: &str) -> Option<Type> {
        self.fn_ret_types.get(name).cloned()
    }

    pub fn get_globals(&self) -> &HashMap<String, Type> {
        &self.globals
    }

    /// Get return type for a function, or None if unknown
    pub fn get_function_return_type(&self, name: &str) -> Option<Type> {
        self.get_fn_ret_type(name)
    }
}
