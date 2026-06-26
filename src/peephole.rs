/// Bytecode peephole optimizer
/// Removes redundant patterns and optimizes instruction sequences
pub struct PeepholeOptimizer;

impl PeepholeOptimizer {
    pub fn new() -> Self {
        Self
    }

    pub fn optimize(&self, bytecode: &[u8]) -> Vec<u8> {
        let mut current = bytecode.to_vec();
        loop {
            let pass = self.run_pass(&current);
            if pass.len() == current.len() && pass == current {
                return pass;
            }
            current = pass;
        }
    }

    fn run_pass(&self, bytecode: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        let mut i = 0;
        // map old address -> new address for jump fixup
        let mut addr_map: Vec<Option<usize>> = vec![None; bytecode.len()];

        while i < bytecode.len() {
            let start = result.len();
            let optimized = self.try_opt(bytecode, i);

            if let Some((replacement, skip)) = optimized {
                for j in 0..skip {
                    addr_map[i + j] = if j < replacement.len() {
                        Some(start + j)
                    } else {
                        None
                    };
                }
                result.extend_from_slice(&replacement);
                i += skip;
            } else {
                addr_map[i] = Some(start);
                result.push(bytecode[i]);
                i += 1;
            }
        }

        self.adjust_jumps(&mut result, &addr_map);
        result
    }

    fn adjust_jumps(&self, code: &mut Vec<u8>, map: &[Option<usize>]) {
        let mut i = 0;
        while i < code.len() {
            let opcode = code[i];
            // Instructions with 1-byte operand that is a jump/call address
            if opcode == 0x40 || opcode == 0x41 || opcode == 0x60 {
                if i + 1 < code.len() {
                    let old_target = code[i + 1] as usize;
                    if old_target < map.len() {
                        if let Some(new_target) = map[old_target] {
                            code[i + 1] = new_target as u8;
                        }
                    }
                }
                i += 2;
            } else {
                // Skip operand for other 1-byte-operand instructions
                match opcode {
                    0x01 | 0x08 | 0x20 | 0x21 | 0x50 => i += 2,
                    _ => i += 1,
                }
            }
        }
    }

