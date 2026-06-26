use crate::ast::{BinOp, Expr, Stmt, UnaryOp};
use crate::typecheck::{infer_expression_type, Type};
use std::collections::{HashMap, HashSet};

pub type Temp = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TempExpr {
    Const(i32),
    Var(String),
    BinOp { op: BinOp, left: Temp, right: Temp },
    UnaryOp { op: UnaryOp, operand: Temp },
    Call { name: String, args: Vec<Temp> },
    NativeCall { id: u8, args: Vec<Temp> },
    Str(String),
    StrConcat { left: Temp, right: Temp },
    StrCmp { op: BinOp, left: Temp, right: Temp },
    ArrLit { elements: Vec<Temp> },
    ArrIndex { arr: Temp, index: Temp },
}

#[derive(Debug, Clone)]
pub enum IrStmt {
    Compute { dest: Temp, expr: TempExpr },
    Assign { name: String, value: Temp },
    ArrAssign { name: String, index: Temp, value: Temp },
    Print(Temp),
    While { condition: Temp, body: Vec<IrStmt> },
    If { condition: Temp, then_body: Vec<IrStmt>, else_body: Vec<IrStmt> },
    Func { name: String, params: Vec<String>, body: Vec<IrStmt> },
    Return(Temp),
    Native { id: u8, args: Vec<Temp> },
    Break,
    Continue,
}

#[derive(Debug)]
pub struct IrProgram {
    pub stmts: Vec<IrStmt>,
}

pub struct IrBuilder {
    next_temp: Temp,
    pub temps: Vec<TempExpr>,
    expr_cache: HashMap<TempExpr, Temp>,
    emitted_computes: HashSet<Temp>,
    globals: HashMap<String, Type>,
}

impl IrBuilder {
    pub fn new(globals: HashMap<String, Type>) -> Self {
        Self {
            next_temp: 0,
            temps: Vec::new(),
            expr_cache: HashMap::new(),
            emitted_computes: HashSet::new(),
            globals,
        }
    }

    fn alloc_temp(&mut self, expr: TempExpr) -> Temp {
        let id = self.next_temp;
        self.next_temp += 1;
        self.temps.push(expr);
        id
    }

    fn alloc_temp_cse(&mut self, expr: TempExpr) -> Temp {
        if let Some(&existing) = self.expr_cache.get(&expr) {
            return existing;
        }
        let id = self.alloc_temp(expr.clone());
        self.expr_cache.insert(expr, id);
        id
    }

    /// Emit a Compute statement for a temp if it hasn't been emitted yet.
    /// Only emits for non-trivial expressions (not Const/Var).
    fn emit_compute(&mut self, temp: Temp, computes: &mut Vec<IrStmt>) {
        if self.emitted_computes.contains(&temp) {
            return;
        }
        let expr = &self.temps[temp];
        match expr {
            TempExpr::Const(_) | TempExpr::Var(_) | TempExpr::Str(_) => return,
            _ => {}
        }
        computes.push(IrStmt::Compute {
            dest: temp,
            expr: expr.clone(),
        });
        self.emitted_computes.insert(temp);
    }

