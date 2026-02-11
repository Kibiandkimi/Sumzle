use crate::constraints::build_constraints;
use crate::evaluator::{is_integer, validate_expression};
use crate::types::*;
use std::collections::HashMap;

/// 单线程回溯求解器
pub struct Solver {
    pub constraints: Constraints,
    pub solutions: Vec<String>,
    pub max_solutions: usize,
    charset: Vec<u8>,
}

impl Solver {
    pub fn new(constraints: Constraints, max_solutions: usize) -> Self {
        // 根据约束过滤可用字符集
        let charset: Vec<u8> = CHARSET
            .iter()
            .filter(|&&ch| !constraints.globally_forbidden.contains(&ch))
            .cloned()
            .collect();

        Self {
            constraints,
            solutions: Vec::new(),
            max_solutions,
            charset,
        }
    }

    /// 从指定前缀开始搜索
    pub fn solve_with_prefix(&mut self, prefix: &[u8]) {
        let mut buffer = Vec::with_capacity(self.constraints.length);
        let mut char_counts: HashMap<u8, usize> = HashMap::new();

        // 填入前缀
        for &ch in prefix {
            buffer.push(ch);
            *char_counts.entry(ch).or_insert(0) += 1;
        }

        self.backtrack(&mut buffer, &mut char_counts);
    }

    pub fn solve(&mut self) {
        self.solve_with_prefix(&[]);
    }

    fn backtrack(&mut self, buffer: &mut Vec<u8>, char_counts: &mut HashMap<u8, usize>) {
        if self.solutions.len() >= self.max_solutions {
            return;
        }

        let pos = buffer.len();
        let length = self.constraints.length;

        if pos == length {
            // 完成一个候选串，验证
            let expr: String = buffer.iter().map(|&b| b as char).collect();
            if let Ok(true) = validate_expression(&expr) {
                self.solutions.push(expr);
            }
            return;
        }

        // 确定此位置可选字符
        let candidates = if let Some(&fixed) = self.constraints.fixed_chars.get(&pos) {
            vec![fixed]
        } else {
            self.charset.clone()
        };

        for ch in candidates {
            if self.can_place_char(buffer, char_counts, pos, ch) {
                buffer.push(ch);
                *char_counts.entry(ch).or_insert(0) += 1;

                self.backtrack(buffer, char_counts);

                buffer.pop();
                let count = char_counts.get_mut(&ch).unwrap();
                *count -= 1;
                if *count == 0 {
                    char_counts.remove(&ch);
                }

                if self.solutions.len() >= self.max_solutions {
                    return;
                }
            }
        }
    }

    fn can_place_char(
        &self,
        buffer: &[u8],
        char_counts: &HashMap<u8, usize>,
        pos: usize,
        ch: u8,
    ) -> bool {
        let length = self.constraints.length;

        // 1. cannot_be_at 检查
        if let Some(forbidden) = self.constraints.cannot_be_at.get(&pos) {
            if forbidden.contains(&ch) {
                return false;
            }
        }

        // 2. 全局禁用
        if self.constraints.globally_forbidden.contains(&ch) {
            return false;
        }

        // 3. 精确计数约束
        let current_count = char_counts.get(&ch).cloned().unwrap_or(0);
        if let Some(&exact) = self.constraints.must_appear_exact_count.get(&ch) {
            if current_count >= exact {
                return false; // 已经达到精确计数上限
            }
        }

        // 4. 剩余位置能否满足 must_appear_min_count
        let remaining = length - pos - 1;
        if !self.can_satisfy_min_counts(buffer, char_counts, pos, ch, remaining) {
            return false;
        }

        // 5. 语法合法性剪枝
        if !self.syntax_check(buffer, pos, ch) {
            return false;
        }

        true
    }

    fn can_satisfy_min_counts(
        &self,
        _buffer: &[u8],
        char_counts: &HashMap<u8, usize>,
        _pos: usize,
        new_ch: u8,
        remaining: usize,
    ) -> bool {
        // 计算还需要多少个必须出现的字符
        let mut needed = 0usize;
        for (&ch, &min_count) in &self.constraints.must_appear_min_count {
            let mut current = char_counts.get(&ch).cloned().unwrap_or(0);
            if ch == new_ch {
                current += 1;
            }
            if current < min_count {
                needed += min_count - current;
            }
        }
        needed <= remaining
    }