    fn try_opt(&self, code: &[u8], i: usize) -> Option<(Vec<u8>, usize)> {
        if i >= code.len() {
            return None;
        }

        let opcode = code[i];

        // Pattern: LOAD x, STORE x -> delete both (redundant load/store)
        if opcode == 0x20 && i + 3 < code.len() {
            // Guard: ensure code[i] is a LOAD opcode, not an operand byte.
            // Opcodes with 1-byte operands: 0x01(PUSH), 0x08(ARRLIT), 0x20(LOAD),
            // 0x21(STORE), 0x40(JMP), 0x41(JMP_IF_FALSE), 0x50(CALL_NATIVE), 0x60(CALL)
            let is_operand = i >= 1 && matches!(code[i - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60);
            if !is_operand {
                let slot = code[i + 1];
                if code[i + 2] == 0x21 && code[i + 3] == slot {
                    return Some((vec![], 4));
                }
            }
        }

        // Pattern: PUSH 0, ADD -> delete all
        if self.match_push_op(code, i, 0, 0x02) {
            return Some((vec![], 3));
        }

        // Pattern: PUSH 0, SUB -> delete all
        if self.match_push_op(code, i, 0, 0x03) {
            return Some((vec![], 3));
        }

        // Pattern: PUSH 0, MUL -> PUSH 0
        if self.match_push_op(code, i, 0, 0x04) {
            return Some((vec![0x01, 0], 3));
        }

        // Pattern: PUSH 1, MUL -> delete all
        if self.match_push_op(code, i, 1, 0x04) {
            return Some((vec![], 3));
        }

        // Pattern: PUSH 1, DIV -> delete all
        if self.match_push_op(code, i, 1, 0x05) {
            return Some((vec![], 3));
        }

        // Pattern: PUSH a, PUSH b, OP -> PUSH result (constant folding)
        if let Some(result) = self.try_fold_constants(code, i) {
            return Some(result);
        }

        // Pattern: JMP to next instruction -> delete JMP
        if opcode == 0x40 && i + 2 < code.len() {
            let is_operand = i >= 1 && matches!(code[i - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60);
            if !is_operand {
                let target = code[i + 1] as usize;
                if target == i + 2 {
                    return Some((vec![], 2));
                }
            }
        }

        // Pattern: JMP addr where code[addr] is JMP addr2 -> redirect to addr2
        if opcode == 0x40 && i + 1 < code.len() {
            let is_operand = i >= 1 && matches!(code[i - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60);
            if !is_operand {
                let target = code[i + 1] as usize;
                if target + 1 < code.len() && code[target] == 0x40 {
                    // Guard: ensure JMP at target is also a real opcode
                    let target_is_operand = target >= 1 && matches!(code[target - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60);
                    if !target_is_operand {
                        let target2 = code[target + 1] as usize;
                        if target2 != target {
                            return Some((vec![0x40, target2 as u8], 2));
                        }
                    }
                }
            }
        }

        // Pattern: Dead store — PUSH v, STORE x with no LOAD x before next STORE x
        if let Some(result) = self.try_dead_store(code, i) {
            return Some(result);
        }

        None
    }

    fn match_push_op(&self, code: &[u8], i: usize, val: u8, op: u8) -> bool {
        if code.get(i) != Some(&0x01) {
            return false;
        }
        // Guard: ensure code[i] is a PUSH opcode, not an operand byte.
        if i >= 1 {
            match code[i - 1] {
                0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60 => return false,
                _ => {}
            }
        }
        code.get(i + 1) == Some(&val) && code.get(i + 2) == Some(&op)
    }

    fn try_fold_constants(&self, code: &[u8], i: usize) -> Option<(Vec<u8>, usize)> {
        if i + 4 >= code.len() {
            return None;
        }
        if code[i] != 0x01 || code[i + 2] != 0x01 {
            return None;
        }
        // Guard: ensure code[i] is a PUSH opcode, not an operand byte.
        // Opcodes with 1-byte operands: 0x01(PUSH), 0x08(ARRLIT), 0x20(LOAD),
        // 0x21(STORE), 0x40(JMP), 0x41(JMP_IF_FALSE), 0x50(CALL_NATIVE), 0x60(CALL)
        if i >= 1 {
            let prev = code[i - 1];
            match prev {
                0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60 => return None,
                _ => {}
            }
        }

        let a = code[i + 1] as i8 as i32;
        let b = code[i + 2 + 1] as i8 as i32;
        let op = code[i + 4];

        let result = match op {
            0x02 => a.wrapping_add(b),
            0x03 => a.wrapping_sub(b),
            0x04 => a.wrapping_mul(b),
            0x05 => {
                if b != 0 {
                    a.wrapping_div(b)
                } else {
                    return None;
                }
            }
            0x30 => (a == b) as i32,
            0x31 => (a < b) as i32,
            0x32 => (a > b) as i32,
            0x34 => (a <= b) as i32,
            0x35 => (a >= b) as i32,
            _ => return None,
        };

        let result_u8 = result as u8;
        if result_u8 as i32 == result {
            Some((vec![0x01, result_u8], 5))
        } else {
            None
        }
    }

    /// Pattern: PUSH v, STORE x ... PUSH w, STORE x (same slot, no LOAD x between)
    /// The first PUSH+STORE is dead — value never read before being overwritten.
    fn try_dead_store(&self, code: &[u8], i: usize) -> Option<(Vec<u8>, usize)> {
        if code.get(i) != Some(&0x01) {
            return None;
        }
        // Guard: code[i] is a real PUSH opcode
        if i >= 1 && matches!(code[i - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60) {
            return None;
        }
        if i + 3 >= code.len() || code[i + 2] != 0x21 {
            return None;
        }
        let slot = code[i + 3];

        // Scan forward from i+4 looking for next reference to `slot`
        let max_lookahead = 32;
        let mut j = i + 4;
        while j < code.len() && (j - i) < max_lookahead {
            let is_operand = j >= 1
                && matches!(code[j - 1], 0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 | 0x60);
            if is_operand {
                j += 1;
                continue;
            }

            let op = code[j];
            match op {
                0x20 if j + 1 < code.len() && code[j + 1] == slot => return None, // LOAD x — live
                0x21 if j + 1 < code.len() && code[j + 1] == slot => {
                    // STORE x without intervening LOAD x — dead store, remove 4 bytes
                    return Some((vec![], 4));
                }
                0x60 | 0x61 => return None, // CALL or RET — can't safely analyze
                0x01 | 0x08 | 0x20 | 0x21 | 0x40 | 0x41 | 0x50 => j += 2,
                _ => j += 1,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_add_zero() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 5, 0x01, 0, 0x02];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 5]);
    }

    #[test]
    fn test_remove_mul_zero() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 10, 0x01, 0, 0x04];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 0]);
    }