    fn expr_to_ir(&mut self, expr: &Expr, computes: &mut Vec<IrStmt>) -> Temp {
        match expr {
            Expr::Int(n) => self.alloc_temp_cse(TempExpr::Const(*n)),
            Expr::Var(name) => self.alloc_temp_cse(TempExpr::Var(name.clone())),
            Expr::Binary { left, op, right } => {
                let l = self.expr_to_ir(left, computes);
                let r = self.expr_to_ir(right, computes);
                let left_type = infer_expression_type(left, &self.globals);
                let right_type = infer_expression_type(right, &self.globals);
                let is_str = left_type == Type::Str || right_type == Type::Str;
                if is_str {
                    let raw = match op {
                        BinOp::Add => TempExpr::StrConcat { left: l, right: r },
                        _ => TempExpr::StrCmp { op: op.clone(), left: l, right: r },
                    };
                    let t = self.alloc_temp_cse(raw);
                    self.emit_compute(t, computes);
                    t
                } else {
                    let raw = TempExpr::BinOp {
                        op: op.clone(),
                        left: l,
                        right: r,
                    };
                    let reduced = Self::apply_strength_reduction(&raw, &self.temps);
                    let t = self.alloc_temp_cse(reduced);
                    self.emit_compute(t, computes);
                    t
                }
            }
            Expr::Unary { op, operand } => {
                let opd = self.expr_to_ir(operand, computes);
                let expr = TempExpr::UnaryOp {
                    op: op.clone(),
                    operand: opd,
                };
                let t = self.alloc_temp_cse(expr);
                self.emit_compute(t, computes);
                t
            }
            Expr::Call { name, args } => {
                let ir_args: Vec<Temp> = args.iter().map(|a| self.expr_to_ir(a, computes)).collect();
                let expr = TempExpr::Call {
                    name: name.clone(),
                    args: ir_args,
                };
                let t = self.alloc_temp_cse(expr);
                self.emit_compute(t, computes);
                t
            }
            Expr::NativeCall { id, args } => {
                let ir_args: Vec<Temp> = args.iter().map(|a| self.expr_to_ir(a, computes)).collect();
                let expr = TempExpr::NativeCall { id: *id, args: ir_args };
                let t = self.alloc_temp_cse(expr);
                self.emit_compute(t, computes);
                t
            }
            Expr::Str(s) => self.alloc_temp_cse(TempExpr::Str(s.clone())),
            Expr::ArrLit(elements) => {
                let el_temps: Vec<Temp> = elements.iter().map(|e| self.expr_to_ir(e, computes)).collect();
                let expr = TempExpr::ArrLit { elements: el_temps };
                let t = self.alloc_temp_cse(expr);
                self.emit_compute(t, computes);
                t
            }
            Expr::ArrIndex { arr, index } => {
                let arr_temp = self.expr_to_ir(arr, computes);
                let idx_temp = self.expr_to_ir(index, computes);
                let expr = TempExpr::ArrIndex { arr: arr_temp, index: idx_temp };
                let t = self.alloc_temp_cse(expr);
                self.emit_compute(t, computes);
                t
            }
        }
    }

    /// Strength Reduction: replace expensive operations with cheaper ones.
    /// x * 2 -> x + x
    fn apply_strength_reduction(expr: &TempExpr, temps: &[TempExpr]) -> TempExpr {
        match expr {
            TempExpr::BinOp {
                op: BinOp::Mul,
                left,
                right,
            } => {
                let is_left_two = matches!(temps.get(*left), Some(TempExpr::Const(2)));
                let is_right_two = matches!(temps.get(*right), Some(TempExpr::Const(2)));
                if is_left_two || is_right_two {
                    let inner = if is_left_two { *right } else { *left };
                    return TempExpr::BinOp {
                        op: BinOp::Add,
                        left: inner,
                        right: inner,
                    };
                }
                expr.clone()
            }
            _ => expr.clone(),
        }
    }

    pub fn stmt_to_ir(&mut self, stmt: &Stmt) -> Vec<IrStmt> {
        let mut result = Vec::new();
        match stmt {
            Stmt::Assign { name, value } => {
                let val_temp = self.expr_to_ir(value, &mut result);
                result.push(IrStmt::Assign {
                    name: name.clone(),
                    value: val_temp,
                });
            }
            Stmt::ArrAssign { name, index, value } => {
                let idx_temp = self.expr_to_ir(index, &mut result);
                let val_temp = self.expr_to_ir(value, &mut result);
                result.push(IrStmt::ArrAssign {
                    name: name.clone(),
                    index: idx_temp,
                    value: val_temp,
                });
            }
            Stmt::Print(expr) => {
                let t = self.expr_to_ir(expr, &mut result);
                result.push(IrStmt::Print(t));
            }
            Stmt::While { condition, body } => {
                // Emit condition compute before the loop (initial check)
                // and at the end of the body (re-check after each iteration)
                let mut cond_ir = Vec::new();
                let cond_temp = self.expr_to_ir(condition, &mut cond_ir);
                let mut body_ir = Vec::new();
                for s in body {
                    body_ir.extend(self.stmt_to_ir(s));
                }
                body_ir.extend(cond_ir.clone());
                result.extend(cond_ir);
                result.push(IrStmt::While {
                    condition: cond_temp,
                    body: body_ir,
                });
                self.expr_cache.clear();
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut cond_computes = Vec::new();
                let cond_temp = self.expr_to_ir(condition, &mut cond_computes);
                let mut then_ir = Vec::new();
                let mut else_ir = Vec::new();
                for s in then_body {
                    then_ir.extend(self.stmt_to_ir(s));
                }
                for s in else_body {
                    else_ir.extend(self.stmt_to_ir(s));
                }
                result.extend(cond_computes);
                result.push(IrStmt::If {
                    condition: cond_temp,
                    then_body: then_ir,
                    else_body: else_ir,
                });
            }
            Stmt::Func { name, params, body } => {
                let mut body_ir = Vec::new();
                for s in body {
                    body_ir.extend(self.stmt_to_ir(s));
                }
                result.push(IrStmt::Func {
                    name: name.clone(),
                    params: params.clone(),
                    body: body_ir,
                });
            }
            Stmt::Return(expr) => {
                let t = self.expr_to_ir(expr, &mut result);
                result.push(IrStmt::Return(t));
            }
            Stmt::Native { id, args } => {
                let ir_args: Vec<Temp> = args.iter().map(|a| self.expr_to_ir(a, &mut result)).collect();
                result.push(IrStmt::Native { id: *id, args: ir_args });
            }
            Stmt::Break => result.push(IrStmt::Break),
            Stmt::Continue => result.push(IrStmt::Continue),
        }
        result
    }

