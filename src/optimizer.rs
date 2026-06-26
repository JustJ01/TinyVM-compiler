use crate::ast::{BinOp, Expr, Stmt, UnaryOp};

/// AST optimization pass - constant folding, dead code elimination
pub struct AstOptimizer;

impl AstOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// Optimize a program (list of statements)
    pub fn optimize_program(&self, program: &[Stmt]) -> Vec<Stmt> {
        program.iter().map(|stmt| self.optimize_stmt(stmt)).collect()
    }

    /// Optimize a single statement
    fn optimize_stmt(&self, stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::Print(expr) => Stmt::Print(self.optimize_expr(expr)),

            Stmt::Assign { name, value } => Stmt::Assign {
                name: name.clone(),
                value: self.optimize_expr(value),
            },

            Stmt::ArrAssign { name, index, value } => Stmt::ArrAssign {
                name: name.clone(),
                index: Box::new(self.optimize_expr(index)),
                value: self.optimize_expr(value),
            },

            Stmt::While { condition, body } => {
                let opt_condition = self.optimize_expr(condition);
                
                // Dead code elimination: while false { ... } can be removed
                if let Expr::Int(0) = opt_condition {
                    // Return empty block - caller should filter this out
                    return Stmt::While {
                        condition: opt_condition,
                        body: vec![],
                    };
                }

                Stmt::While {
                    condition: opt_condition,
                    body: self.optimize_body(body),
                }
            }

            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let opt_condition = self.optimize_expr(condition);

                // Constant condition optimization
                match &opt_condition {
                    Expr::Int(0) => {
                        // Condition is false - only keep else branch
                        if else_body.is_empty() {
                            return Stmt::If {
                                condition: Expr::Int(0),
                                then_body: vec![],
                                else_body: vec![],
                            };
                        }
                        // Flatten else body
                        if else_body.len() == 1 {
                            return self.optimize_stmt(&else_body[0]);
                        }
                        return Stmt::If {
                            condition: Expr::Int(0),
                            then_body: vec![],
                            else_body: self.optimize_body(else_body),
                        };
                    }
                    Expr::Int(_) => {
                        // Condition is true - only keep then branch
                        if then_body.len() == 1 {
                            return self.optimize_stmt(&then_body[0]);
                        }
                        return Stmt::If {
                            condition: opt_condition,
                            then_body: self.optimize_body(then_body),
                            else_body: vec![],
                        };
                    }
                    _ => {}
                }

                Stmt::If {
                    condition: opt_condition,
                    then_body: self.optimize_body(then_body),
                    else_body: self.optimize_body(else_body),
                }
            }

            Stmt::Func { name, params, body } => Stmt::Func {
                name: name.clone(),
                params: params.clone(),
                body: self.optimize_body_with_dead_code_after_return(body),
            },

            Stmt::Return(expr) => Stmt::Return(self.optimize_expr(expr)),

            Stmt::Native { id, args } => Stmt::Native {
                id: *id,
                args: args.iter().map(|a| self.optimize_expr(a)).collect(),
            },

            Stmt::Break => Stmt::Break,
            Stmt::Continue => Stmt::Continue,
        }
    }

    /// Optimize function body and remove dead code after return
    fn optimize_body_with_dead_code_after_return(&self, body: &[Stmt]) -> Vec<Stmt> {
        let mut result = Vec::new();
        
        for stmt in body {
            let optimized = self.optimize_stmt(stmt);
            result.push(optimized);
            
            // Dead code elimination: stop after return
            if matches!(stmt, Stmt::Return(_) | Stmt::Break | Stmt::Continue) {
                break;
            }
        }
        
        result
    }

    /// Optimize a body of statements
    fn optimize_body(&self, body: &[Stmt]) -> Vec<Stmt> {
        body.iter().map(|stmt| self.optimize_stmt(stmt)).collect()
    }

    /// Optimize an expression - perform constant folding
    fn optimize_expr(&self, expr: &Expr) -> Expr {
        match expr {
            Expr::Binary { left, op, right } => {
                let opt_left = self.optimize_expr(left);
                let opt_right = self.optimize_expr(right);

                // Constant folding
                if let (Expr::Int(l), Expr::Int(r)) = (&opt_left, &opt_right) {
                    return match op {
                        BinOp::Add => Expr::Int(l + r),
                        BinOp::Sub => Expr::Int(l - r),
                        BinOp::Mul => Expr::Int(l * r),
                        BinOp::Div => {
                            if *r != 0 {
                                Expr::Int(l / r)
                            } else {
                                // Division by zero - keep as is, will fail at runtime
                                Expr::Binary {
                                    left: Box::new(opt_left),
                                    op: op.clone(),
                                    right: Box::new(opt_right),
                                }
                            }
                        }
                        BinOp::Lt => Expr::Int(if l < r { 1 } else { 0 }),
                        BinOp::Gt => Expr::Int(if l > r { 1 } else { 0 }),
                        BinOp::EqEq => Expr::Int(if l == r { 1 } else { 0 }),
                        BinOp::Le => Expr::Int(if l <= r { 1 } else { 0 }),
                        BinOp::Ge => Expr::Int(if l >= r { 1 } else { 0 }),
                        BinOp::And => Expr::Int(if *l != 0 && *r != 0 { 1 } else { 0 }),
                        BinOp::Or => Expr::Int(if *l != 0 || *r != 0 { 1 } else { 0 }),
                    };
                }

                // Algebraic simplifications
                match (&opt_left, op, &opt_right) {
                    // x + 0 = x
                    (_, BinOp::Add, Expr::Int(0)) => opt_left,
                    (Expr::Int(0), BinOp::Add, _) => opt_right,
                    
                    // x - 0 = x
                    (_, BinOp::Sub, Expr::Int(0)) => opt_left,
                    
                    // x * 0 = 0
                    (_, BinOp::Mul, Expr::Int(0)) => Expr::Int(0),
                    (Expr::Int(0), BinOp::Mul, _) => Expr::Int(0),
                    
                    // x * 1 = x
                    (_, BinOp::Mul, Expr::Int(1)) => opt_left,
                    (Expr::Int(1), BinOp::Mul, _) => opt_right,
                    
                    // x / 1 = x
                    (_, BinOp::Div, Expr::Int(1)) => opt_left,

                    _ => Expr::Binary {
                        left: Box::new(opt_left),
                        op: op.clone(),
                        right: Box::new(opt_right),
                    },
                }
            }

            Expr::Call { name, args } => Expr::Call {
                name: name.clone(),
                args: args.iter().map(|arg| self.optimize_expr(arg)).collect(),
            },

            Expr::NativeCall { id, args } => Expr::NativeCall {
                id: *id,
                args: args.iter().map(|a| self.optimize_expr(a)).collect(),
            },

            Expr::ArrLit(elements) => Expr::ArrLit(
                elements.iter().map(|e| self.optimize_expr(e)).collect(),
            ),
            Expr::ArrIndex { arr, index } => Expr::ArrIndex {
                arr: Box::new(self.optimize_expr(arr)),
                index: Box::new(self.optimize_expr(index)),
            },

            Expr::Str(_) => expr.clone(),

            // Literals and variables don't need optimization
            _ => expr.clone(),
        }
    }
}
