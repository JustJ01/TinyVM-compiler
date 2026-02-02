#[derive(Debug)]
pub enum Expr {
    Int(i32),
    Var(String),
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
}

#[derive(Debug)]
pub enum Stmt {
    Print(Expr),
    Assign { name: String, value: Expr },
    While { condition: Expr, body: Vec<Stmt> },
}

#[derive(Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Gt,
    EqEq,
}