    pub fn build_program(&mut self, program: &[Stmt]) -> IrProgram {
        let stmts: Vec<IrStmt> = program.iter().flat_map(|s| self.stmt_to_ir(s)).collect();
        IrProgram { stmts }
    }

    pub fn get_expr(&self, temp: Temp) -> &TempExpr {
        &self.temps[temp]
    }

    /// Collect all temps referenced by IR statements (as values, not destinations)
    fn collect_used_temps(stmts: &[IrStmt]) -> HashSet<Temp> {
        let mut result = HashSet::new();
        for stmt in stmts {
            match stmt {
                IrStmt::Compute { expr, .. } => {
                    result.extend(Self::expr_deps(expr));
                }
                IrStmt::Assign { value, .. } => { result.insert(*value); }
                IrStmt::ArrAssign { index, value, .. } => { result.insert(*index); result.insert(*value); }
                IrStmt::Print(t) => { result.insert(*t); }
                IrStmt::While { condition, body } => {
                    result.insert(*condition);
                    result.extend(Self::collect_used_temps(body));
                }
                IrStmt::If { condition, then_body, else_body } => {
                    result.insert(*condition);
                    result.extend(Self::collect_used_temps(then_body));
                    result.extend(Self::collect_used_temps(else_body));
                }
                IrStmt::Func { body, .. } => {
                    result.extend(Self::collect_used_temps(body));
                }
                IrStmt::Return(t) => { result.insert(*t); }
                IrStmt::Native { args, .. } => { for a in args { result.insert(*a); } }
                _ => {}
            }
        }
        result
    }

    fn expr_deps(expr: &TempExpr) -> Vec<Temp> {
        match expr {
            TempExpr::Str(_) | TempExpr::Const(_) | TempExpr::Var(_) => vec![],
            TempExpr::BinOp { left, right, .. } => vec![*left, *right],
            TempExpr::UnaryOp { operand, .. } => vec![*operand],
            TempExpr::Call { args, .. } => args.clone(),
            TempExpr::NativeCall { args, .. } => args.clone(),
            TempExpr::StrConcat { left, right } => vec![*left, *right],
            TempExpr::StrCmp { left, right, .. } => vec![*left, *right],
            TempExpr::ArrLit { elements } => elements.clone(),
            TempExpr::ArrIndex { arr, index } => vec![*arr, *index],
        }
    }

    /// Convert IR back to AST, preserving compute statements inline
    pub fn ir_to_ast(&self, ir: &IrProgram) -> Vec<Stmt> {
        Self::ir_stmts_to_ast(&ir.stmts, &self.temps)
    }

    fn compute_to_stmt(dest: &Temp, expr: &TempExpr, temps: &[TempExpr]) -> Stmt {
        let rhs = Self::temp_expr_to_ast(expr, temps);
        Stmt::Assign {
            name: format!("__tmp{}", dest),
            value: rhs,
        }
    }

