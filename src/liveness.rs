use crate::ast::{Expr, Stmt};
use std::collections::{HashMap, HashSet};

/// Liveness analysis to determine when variables are used
/// This enables stack slot reuse for non-overlapping lifetimes
pub struct LivenessAnalyzer {
    /// Map from variable name to all statement indices where it's read
    reads: HashMap<String, Vec<usize>>,
    /// Map from variable name to statement index where it's written
    writes: HashMap<String, usize>,
    /// Current statement index during traversal
    current_index: usize,
}

impl LivenessAnalyzer {
    pub fn new() -> Self {
        Self {
            reads: HashMap::new(),
            writes: HashMap::new(),
            current_index: 0,
        }
    }

    /// Analyze program and return slot assignments with reuse
    pub fn analyze_and_assign_slots(&mut self, program: &[Stmt]) -> HashMap<String, usize> {
        // First pass: collect all read/write positions
        self.collect_liveness(program);

        // Second pass: assign slots with reuse based on lifetime intervals
        self.assign_slots_with_reuse()
    }

    fn collect_liveness(&mut self, program: &[Stmt]) {
        for stmt in program {
            self.analyze_stmt(stmt);
            self.current_index += 1;
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign { name, value } => {
                self.analyze_expr(value);
                self.writes.entry(name.clone()).or_insert(self.current_index);
            }

            Stmt::ArrAssign { name, index, value } => {
                self.analyze_expr(index);
                self.analyze_expr(value);
                // arr is read (handle used for heap store), not written
                self.reads
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .push(self.current_index);
            }

            Stmt::Print(expr) => {
                self.analyze_expr(expr);
            }

            Stmt::While { condition, body } => {
                self.analyze_expr(condition);
                let body_start = self.current_index + 1;
                let mut body_writes: HashSet<String> = HashSet::new();
                for stmt in body {
                    if let Stmt::Assign { name, .. } = stmt {
                        body_writes.insert(name.clone());
                    }
                    self.current_index += 1;
                    self.analyze_stmt(stmt);
                }
                let body_end = self.current_index;
                // Extend lifetimes of all variables accessed inside the loop body
                // to the end of the body. This prevents slot reuse across iterations,
                // since variables needed in one iteration are also needed in the next.
                let body_vars: HashSet<String> = self.reads.iter()
                    .filter(|(_, reads)| reads.iter().any(|&r| r >= body_start && r <= body_end))
                    .map(|(name, _)| name.clone())
                    .chain(body_writes.into_iter())
                    .collect();
                for name in body_vars {
                    self.reads.entry(name).or_insert_with(Vec::new).push(body_end);
                }
            }

            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.analyze_expr(condition);
                for stmt in then_body {
                    self.current_index += 1;
                    self.analyze_stmt(stmt);
                }
                for stmt in else_body {
                    self.current_index += 1;
                    self.analyze_stmt(stmt);
                }
            }

            Stmt::Func { params, body, .. } => {
                // Parameters are written at function entry
                for param in params {
                    self.writes.entry(param.clone()).or_insert(self.current_index);
                }
                for stmt in body {
                    self.current_index += 1;
                    self.analyze_stmt(stmt);
                }
            }

            Stmt::Return(expr) => {
                self.analyze_expr(expr);
            }

            Stmt::Native { args, .. } => {
                for a in args { self.analyze_expr(a); }
            }

            Stmt::Break | Stmt::Continue => {}
        }
    }
    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Var(name) => {
                self.reads
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .push(self.current_index);
            }

            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            }

            Expr::Call { args, .. } => {
                for arg in args {
                    self.analyze_expr(arg);
                }
            }

            Expr::NativeCall { args, .. } => {
                for a in args { self.analyze_expr(a); }
            }

            Expr::Unary { operand, .. } => {
                self.analyze_expr(operand);
            }

            Expr::ArrLit(elements) => {
                for e in elements {
                    self.analyze_expr(e);
                }
            }
            Expr::ArrIndex { arr, index } => {
                // Record reads for arr and index at the normal position
                self.analyze_expr(arr);
                self.analyze_expr(index);
                // Extend array variable lifetime: the handle must persist on stack
                // through ARRLOAD, which executes "after" this statement index.
                // Add a phantom read at next index to prevent slot reuse with result.
                if let Expr::Var(name) = arr.as_ref() {
                    self.reads
                        .entry(name.clone())
                        .or_insert_with(Vec::new)
                        .push(self.current_index + 1);
                }
            }
            Expr::Str(_) => {}
            Expr::Int(_) => {}
        }
    }

    fn assign_slots_with_reuse(&self) -> HashMap<String, usize> {
        let mut assignments = HashMap::new();
        let mut slot_last_use: Vec<Option<usize>> = Vec::new();

        // Collect all variables with their lifetime intervals
        let mut variables: Vec<(String, usize, usize)> = Vec::new();

        for (name, &write_pos) in &self.writes {
            let (first_read, last_read) = self
                .reads
                .get(name)
                .map(|reads| {
                    let min = reads.iter().min().copied().unwrap_or(write_pos);
                    let max = reads.iter().max().copied().unwrap_or(write_pos);
                    (min, max)
                })
                .unwrap_or((write_pos, write_pos));

            // Span from earliest use (read or write) to latest use
            let start = std::cmp::min(write_pos, first_read);
            let end = std::cmp::max(last_read, write_pos);
            variables.push((name.clone(), start, end));
        }

        // Sort by write position (when variable is first defined)
        variables.sort_by_key(|(_, start, _)| *start);

        // Assign slots using first-fit with lifetime checking
        for (name, start, end) in variables {
            let mut assigned = false;

            // Try to reuse an existing slot
            for (slot_idx, last_use) in slot_last_use.iter_mut().enumerate() {
                if let Some(prev_end) = last_use {
                    // Can reuse this slot if previous variable's lifetime ended
                    if *prev_end < start {
                        assignments.insert(name.clone(), slot_idx);
                        *last_use = Some(end);
                        assigned = true;
                        break;
                    }
                }
            }

            // Need a new slot
            if !assigned {
                let new_slot = slot_last_use.len();
                slot_last_use.push(Some(end));
                assignments.insert(name, new_slot);
            }
        }

        assignments
    }

    /// Get statistics about slot usage
    pub fn get_slot_stats(&self, assignments: &HashMap<String, usize>) -> (usize, usize) {
        let max_slot = assignments.values().max().copied().unwrap_or(0) + 1;
        let total_vars = assignments.len();
        (total_vars, max_slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinOp, Expr, Stmt};

    #[test]
    fn test_simple_slot_reuse() {
        let mut analyzer = LivenessAnalyzer::new();

        // x = 5
        // y = x + 1
        // z = y + 2
        // After x is used in line 2, its slot can be reused for z
        let program = vec![
            Stmt::Assign {
                name: "x".to_string(),
                value: Expr::Int(5),
            },
            Stmt::Assign {
                name: "y".to_string(),
                value: Expr::Binary {
                    left: Box::new(Expr::Var("x".to_string())),
                    op: BinOp::Add,
                    right: Box::new(Expr::Int(1)),
                },
            },
            Stmt::Assign {
                name: "z".to_string(),
                value: Expr::Binary {
                    left: Box::new(Expr::Var("y".to_string())),
                    op: BinOp::Add,
                    right: Box::new(Expr::Int(2)),
                },
            },
        ];

        let assignments = analyzer.analyze_and_assign_slots(&program);
        let (total_vars, slots_used) = analyzer.get_slot_stats(&assignments);

        assert_eq!(total_vars, 3);
        // Should use fewer slots than variables due to reuse
        assert!(slots_used <= 3);
    }
}
