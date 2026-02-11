use std::collections::VecDeque;

/// 表达式求值引擎
/// 支持: +, -, *, /, %, ^, !, A(排列), [](取整), 括号
/// 
/// 运算符优先级:
///   1: +, -
///   2: *, /, %
///   3: ^ (右结合)
///   4: A (排列, 左结合)
///   5: ! (后缀阶乘), [] (取整) -- 作为后缀在 parse_unary 中处理

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Op(char),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Factorial,
    Permutation, // A
}

pub fn evaluate_expression(expr: &str) -> Result<f64, String> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(format!(
            "Unexpected token at position {}: {:?}",
            pos,
            tokens.get(pos)
        ));
    }
    Ok(result)
}

fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '0'..='9' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let num: f64 = num_str.parse().map_err(|e| format!("Parse error: {}", e))?;
                tokens.push(Token::Number(num));
            }
            '+' | '-' => {
                // 判断是否为一元负号/正号
                let is_unary = tokens.is_empty()
                    || matches!(
                        tokens.last(),
                        Some(Token::Op(_))
                            | Some(Token::LParen)
                            | Some(Token::LBracket)
                            | Some(Token::Permutation)
                    );
                if is_unary && ch == '-' {
                    // 一元负号：读取后续数字或标记为一元操作
                    if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                        i += 1;
                        let start = i;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                        let num_str: String = chars[start..i].iter().collect();
                        let num: f64 =
                            num_str.parse().map_err(|e| format!("Parse error: {}", e))?;
                        tokens.push(Token::Number(-num));
                    } else {
                        // 一元负号后面是括号等
                        tokens.push(Token::Number(0.0));
                        tokens.push(Token::Op('-'));
                        i += 1;
                    }
                } else if is_unary && ch == '+' {
                    i += 1; // 忽略一元正号
                } else {
                    tokens.push(Token::Op(ch));
                    i += 1;
                }
            }
            '*' | '/' | '%' | '^' => {
                tokens.push(Token::Op(ch));
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            '!' => {
                tokens.push(Token::Factorial);
                i += 1;
            }
            'A' => {
                tokens.push(Token::Permutation);
                i += 1;
            }
            ' ' => {
                i += 1;
            }
            _ => {
                return Err(format!("Unexpected character: '{}'", ch));
            }
        }
    }

    Ok(tokens)
}

// 递归下降解析器
// expr = term (('+' | '-') term)*
fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_term(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('+') => {
                *pos += 1;
                let right = parse_term(tokens, pos)?;
                left += right;
            }
            Token::Op('-') => {
                *pos += 1;
                let right = parse_term(tokens, pos)?;
                left -= right;
            }
            _ => break,
        }
    }

    Ok(left)
}

// term = power (('*' | '/' | '%') power)*
fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_permutation(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('*') => {
                *pos += 1;
                let right = parse_permutation(tokens, pos)?;
                left *= right;
            }
            Token::Op('/') => {
                *pos += 1;
                let right = parse_permutation(tokens, pos)?;
                if right == 0.0 {
                    return Err("Division by zero".into());
                }
                left /= right;
            }
            Token::Op('%') => {
                *pos += 1;
                let right = parse_permutation(tokens, pos)?;
                if right == 0.0 {
                    return Err("Modulo by zero".into());
                }
                left %= right;
            }
            _ => break,
        }
    }

    Ok(left)
}

// permutation = power ('A' power)*  -- nAr = n!/(n-r)!
fn parse_permutation(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_power(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Permutation => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                left = permutation(left, right)?;
            }
            _ => break,
        }
    }

    Ok(left)
}

// power = unary ('^' power)  -- 右结合
fn parse_power(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let base = parse_unary(tokens, pos)?;

    if *pos < tokens.len() {
        if let Token::Op('^') = &tokens[*pos] {
            *pos += 1;
            let exp = parse_power(tokens, pos)?; // 右结合：递归调用自身
            return Ok(base.powf(exp));
        }
    }

    Ok(base)
}

// unary = primary ('!')*
fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    let mut val = parse_primary(tokens, pos)?;

    // 后缀阶乘
    while *pos < tokens.len() {
        if let Token::Factorial = &tokens[*pos] {
            *pos += 1;
            val = factorial(val)?;
        } else {
            break;
        }
    }

    Ok(val)
}

// primary = Number | '(' expr ')' | '[' expr ']'
fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".into());
    }

    match &tokens[*pos] {
        Token::Number(n) => {
            let val = *n;
            *pos += 1;
            Ok(val)
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() {
                return Err("Missing closing parenthesis".into());
            }
            match &tokens[*pos] {
                Token::RParen => {
                    *pos += 1;
                    Ok(val)
                }
                _ => Err("Expected closing parenthesis".into()),
            }
        }
        Token::LBracket => {
            *pos += 1;
            let val = parse_expr(tokens, pos)?;
            if *pos >= tokens.len() {
                return Err("Missing closing bracket".into());
            }
            match &tokens[*pos] {
                Token::RBracket => {
                    *pos += 1;
                    Ok(val.floor()) // [] 取整（向下取整）
                }
                _ => Err("Expected closing bracket".into()),
            }
        }
        other => Err(format!("Unexpected token: {:?}", other)),
    }
}