    fn ir_stmts_to_ast(stmts: &[IrStmt], temps: &[TempExpr]) -> Vec<Stmt> {
        let mut result = Vec::new();
        for stmt in stmts {
            match stmt {
                IrStmt::Compute { dest, expr } => {
                    result.push(Self::compute_to_stmt(dest, expr, temps));
                }
                IrStmt::Assign { name, value } => {
                    result.push(Stmt::Assign {
                        name: name.clone(),
                        value: Self::temp_to_expr(*value, temps),
                    });
                }
                IrStmt::ArrAssign { name, index, value } => {
                    result.push(Stmt::ArrAssign {
                        name: name.clone(),
                        index: Box::new(Self::temp_to_expr(*index, temps)),
                        value: Self::temp_to_expr(*value, temps),
                    });
                }
                IrStmt::Print(t) => {
                    result.push(Stmt::Print(Self::temp_to_expr(*t, temps)));
                }
                IrStmt::While { condition, body } => {
                    result.push(Stmt::While {
                        condition: Self::temp_to_expr(*condition, temps),
                        body: Self::ir_stmts_to_ast(body, temps),
                    });
                }
                IrStmt::If { condition, then_body, else_body } => {
                    result.push(Stmt::If {
                        condition: Self::temp_to_expr(*condition, temps),
                        then_body: Self::ir_stmts_to_ast(then_body, temps),
                        else_body: Self::ir_stmts_to_ast(else_body, temps),
                    });
                }
                IrStmt::Func { name, params, body } => {
                    result.push(Stmt::Func {
                        name: name.clone(),
                        params: params.clone(),
                        body: Self::ir_stmts_to_ast(body, temps),
                    });
                }
                IrStmt::Return(t) => {
                    result.push(Stmt::Return(Self::temp_to_expr(*t, temps)));
                }
                IrStmt::Native { id, args } => {
                    result.push(Stmt::Native {
                        id: *id,
                        args: args.iter().map(|a| Self::temp_to_expr(*a, temps)).collect(),
                    });
                }
                IrStmt::Break => result.push(Stmt::Break),
                IrStmt::Continue => result.push(Stmt::Continue),
            }
        }
        result
    }

    /// Convert a temp reference to an AST expression.
    /// Inlines constants and variables directly; references temps as __tmpN.
    fn temp_to_expr(temp: Temp, temps: &[TempExpr]) -> Expr {
        match &temps[temp] {
            TempExpr::Const(n) => Expr::Int(*n),
            TempExpr::Var(name) => Expr::Var(name.clone()),
            TempExpr::Str(s) => Expr::Str(s.clone()),
            _ => Expr::Var(format!("__tmp{}", temp)),
        }
    }

    fn temp_expr_to_ast(expr: &TempExpr, temps: &[TempExpr]) -> Expr {
        match expr {
            TempExpr::Const(n) => Expr::Int(*n),
            TempExpr::Var(name) => Expr::Var(name.clone()),
            TempExpr::Str(s) => Expr::Str(s.clone()),
            TempExpr::BinOp { op, left, right } => Expr::Binary {
                left: Box::new(Self::temp_to_expr(*left, temps)),
                op: op.clone(),
                right: Box::new(Self::temp_to_expr(*right, temps)),
            },
            TempExpr::UnaryOp { op, operand } => Expr::Unary {
                op: op.clone(),
                operand: Box::new(Self::temp_to_expr(*operand, temps)),
            },
            TempExpr::Call { name, args } => Expr::Call {
                name: name.clone(),
                args: args.iter().map(|a| Self::temp_to_expr(*a, temps)).collect(),
            },
            TempExpr::NativeCall { id, args } => Expr::NativeCall {
                id: *id,
                args: args.iter().map(|a| Self::temp_to_expr(*a, temps)).collect(),
            },
            TempExpr::StrConcat { left, right } => Expr::Binary {
                left: Box::new(Self::temp_to_expr(*left, temps)),
                op: BinOp::Add,
                right: Box::new(Self::temp_to_expr(*right, temps)),
            },
            TempExpr::StrCmp { op, left, right } => Expr::Binary {
                left: Box::new(Self::temp_to_expr(*left, temps)),
                op: op.clone(),
                right: Box::new(Self::temp_to_expr(*right, temps)),
            },
            TempExpr::ArrLit { elements } => Expr::ArrLit(
                elements.iter().map(|e| Self::temp_to_expr(*e, temps)).collect(),
            ),
            TempExpr::ArrIndex { arr, index } => Expr::ArrIndex {
                arr: Box::new(Self::temp_to_expr(*arr, temps)),
                index: Box::new(Self::temp_to_expr(*index, temps)),
            },
        }
    }