    fn syntax_check(&self, buffer: &[u8], pos: usize, ch: u8) -> bool {
        let length = self.constraints.length;

        // 规则1: 表达式不能以运算符开头（除了开括号和[）
        if pos == 0 {
            match ch {
                b'+' | b'*' | b'/' | b'%' | b'^' | b'=' | b'>' | b')' | b']' | b'!' | b'A' => {
                    return false;
                }
                _ => {}
            }
        }

        // 规则2: 表达式不能以运算符结尾（最后一个位置）
        if pos == length - 1 {
            match ch {
                b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'(' | b'[' | b'A' => {
                    return false;
                }
                b'=' | b'>' => {
                    return false; // 关系符不能在最后
                }
                _ => {}
            }
        }

        // 获取前一个字符
        let prev = if pos > 0 {
            Some(buffer[pos - 1])
        } else {
            None
        };

        // 规则3: 不能连续两个二元运算符
        if let Some(p) = prev {
            let prev_is_binop = matches!(p, b'+' | b'-' | b'*' | b'/' | b'%' | b'^');
            let ch_is_binop = matches!(ch, b'+' | b'*' | b'/' | b'%' | b'^');
            if prev_is_binop && ch_is_binop {
                return false;
            }
            // 运算符后不能直接跟 ) 或 ]
            if prev_is_binop && matches!(ch, b')' | b']') {
                return false;
            }
            // ( 后不能直接跟二元运算符（负号除外，但简化处理）
            if p == b'(' && ch_is_binop {
                return false;
            }
        }

        // 规则4: 只能有一个关系符(= 或 >)
        if ch == b'=' || ch == b'>' {
            let relation_count = buffer.iter().filter(|&&c| c == b'=' || c == b'>').count();
            if relation_count >= 1 {
                return false; // 已经有一个关系符了
            }
            // 关系符不能在开头
            if pos == 0 {
                return false;
            }
        }

        // 规则5: 必须有恰好一个关系符
        if pos == length - 1 {
            let mut total_relations = buffer.iter().filter(|&&c| c == b'=' || c == b'>').count();
            if ch == b'=' || ch == b'>' {
                total_relations += 1;
            }
            if total_relations != 1 {
                return false;
            }
        }

        // 规则6: 括号配对
        {
            let mut paren_depth = 0i32;
            let mut bracket_depth = 0i32;
            for &c in buffer.iter() {
                match c {
                    b'(' => paren_depth += 1,
                    b')' => paren_depth -= 1,
                    b'[' => bracket_depth += 1,
                    b']' => bracket_depth -= 1,
                    _ => {}
                }
            }
            match ch {
                b'(' => paren_depth += 1,
                b')' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        return false;
                    }
                }
                b'[' => bracket_depth += 1,
                b']' => {
                    bracket_depth -= 1;
                    if bracket_depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
            // 最后一个位置必须括号全部闭合
            if pos == length - 1 && (paren_depth != 0 || bracket_depth != 0) {
                return false;
            }
            // 剩余位置数必须 >= 未闭合括号数
            let remaining = length - pos - 1;
            let unclosed = paren_depth as usize + bracket_depth as usize;
            if unclosed > remaining {
                return false;
            }
        }

        // 规则7: 无前导0
        if ch == b'0' {
            if pos == 0 {
                // 如果长度 > 1 且下一个位置不是运算符/关系符，则前导0
                // 但这需要前瞻，简化：允许0但后续检查
                // 实际上在位置0，如果下一个字符也是数字就是前导0
                // 这里不做强剪枝，在完整验证中检查
            } else if let Some(p) = prev {
                // 前一个是运算符或(或[或关系符，则当前0可能是前导0
                let prev_is_start =
                    matches!(p, b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'(' | b'[' | b'=' | b'>');
                if prev_is_start && pos + 1 < length {
                    // 下一个位置如果固定为数字，则前导0
                    if let Some(&next_fixed) = self.constraints.fixed_chars.get(&(pos + 1)) {
                        if next_fixed.is_ascii_digit() {
                            return false;
                        }
                    }
                    // 不确定时允许，后续再验证
                }
            }
        }
        // 检查当前位置是数字且前一个是0，且0之前是运算符起始
        if ch.is_ascii_digit() && pos >= 1 {
            if buffer[pos - 1] == b'0' {
                if pos == 1 {
                    // 开头0后跟数字 → 前导0
                    return false;
                } else if pos >= 2 {
                    let pp = buffer[pos - 2];
                    let pp_is_start = matches!(
                        pp,
                        b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'(' | b'[' | b'=' | b'>'
                    );
                    if pp_is_start {
                        return false;
                    }
                }
            }
        }

        // 规则8: = 右侧只能是纯整数（可带负号）
        // 找到缓冲区中是否有 =
        if let Some(eq_pos) = buffer.iter().position(|&c| c == b'=') {
            // 当前位置在 = 右侧
            if pos > eq_pos {
                // = 右侧只能是数字或开头负号
                if pos == eq_pos + 1 {
                    // = 后第一个字符
                    if !ch.is_ascii_digit() && ch != b'-' {
                        return false;
                    }
                } else {
                    // = 后非第一个字符
                    if !ch.is_ascii_digit() {
                        return false;
                    }
                    // 检查是否 = 后面是 -，若是则当前必须是数字（已满足）
                }
            }
        }
        // 如果当前字符是 =，检查后面是否还有足够空间放至少一个数字
        if ch == b'=' {
            let remaining = length - pos - 1;
            if remaining < 1 {
                return false;
            }
        }

        // 规则9: ! 只能跟在数字或 ) 或 ] 后面
        if ch == b'!' {
            if let Some(p) = prev {
                if !p.is_ascii_digit() && p != b')' && p != b']' && p != b'!' {
                    return false;
                }
            } else {
                return false;
            }
        }

        // 规则10: A 只能跟在数字或 ) 或 ] 或 ! 后面
        if ch == b'A' {
            if let Some(p) = prev {
                if !p.is_ascii_digit() && p != b')' && p != b']' && p != b'!' {
                    return false;
                }
            } else {
                return false;
            }
        }

        // 规则11: 关系符（> 在这里）右侧可以有表达式
        // > 已在规则4/5中处理

        true
    }
}

