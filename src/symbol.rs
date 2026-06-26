use std::collections::HashMap;

use crate::ast::{Expr, Stmt};
use crate::liveness::LivenessAnalyzer;

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

    pub fn new_with_slots(slot_assignments: HashMap<String, usize>) -> Self {
        let max_slot = slot_assignments.values().max().copied().unwrap_or(0);
        Self {
            symbols: slot_assignments,
            next_slot: max_slot + 1,
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
    // Use liveness analysis for slot reuse
    let mut analyzer = LivenessAnalyzer::new();
    let slot_assignments = analyzer.analyze_and_assign_slots(program);
    
    let (total_vars, slots_used) = analyzer.get_slot_stats(&slot_assignments);
    println!("  Variables: {}, Slots used: {} (saved {} slots)", 
        total_vars, slots_used, total_vars.saturating_sub(slots_used));
    
    SymbolTable::new_with_slots(slot_assignments)
}