fn factorial(n: f64) -> Result<f64, String> {
    if n < 0.0 || n != n.floor() {
        return Err(format!("Factorial of non-natural number: {}", n));
    }
    let n = n as u64;
    if n > 20 {
        return Err(format!("Factorial too large: {}!", n));
    }
    let mut result: u64 = 1;
    for i in 2..=n {
        result = result
            .checked_mul(i)
            .ok_or_else(|| format!("Factorial overflow: {}!", n))?;
    }
    Ok(result as f64)
}

fn permutation(n: f64, r: f64) -> Result<f64, String> {
    if n < 0.0 || r < 0.0 || n != n.floor() || r != r.floor() {
        return Err(format!("Permutation of non-natural numbers: {}A{}", n, r));
    }
    let n = n as u64;
    let r = r as u64;
    if r > n {
        return Ok(0.0);
    }
    if n > 20 {
        return Err(format!("Permutation too large: {}A{}", n, r));
    }
    let mut result: u64 = 1;
    for i in (n - r + 1)..=n {
        result = result
            .checked_mul(i)
            .ok_or_else(|| format!("Permutation overflow: {}A{}", n, r))?;
    }
    Ok(result as f64)
}

/// 检查浮点数是否为整数
pub fn is_integer(v: f64) -> bool {
    v.is_finite() && (v - v.round()).abs() < 1e-9
}

/// 验证完整表达式（含关系符）
pub fn validate_expression(expr: &str) -> Result<bool, String> {
    // 找主关系符 = 或 >
    let (relation, left_str, right_str) = find_relation(expr)?;

    let left_val = evaluate_expression(&left_str)?;
    let right_val = evaluate_expression(&right_str)?;

    // 结果必须是整数
    if !is_integer(left_val) {
        return Err(format!("Left side is not integer: {}", left_val));
    }
    if !is_integer(right_val) {
        return Err(format!("Right side is not integer: {}", right_val));
    }

    match relation {
        '=' => {
            // 右边必须是纯整数文本（可能带负号）
            if !is_pure_integer_text(&right_str) {
                return Err(format!(
                    "Right side of = must be pure integer text: '{}'",
                    right_str
                ));
            }
            let left_int = left_val.round() as i64;
            let right_int = right_val.round() as i64;
            Ok(left_int == right_int)
        }
        '>' => {
            let left_int = left_val.round() as i64;
            let right_int = right_val.round() as i64;
            Ok(left_int > right_int)
        }
        _ => Err(format!("Unknown relation: {}", relation)),
    }
}

fn find_relation(expr: &str) -> Result<(char, String, String), String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut depth = 0i32;
    let mut bracket_depth = 0i32;

    // 从左到右扫描，跳过括号/方括号内部，找到第一个未嵌套的 = 或 >
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '=' | '>' if depth == 0 && bracket_depth == 0 => {
                let left: String = chars[..i].iter().collect();
                let right: String = chars[i + 1..].iter().collect();
                if left.is_empty() || right.is_empty() {
                    return Err("Empty side of relation".into());
                }
                return Ok((ch, left, right));
            }
            _ => {}
        }
    }

    Err("No relation operator (= or >) found".into())
}

fn is_pure_integer_text(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let chars: Vec<char> = s.chars().collect();
    let start = if chars[0] == '-' { 1 } else { 0 };
    if start >= chars.len() {
        return false;
    }
    // 不能有前导0（除非是 "0" 本身）
    if chars.len() - start > 1 && chars[start] == '0' {
        return false;
    }
    chars[start..].iter().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_eval() {
        assert_eq!(evaluate_expression("1+2").unwrap(), 3.0);
        assert_eq!(evaluate_expression("3*4+2").unwrap(), 14.0);
        assert_eq!(evaluate_expression("2^3").unwrap(), 8.0);
        assert_eq!(evaluate_expression("5!").unwrap(), 120.0);
        assert_eq!(evaluate_expression("5A2").unwrap(), 20.0);
        assert_eq!(evaluate_expression("[3/2]").unwrap(), 1.0);
    }

    #[test]
    fn test_validate() {
        assert!(validate_expression("1+2=3").unwrap());
        assert!(!validate_expression("1+2=4").unwrap());
        assert!(validate_expression("5>3").unwrap());
        assert!(validate_expression("3!=6").unwrap()); // 3! = 6
    }

    #[test]
    fn test_power_right_assoc() {
        // 2^3^2 = 2^(3^2) = 2^9 = 512
        assert_eq!(evaluate_expression("2^3^2").unwrap(), 512.0);
    }

    #[test]
    fn test_nested_brackets() {
        assert_eq!(evaluate_expression("[7/2]").unwrap(), 3.0);
        assert_eq!(evaluate_expression("[[7/2]/2]").unwrap(), 1.0);
    }
}