/// 生成所有可能的前缀用于并行分发
pub fn generate_prefixes(constraints: &Constraints, prefix_len: usize) -> Vec<Vec<u8>> {
    let charset: Vec<u8> = CHARSET
        .iter()
        .filter(|&&ch| !constraints.globally_forbidden.contains(&ch))
        .cloned()
        .collect();

    let mut prefixes = Vec::new();
    let mut buffer = Vec::with_capacity(prefix_len);
    generate_prefixes_recursive(
        constraints,
        &charset,
        &mut buffer,
        prefix_len,
        &mut prefixes,
    );
    prefixes
}

fn generate_prefixes_recursive(
    constraints: &Constraints,
    charset: &[u8],
    buffer: &mut Vec<u8>,
    target_len: usize,
    result: &mut Vec<Vec<u8>>,
) {
    if buffer.len() == target_len {
        result.push(buffer.clone());
        return;
    }

    let pos = buffer.len();
    let candidates = if let Some(&fixed) = constraints.fixed_chars.get(&pos) {
        vec![fixed]
    } else {
        charset.to_vec()
    };

    // 创建临时solver仅用于语法检查
    let temp_solver = Solver::new(constraints.clone(), 0);

    for ch in candidates {
        // 基本剪枝
        if constraints.globally_forbidden.contains(&ch) {
            continue;
        }
        if let Some(forbidden) = constraints.cannot_be_at.get(&pos) {
            if forbidden.contains(&ch) {
                continue;
            }
        }
        // 语法检查
        if !temp_solver.syntax_check(buffer, pos, ch) {
            continue;
        }

        buffer.push(ch);
        generate_prefixes_recursive(constraints, charset, buffer, target_len, result);
        buffer.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_solve() {
        // 假设约束已经非常紧，例如 "1+2=3"
        let mut constraints = Constraints::new(5);
        constraints.fixed_chars.insert(0, b'1');
        constraints.fixed_chars.insert(1, b'+');
        constraints.fixed_chars.insert(2, b'2');
        constraints.fixed_chars.insert(3, b'=');
        constraints.fixed_chars.insert(4, b'3');

        let mut solver = Solver::new(constraints, 100);
        solver.solve();
        assert_eq!(solver.solutions.len(), 1);
        assert_eq!(solver.solutions[0], "1+2=3");
    }

    #[test]
    fn test_partial_solve() {
        let mut constraints = Constraints::new(5);
        constraints.fixed_chars.insert(1, b'+');
        constraints.fixed_chars.insert(3, b'=');
        // 较宽松约束
        let mut solver = Solver::new(constraints, 10);
        solver.solve();
        // 应该能找到一些解如 "1+2=3", "2+3=5" 等
        assert!(!solver.solutions.is_empty());
        for sol in &solver.solutions {
            println!("Found: {}", sol);
        }
    }
}
