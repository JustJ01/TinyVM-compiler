#[derive(Debug)]
pub enum Expr {
    Int(i32),
    Var(String),
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    NativeCall {
        id: u8,
        arg: Box<Expr>,
    },
}

#[derive(Debug)]
pub enum Stmt {
    Print(Expr),
    Assign {
        name: String,
        value: Expr,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    Func {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },

    Return(Expr),
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
    Le,
    Ge,
}