    #[test]
    fn test_remove_mul_one() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 42, 0x01, 1, 0x04];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 42]);
    }

    #[test]
    fn test_remove_redundant_load_store() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x20, 5, 0x21, 5];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn test_constant_fold_add() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 3, 0x01, 4, 0x02];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 7]);
    }

    #[test]
    fn test_constant_fold_cmp_eq() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 5, 0x01, 5, 0x30];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 1]);
    }

    #[test]
    fn test_constant_fold_cmp_lt() {
        let optimizer = PeepholeOptimizer::new();
        let code = vec![0x01, 3, 0x01, 7, 0x31];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 1]);
    }

    #[test]
    fn test_multi_pass() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 0, ADD inside a longer sequence should be removed
        let code = vec![0x01, 5, 0x01, 0, 0x02, 0x01, 1, 0x01, 0, 0x03];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 5, 0x01, 1]);
    }

    #[test]
    fn test_jump_to_jump_chain() {
        let optimizer = PeepholeOptimizer::new();
        // JMP 3 where @3 is JMP 5 -> redirect to JMP 5, then JMP 5 -> next -> deleted
        // Original: JMP 3 -> JMP 5 -> PRINT
        // @0-1: JMP 3, @2: PRINT (unreachable), @3-4: JMP 5, @5: PRINT (target)
        let code = vec![
            0x40, 3,    // @0-1: JMP 3
            0x51,       // @2:   PRINT
            0x40, 5,    // @3-4: JMP 5
            0x51,       // @5:   PRINT
        ];
        let result = optimizer.optimize(&code);
        // Pass 1: JMP 3 -> JMP 5, JMP 5 -> deleted (next). Result: [0x40, 5, 0x51, 0x51]
        // then adjust JMP target: addr_map[5] = Some(3) -> JMP 3
        // Pass 2: stable
        // Final: JMP 3 (direct to target PRINT), PRINT (unreachable), PRINT (target)
        assert_eq!(result, vec![0x40, 3, 0x51, 0x51]);
    }

    #[test]
    fn test_push_value_guard_jmp() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 64 (0x40) followed by SUB (0x03)
        // Without guard, 0x40 at offset 1 would be treated as JMP with target 0x03
        // i+2=3, target=3 -> would delete JMP (corrupting PUSH operand)
        // With guard: no corruption
        let code = vec![0x01, 0x40, 0x03];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 0x40, 0x03]);
    }

    #[test]
    fn test_push_value_guard_load_store() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 32 (0x20) followed by LOAD pattern
        // Without guard, 0x20 at offset 1 would be treated as LOAD
        // With guard: no corruption
        let code = vec![0x01, 0x20, 0x21, 0x05];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 0x20, 0x21, 0x05]);
    }

    #[test]
    fn test_dead_store_adjacent() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 0, STORE 0, PUSH 5, STORE 0 -> PUSH 5, STORE 0
        let code = vec![0x01, 0, 0x21, 0, 0x01, 5, 0x21, 0];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 5, 0x21, 0]);
    }

    #[test]
    fn test_dead_store_with_intervening() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 0, STORE 0, PUSH 99, STORE 1, PUSH 5, STORE 0
        // First STORE 0 is dead (slot 0 not loaded before second STORE 0)
        let code = vec![0x01, 0, 0x21, 0, 0x01, 99, 0x21, 1, 0x01, 5, 0x21, 0];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 99, 0x21, 1, 0x01, 5, 0x21, 0]);
    }

    #[test]
    fn test_dead_store_live_via_load() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 0, STORE 0, LOAD 0, ... -> store is live, don't touch
        let code = vec![0x01, 0, 0x21, 0, 0x20, 0, 0x01, 5, 0x21, 0];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 0, 0x21, 0, 0x20, 0, 0x01, 5, 0x21, 0]);
    }

    #[test]
    fn test_dead_store_diff_slot_live() {
        let optimizer = PeepholeOptimizer::new();
        // PUSH 0, STORE 1, PUSH 5, STORE 1 (different slot from other activity)
        // First STORE 1 is dead
        let code = vec![0x01, 0, 0x21, 1, 0x01, 99, 0x21, 2, 0x01, 5, 0x21, 1];
        let result = optimizer.optimize(&code);
        assert_eq!(result, vec![0x01, 99, 0x21, 2, 0x01, 5, 0x21, 1]);
    }

    #[test]
    fn test_jump_adj_after_removal() {
        let optimizer = PeepholeOptimizer::new();
        // LOAD 0, STORE 0 (4 bytes) removed before JMP that skips forward
        // Original layout:
        //   @0-3: LOAD 0, STORE 0 (removed)
        //   @4-5: PUSH 42
        //   @6:   PRINT
        //   @7-8: PUSH 1
        //   @9-10: JMP 13 (targets PRINT at @13)
        //   @11-12: PUSH 99 (skipped)
        //   @13:  PRINT (target)
        let code = vec![
            0x20, 0, 0x21, 0,  // @0-3: LOAD 0, STORE 0
            0x01, 42,          // @4-5: PUSH 42
            0x51,              // @6:   PRINT 42
            0x01, 1,           // @7-8: PUSH 1
            0x40, 13,          // @9-10: JMP 13 -> target PRINT at @13
            0x01, 99,          // @11-12: PUSH 99 (skipped)
            0x51,              // @13:  PRINT target
        ];
        let result = optimizer.optimize(&code);
        // After 4-byte removal:
        //   @0-1: PUSH 42, @2: PRINT, @3-4: PUSH 1,
        //   @5-6: JMP 9 (13-4=9), @7-8: PUSH 99, @9: PRINT
        assert_eq!(result, vec![0x01, 42, 0x51, 0x01, 1, 0x40, 9, 0x01, 99, 0x51]);
    }
}
