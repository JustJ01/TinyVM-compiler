#[derive(Debug, Clone)]
pub enum Expr {
    Int(i32),
    Var(String),
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    NativeCall {
        id: u8,
        args: Vec<Expr>,
    },
    Str(String),
    ArrLit(Vec<Expr>),
    ArrIndex {
        arr: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Print(Expr),
    Assign {
        name: String,
        value: Expr,
    },
    ArrAssign {
        name: String,
        index: Box<Expr>,
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
    Native {
        id: u8,
        args: Vec<Expr>,
    },
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Neg,
}