    pub fn reset_emitted_computes(&mut self) {
        self.emitted_computes.clear();
        self.expr_cache.clear();
    }
}

/// Loop Invariant Code Motion: move invariant computations out of loop bodies.
pub fn licm_optimize(program: &mut IrProgram, builder: &IrBuilder) {
    program.stmts = optimize_stmts(&program.stmts, builder);
}

fn optimize_stmts(stmts: &[IrStmt], builder: &IrBuilder) -> Vec<IrStmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        match stmt {
            IrStmt::While { condition, body } => {
                let invariant = compute_invariant_temps(&result, body, builder);
                let (pre_loop, new_body) = extract_invariant(body, &invariant, builder);
                result.extend(pre_loop);
                result.push(IrStmt::While {
                    condition: *condition,
                    body: new_body,
                });
            }
            _ => {
                result.push(stmt.clone());
            }
        }
    }
    result
}

fn compute_invariant_temps(pre_loop: &[IrStmt], body: &[IrStmt], builder: &IrBuilder) -> HashSet<Temp> {
    // Find variables that are modified inside the loop body
    let mut modified_vars: HashSet<String> = HashSet::new();
    for stmt in body {
        match stmt {
            IrStmt::Assign { name, .. } => { modified_vars.insert(name.clone()); }
            IrStmt::ArrAssign { name, .. } => { modified_vars.insert(name.clone()); }
            _ => {}
        }
    }

    let mut invariant: HashSet<Temp> = HashSet::new();

    for stmt in pre_loop {
        match stmt {
            IrStmt::Compute { dest, .. } => { invariant.insert(*dest); }
            IrStmt::Assign { value, .. } => { invariant.insert(*value); }
            _ => {}
        }
    }

    for (id, expr) in builder.temps.iter().enumerate() {
        match expr {
            TempExpr::Const(_) | TempExpr::Str(_) => {
                invariant.insert(id);
            }
            TempExpr::Var(name) => {
                if !modified_vars.contains(name) {
                    invariant.insert(id);
                }
            }
            _ => {}
        }
    }

    invariant
}

fn extract_invariant(
    body: &[IrStmt],
    invariant: &HashSet<Temp>,
    builder: &IrBuilder,
) -> (Vec<IrStmt>, Vec<IrStmt>) {
    let mut pre_loop = Vec::new();
    let mut new_body = Vec::new();

    for stmt in body {
        match stmt {
            IrStmt::Compute { dest, expr } => {
                if is_invariant(expr, invariant, builder) {
                    pre_loop.push(stmt.clone());
                } else {
                    new_body.push(stmt.clone());
                }
            }
            _ => {
                new_body.push(stmt.clone());
            }
        }
    }

    (pre_loop, new_body)
}

fn is_invariant(expr: &TempExpr, invariant: &HashSet<Temp>, _builder: &IrBuilder) -> bool {
    match expr {
            TempExpr::Const(_) => true,
            TempExpr::Str(_) => true,
            TempExpr::Var(_) => true,
        TempExpr::BinOp { left, right, .. } => invariant.contains(left) && invariant.contains(right),
        TempExpr::UnaryOp { operand, .. } => invariant.contains(operand),
        TempExpr::Call { args, .. } => args.iter().all(|a| invariant.contains(a)),
        TempExpr::NativeCall { args, .. } => args.iter().all(|a| invariant.contains(a)),
        TempExpr::StrConcat { left, right } => invariant.contains(left) && invariant.contains(right),
        TempExpr::StrCmp { left, right, .. } => invariant.contains(left) && invariant.contains(right),
        TempExpr::ArrLit { elements } => elements.iter().all(|e| invariant.contains(e)),
        TempExpr::ArrIndex { arr, index } => invariant.contains(arr) && invariant.contains(index),
    }
}
