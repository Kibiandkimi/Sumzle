use crate::types::*;
use std::collections::{HashMap, HashSet};

/// 从猜测行构建约束，并检测矛盾
pub fn build_constraints(input: &PuzzleInput) -> Result<Constraints, String> {
    let mut constraints = Constraints::new(input.length);

    for row in &input.rows {
        if row.cells.len() != input.length {
            return Err(format!(
                "Row length {} != expected {}",
                row.cells.len(),
                input.length
            ));
        }
        process_row(row, &mut constraints)?;
    }

    // 后处理：从 globally_forbidden 清理
    post_process(&mut constraints)?;

    // 矛盾检测
    detect_contradictions(&constraints)?;

    Ok(constraints)
}

fn process_row(row: &GuessRow, constraints: &mut Constraints) -> Result<(), String> {
    let length = constraints.length;

    // 第一遍：统计每个字符在本行中的绿/黄/灰数量
    let mut char_green_yellow: HashMap<u8, usize> = HashMap::new();
    let mut char_gray: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut char_green: HashMap<u8, Vec<usize>> = HashMap::new();
    let mut char_yellow: HashMap<u8, Vec<usize>> = HashMap::new();

    for (pos, cell) in row.cells.iter().enumerate() {
        let ch = cell.ch as u8;
        match cell.color {
            CellColor::Green => {
                *char_green_yellow.entry(ch).or_insert(0) += 1;
                char_green.entry(ch).or_default().push(pos);
            }
            CellColor::Yellow => {
                *char_green_yellow.entry(ch).or_insert(0) += 1;
                char_yellow.entry(ch).or_default().push(pos);
            }
            CellColor::Gray => {
                char_gray.entry(ch).or_default().push(pos);
            }
        }
    }

    // 处理绿色：固定字符
    for (&ch, positions) in &char_green {
        for &pos in positions {
            if let Some(&existing) = constraints.fixed_chars.get(&pos) {
                if existing != ch {
                    return Err(format!(
                        "Conflict at position {}: fixed '{}' vs new green '{}'",
                        pos, existing as char, ch as char
                    ));
                }
            }
            constraints.fixed_chars.insert(pos, ch);
        }
    }

    // 处理黄色：字符不能在该位置
    for (&ch, positions) in &char_yellow {
        for &pos in positions {
            constraints
                .cannot_be_at
                .entry(pos)
                .or_default()
                .insert(ch);
        }
    }

    // 处理灰色 + 绿/黄组合推断
    for (&ch, gray_positions) in &char_gray {
        let gy_count = char_green_yellow.get(&ch).cloned().unwrap_or(0);

        if gy_count > 0 {
            // 同一行同字符既有绿/黄又有灰 → 精确出现次数 = gy_count
            let current_exact = constraints
                .must_appear_exact_count
                .entry(ch)
                .or_insert(gy_count);
            // 取较小值（多行约束取交集中更严格的）
            if gy_count < *current_exact {
                *current_exact = gy_count;
            }
            // 灰色位置仍然不能放该字符
            for &pos in gray_positions {
                constraints
                    .cannot_be_at
                    .entry(pos)
                    .or_default()
                    .insert(ch);
            }
        } else {
            // 纯灰色（本行无绿/黄）→ 全局禁用
            constraints.globally_forbidden.insert(ch);
            // 所有位置都不能放
            for pos in 0..length {
                constraints
                    .cannot_be_at
                    .entry(pos)
                    .or_default()
                    .insert(ch);
            }
        }
    }

    // 最小出现次数：绿+黄数量
    for (&ch, &count) in &char_green_yellow {
        let current = constraints.must_appear_min_count.entry(ch).or_insert(0);
        if count > *current {
            *current = count;
        }
    }

    Ok(())
}

fn post_process(constraints: &mut Constraints) -> Result<(), String> {
    // 如果一个字符有精确计数，且精确计数为0，则全局禁用
    let to_forbid: Vec<u8> = constraints
        .must_appear_exact_count
        .iter()
        .filter(|(_, &count)| count == 0)
        .map(|(&ch, _)| ch)
        .collect();

    for ch in to_forbid {
        constraints.globally_forbidden.insert(ch);
        constraints.must_appear_exact_count.remove(&ch);
    }

    // 全局禁用的字符不应有 min/exact count
    for &ch in &constraints.globally_forbidden {
        constraints.must_appear_min_count.remove(&ch);
        constraints.must_appear_exact_count.remove(&ch);
    }

    // 精确计数应 >= 最小计数
    for (&ch, &exact) in &constraints.must_appear_exact_count {
        if let Some(&min) = constraints.must_appear_min_count.get(&ch) {
            if exact < min {
                return Err(format!(
                    "Contradiction: char '{}' exact={} < min={}",
                    ch as char, exact, min
                ));
            }
        }
    }

    Ok(())
}

fn detect_contradictions(constraints: &Constraints) -> Result<(), String> {
    // 检查：同位置两个固定字符冲突（已在处理中检查）

    // 检查：固定字符是否被全局禁用
    for (&pos, &ch) in &constraints.fixed_chars {
        if constraints.globally_forbidden.contains(&ch) {
            return Err(format!(
                "Contradiction: pos {} fixed to '{}' but '{}' is globally forbidden",
                pos, ch as char, ch as char
            ));
        }
    }

    // 检查：固定字符在 cannot_be_at 自己位置
    for (&pos, &ch) in &constraints.fixed_chars {
        if let Some(forbidden) = constraints.cannot_be_at.get(&pos) {
            if forbidden.contains(&ch) {
                return Err(format!(
                    "Contradiction: pos {} fixed to '{}' but '{}' cannot be at pos {}",
                    pos, ch as char, ch as char, pos
                ));
            }
        }
    }

    // 检查：must_appear 的字符有足够的可用位置
    for (&ch, &min_count) in &constraints.must_appear_min_count {
        let mut available_positions = 0;
        for pos in 0..constraints.length {
            if let Some(&fixed) = constraints.fixed_chars.get(&pos) {
                if fixed == ch {
                    available_positions += 1;
                    continue;
                }
                continue; // 该位置已固定为其他字符
            }
            let forbidden_at_pos = constraints.cannot_be_at.get(&pos);
            if let Some(forbidden) = forbidden_at_pos {
                if forbidden.contains(&ch) {
                    continue;
                }
            }
            available_positions += 1;
        }
        if available_positions < min_count {
            return Err(format!(
                "Contradiction: char '{}' needs {} positions but only {} available",
                ch as char, min_count, available_positions
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_constraint_building() {
        let input = PuzzleInput {
            length: 5,
            rows: vec![GuessRow {
                cells: vec![
                    CellGuess {
                        ch: '1',
                        color: CellColor::Green,
                    },
                    CellGuess {
                        ch: '+',
                        color: CellColor::Yellow,
                    },
                    CellGuess {
                        ch: '2',
                        color: CellColor::Gray,
                    },
                    CellGuess {
                        ch: '=',
                        color: CellColor::Green,
                    },
                    CellGuess {
                        ch: '3',
                        color: CellColor::Gray,
                    },
                ],
            }],
        };

        let constraints = build_constraints(&input).unwrap();
        assert_eq!(constraints.fixed_chars.get(&0), Some(&b'1'));
        assert_eq!(constraints.fixed_chars.get(&3), Some(&b'='));
        assert!(constraints.globally_forbidden.contains(&b'2'));
        assert!(constraints.globally_forbidden.contains(&b'3'));
        assert!(constraints.cannot_be_at[&1].contains(&b'+'));
        assert_eq!(constraints.must_appear_min_count[&b'+'], 1);
    }
}
