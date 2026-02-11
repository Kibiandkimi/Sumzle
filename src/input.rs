use crate::types::*;
use std::io::{self, BufRead, Write};

/// 交互式录入
pub fn interactive_input() -> Result<PuzzleInput, String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // 输入长度
    print!("Enter expression length (3-15): ");
    stdout.flush().unwrap();
    let mut line = String::new();
    stdin.lock().read_line(&mut line).map_err(|e| e.to_string())?;
    let length: usize = line.trim().parse().map_err(|e| format!("Invalid length: {}", e))?;

    if length < 3 || length > 15 {
        return Err("Length must be 3-15".into());
    }

    println!("Enter guess rows (empty line to finish).");
    println!("Format per cell: <char><color> where color = g(green)/y(yellow)/x(gray)");
    println!("Example for length 5: 1g +y 2x =g 3x");
    println!("Or paste JSON: {{\"length\": 5, \"rows\": [...]}}");

    let mut rows = Vec::new();

    loop {
        print!("Row {} (or empty to finish): ", rows.len() + 1);
        stdout.flush().unwrap();
        let mut line = String::new();
        stdin.lock().read_line(&mut line).map_err(|e| e.to_string())?;
        let line = line.trim();

        if line.is_empty() {
            if rows.is_empty() {
                println!("At least one row is required.");
                continue;
            }
            break;
        }

        // 尝试解析JSON
        if line.starts_with('{') {
            match serde_json::from_str::<PuzzleInput>(line) {
                Ok(input) => return Ok(input),
                Err(e) => {
                    println!("JSON parse error: {}. Try cell format.", e);
                    continue;
                }
            }
        }

        // 解析 "1g +y 2x =g 3x" 格式
        match parse_row_string(line, length) {
            Ok(row) => rows.push(row),
            Err(e) => {
                println!("Parse error: {}. Try again.", e);
                continue;
            }
        }
    }

    Ok(PuzzleInput { length, rows })
}

fn parse_row_string(s: &str, expected_length: usize) -> Result<GuessRow, String> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != expected_length {
        return Err(format!(
            "Expected {} cells, got {}",
            expected_length,
            parts.len()
        ));
    }

    let mut cells = Vec::new();
    for part in parts {
        let chars: Vec<char> = part.chars().collect();
        if chars.len() < 2 {
            return Err(format!("Cell '{}' too short, need <char><color>", part));
        }

        let ch = chars[0];
        let color_char = chars[chars.len() - 1];
        let color = match color_char {
            'g' | 'G' => CellColor::Green,
            'y' | 'Y' => CellColor::Yellow,
            'x' | 'X' | 'r' | 'R' => CellColor::Gray, // x=gray, r=gray
            _ => return Err(format!("Unknown color '{}' in cell '{}'", color_char, part)),
        };

        // 如果字符本身多于1字符（如特殊字符），取第一个
        cells.push(CellGuess { ch, color });
    }

    Ok(GuessRow { cells })
}

/// 从JSON字符串解析
pub fn parse_json_input(json: &str) -> Result<PuzzleInput, String> {
    serde_json::from_str(json).map_err(|e| format!("JSON parse error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_row() {
        let row = parse_row_string("1g +y 2x =g 3x", 5).unwrap();
        assert_eq!(row.cells.len(), 5);
        assert_eq!(row.cells[0].ch, '1');
        assert_eq!(row.cells[0].color, CellColor::Green);
        assert_eq!(row.cells[1].ch, '+');
        assert_eq!(row.cells[1].color, CellColor::Yellow);
    }

    #[test]
    fn test_parse_json() {
        let json = r#"{"length":5,"rows":[{"cells":[{"ch":"1","color":"Green"},{"ch":"+","color":"Yellow"},{"ch":"2","color":"Gray"},{"ch":"=","color":"Green"},{"ch":"3","color":"Gray"}]}]}"#;
        let input = parse_json_input(json).unwrap();
        assert_eq!(input.length, 5);
        assert_eq!(input.rows.len(), 1);
    }
}
