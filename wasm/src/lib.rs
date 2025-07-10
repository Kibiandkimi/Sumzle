use wasm_bindgen::prelude::*;
use js_sys::Function as JsFunction;
use web_sys::console;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use meval::eval_str;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// Helper macro for logging to the console
macro_rules! console_log {
    ($($t:tt)*) => (log(&format!($($t)*)))
}

// Constraint data structure for the search algorithm
struct GlobalKnowledge {
    fixed_chars: Vec<Option<char>>,
    cannot_be_at: Vec<HashSet<char>>,
    must_appear_min_count: HashMap<char, usize>,
    must_appear_exact_count: HashMap<char, usize>,
    globally_forbidden: HashSet<char>,
}

// Context for floor brackets
#[derive(Clone, Copy)]
struct FloorContext {
    in_floor: bool,
    has_slash_in_current_floor: bool,
}

// Tile data structure for parsing constraints
#[derive(Serialize, Deserialize, Debug)]
struct Tile {
    char: String,
    state: String,
}

// Row data structure for parsing constraints
type Row = Vec<Tile>;

// Constraints data structure for parsing from JSON
#[derive(Serialize, Deserialize, Debug)]
struct Constraints {
    rows: Vec<Row>,
}

#[wasm_bindgen]
pub struct SumzleSolver {
    length: usize,
    valid_chars: String,
    max_operand_value: i32,
}

#[wasm_bindgen]
impl SumzleSolver {
    #[wasm_bindgen(constructor)]
    pub fn new(length: usize, max_operand_value: i32) -> Self {
        #[cfg(target_arch = "wasm32")]
        console::log_1(&"SumzleSolver initialized".into());

        Self {
            length,
            valid_chars: "0123456789+-*/%^=()![]>A".to_string(),
            max_operand_value,
        }
    }

    pub fn evaluate_expression(&self, expr: &str) -> Option<f64> {
        if expr.is_empty() {
            return None;
        }

        let mut processed_expr = expr.to_string();

        // Handle floor brackets [] by converting them to floor() function calls
        let mut bracket_iterations = 0;
        let max_bracket_iterations = 10;
        while processed_expr.contains('[') && bracket_iterations < max_bracket_iterations {
            // Find the position of the first opening bracket
            if let Some(start) = processed_expr.find('[') {
                // Find the matching closing bracket
                let mut depth = 1;
                let mut end = start + 1;

                while end < processed_expr.len() && depth > 0 {
                    match processed_expr.chars().nth(end) {
                        Some('[') => depth += 1,
                        Some(']') => depth -= 1,
                        None => return None, // Unexpected end of string
                        _ => {}
                    }
                    if depth > 0 {
                        end += 1;
                    }
                }

                if depth == 0 {
                    // Extract the expression inside the brackets
                    let inner_expr = &processed_expr[start + 1..end];

                    // Check if the inner expression is a simple number or a division expression
                    let is_simple_number = inner_expr.chars().all(|c| c.is_digit(10));
                    let is_division_expr = {
                        let parts: Vec<&str> = inner_expr.split('/').collect();
                        parts.len() == 2 && 
                        parts[0].chars().all(|c| c.is_digit(10)) && 
                        parts[1].chars().all(|c| c.is_digit(10))
                    };

                    if !is_simple_number && !is_division_expr {
                        return None; // Invalid content inside brackets
                    }

                    // Evaluate the inner expression
                    let inner_value = if is_simple_number {
                        inner_expr.parse::<f64>().ok()
                    } else {
                        // It's a division expression
                        let parts: Vec<&str> = inner_expr.split('/').collect();
                        if let (Ok(num), Ok(denom)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                            if denom == 0.0 {
                                return None; // Division by zero
                            }
                            Some((num / denom).floor())
                        } else {
                            return None; // Parse error
                        }
                    };

                    if let Some(value) = inner_value {
                        processed_expr = processed_expr.replacen(&format!("[{}]", inner_expr), &value.to_string(), 1);
                    } else {
                        return None; // Evaluation error
                    }
                } else {
                    return None; // Unmatched bracket
                }
            } else {
                break;
            }
            bracket_iterations += 1;
        }

        if bracket_iterations >= max_bracket_iterations && processed_expr.contains('[') {
            return None;
        }

        // Handle factorial
        while let Some(pos) = processed_expr.find('!') {
            if pos == 0 {
                return None;
            }

            // Find the number before !
            let mut start = pos - 1;
            while start > 0 && processed_expr.chars().nth(start - 1).unwrap().is_digit(10) {
                start -= 1;
            }

            let num_str = &processed_expr[start..pos];
            if let Ok(n) = num_str.parse::<i32>() {
                if n < 0 || n > 12 {
                    return None; // Too large or negative
                }

                let mut factorial = 1i32;
                for i in 2..=n {
                    factorial *= i;
                }

                processed_expr = processed_expr.replacen(&format!("{}!", num_str), &factorial.to_string(), 1);
            } else {
                return None; // Parse error
            }
        }

        // Handle permutation (A)
        while let Some(pos) = processed_expr.find('A') {
            if pos == 0 || pos == processed_expr.len() - 1 {
                return None;
            }

            // Find m in mAn
            let mut m_start = pos - 1;
            while m_start > 0 && processed_expr.chars().nth(m_start - 1).unwrap().is_digit(10) {
                m_start -= 1;
            }

            // Find n in mAn
            let mut n_end = pos + 1;
            while n_end < processed_expr.len() && processed_expr.chars().nth(n_end).unwrap().is_digit(10) {
                n_end += 1;
            }

            let m_str = &processed_expr[m_start..pos];
            let n_str = &processed_expr[pos+1..n_end];

            if let (Ok(m), Ok(n)) = (m_str.parse::<i32>(), n_str.parse::<i32>()) {
                if m < 0 || n < 0 || m > 10 || n > 10 || n > m {
                    return None; // Invalid values
                }

                let mut result = 1i32;
                for i in 0..n {
                    result *= (m - i);
                }

                processed_expr = processed_expr.replacen(&format!("{}A{}", m_str, n_str), &result.to_string(), 1);
            } else {
                return None; // Parse error
            }
        }

        // Replace ^ with ** for exponentiation
        // processed_expr = processed_expr.replace("^", "**");

        // Evaluate the expression
        self.evaluate_simple_expression(&processed_expr)
    }

    fn evaluate_simple_expression(&self, expr: &str) -> Option<f64> {
        // Check for invalid patterns
        if expr.contains("NaN") {
            return None;
        }

        // Check for numbers with leading zeros
        if expr.contains("0") && expr.matches(char::is_numeric).count() > 1 {
            let chars: Vec<char> = expr.chars().collect();
            for i in 0..chars.len() - 1 {
                if chars[i] == '0' && chars[i + 1].is_digit(10) && (i == 0 || !chars[i - 1].is_digit(10)) {
                    return None;
                }
            }
        }

        // Use the meval library to evaluate the expression
        match eval_str(expr) {
            Ok(result) => {
                // Check if the result is an integer
                if result.fract() == 0.0 {
                    Some(result)
                } else {
                    None
                }
            },
            Err(err) => {
                // If there's an error evaluating the expression, log it and return None
                // for debug
                // console_log!("Error evaluating expression '{}': {}", expr, err);
                None
            }
        }
    }

    pub fn is_valid_solution(&self, expression: &str) -> bool {
        self.is_valid_equation(expression)
    }

    fn is_valid_equation(&self, expression: &str) -> bool {
        if !self.check_brackets(expression) {
            return false;
        }

        // Find the main operator (= or >)
        let mut main_op = None;
        let mut main_op_index = 0;
        let mut depth = 0;

        for (i, c) in expression.chars().enumerate() {
            if c == '(' || c == '[' {
                depth += 1;
            } else if c == ')' || c == ']' {
                depth -= 1;
            } else if depth == 0 && (c == '=' || c == '>') {
                if main_op.is_none() {
                    main_op = Some(c);
                    main_op_index = i;
                } else if main_op == Some('>') && c == '=' {
                    main_op = Some('>'); // Combined >=
                } else if main_op != Some(c) {
                    return false; // Conflicting operators
                }
            }
        }

        if main_op.is_none() || main_op_index == 0 || main_op_index == expression.len() - 1 {
            return false;
        }

        // Split into left and right sides
        let left_side = &expression[0..main_op_index];
        let right_side = &expression[main_op_index + 1..];

        if left_side.is_empty() || right_side.is_empty() {
            return false;
        }

        // Evaluate both sides
        let left_value = self.evaluate_expression(left_side);
        let right_value = self.evaluate_expression(right_side);

        if left_value.is_none() || right_value.is_none() {
            return false;
        }

        let left_value = left_value.unwrap();
        let right_value = right_value.unwrap();

        // Check if the equation is valid
        match main_op {
            Some('=') => left_value == right_value,
            Some('>') => left_value > right_value,
            _ => false,
        }
    }

    fn check_brackets(&self, expression: &str) -> bool {
        let mut stack = Vec::new();

        for c in expression.chars() {
            match c {
                '(' => stack.push(')'),
                '[' => stack.push(']'),
                ')' | ']' => {
                    if stack.pop() != Some(c) {
                        return false;
                    }
                },
                _ => {}
            }
        }

        stack.is_empty()
    }

    // Helper functions for the search algorithm
    fn is_digit(&self, c: char) -> bool {
        c >= '0' && c <= '9'
    }

    fn is_binary_operator(&self, c: char) -> bool {
        matches!(c, '+' | '-' | '*' | '/' | '%' | '^' | 'A')
    }

    fn is_unary_post_operator(&self, c: char) -> bool {
        c == '!'
    }

    fn is_operator(&self, c: char) -> bool {
        self.is_binary_operator(c) || self.is_unary_post_operator(c)
    }

    fn is_open_bracket(&self, c: char) -> bool {
        c == '(' || c == '['
    }

    fn is_close_bracket(&self, c: char) -> bool {
        c == ')' || c == ']'
    }

    fn is_main_operator(&self, c: char) -> bool {
        c == '=' || c == '>'
    }

    fn get_matching_bracket(&self, open_bracket: char) -> Option<char> {
        match open_bracket {
            '(' => Some(')'),
            '[' => Some(']'),
            _ => None,
        }
    }

    // Preprocess constraints to initialize the GlobalKnowledge object
    fn preprocess_constraints(&self, constraints_json: &str) -> Result<GlobalKnowledge, String> {
        // Parse constraints from JSON
        let constraints: Constraints = match serde_json::from_str(constraints_json) {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to parse constraints: {}", e)),
        };

        // Initialize GlobalKnowledge
        let mut gk = GlobalKnowledge {
            fixed_chars: vec![None; self.length],
            cannot_be_at: vec![HashSet::new(); self.length],
            must_appear_min_count: HashMap::new(),
            must_appear_exact_count: HashMap::new(),
            globally_forbidden: HashSet::new(),
        };

        // Process each row of constraints
        for row in &constraints.rows {
            for (c, tile) in row.iter().enumerate() {
                if c >= self.length || tile.char.is_empty() {
                    continue;
                }

                let tile_char = tile.char.chars().next().unwrap();

                match tile.state.as_str() {
                    "correct" => {
                        if let Some(fixed) = gk.fixed_chars[c] {
                            if fixed != tile_char {
                                return Err(format!("Conflict: Position {} is fixed to both {} and {}", c + 1, fixed, tile_char));
                            }
                        }
                        gk.fixed_chars[c] = Some(tile_char);
                        for vc in self.valid_chars.chars() {
                            if vc != tile_char {
                                gk.cannot_be_at[c].insert(vc);
                            }
                        }
                    },
                    "present" => {
                        gk.cannot_be_at[c].insert(tile_char);
                    },
                    "empty" => {
                        gk.cannot_be_at[c].insert(tile_char);
                    },
                    _ => {}
                }
            }
        }

        // Collect all characters in guesses
        let mut all_chars_in_guesses = HashSet::new();
        for row in &constraints.rows {
            for tile in row {
                if !tile.char.is_empty() {
                    all_chars_in_guesses.insert(tile.char.chars().next().unwrap());
                }
            }
        }

        // Process character counts
        for &char in &all_chars_in_guesses {
            let mut min_required_overall = 0;
            let mut derived_exact_count = None;

            for row in &constraints.rows {
                if !row.iter().any(|tile| !tile.char.is_empty() && tile.char.chars().next().unwrap() == char) {
                    continue;
                }

                let mut green_in_row = 0;
                let mut yellow_in_row = 0;

                for tile in row {
                    if !tile.char.is_empty() && tile.char.chars().next().unwrap() == char {
                        match tile.state.as_str() {
                            "correct" => green_in_row += 1,
                            "present" => yellow_in_row += 1,
                            _ => {}
                        }
                    }
                }

                let min_required_this_row = green_in_row + yellow_in_row;
                min_required_overall = min_required_overall.max(min_required_this_row);

                if row.iter().any(|tile| !tile.char.is_empty() && tile.char.chars().next().unwrap() == char && tile.state == "empty") {
                    let exact_count_this_row = green_in_row + yellow_in_row;
                    if let Some(count) = derived_exact_count {
                        if count != exact_count_this_row {
                            return Err(format!("Conflict: Character '{}' has different exact counts in different rows ({} vs {})", char, count, exact_count_this_row));
                        }
                    } else {
                        derived_exact_count = Some(exact_count_this_row);
                    }
                }
            }

            gk.must_appear_min_count.insert(char, min_required_overall);

            if let Some(exact_count) = derived_exact_count {
                if exact_count < min_required_overall {
                    return Err(format!("Conflict: Character '{}' exact count ({}) is less than minimum required ({})", char, exact_count, min_required_overall));
                }
                gk.must_appear_exact_count.insert(char, exact_count);
                if exact_count == 0 && min_required_overall == 0 {
                    gk.globally_forbidden.insert(char);
                }
            }
        }

        // Check for conflicts
        for i in 0..self.length {
            if let Some(fixed) = gk.fixed_chars[i] {
                if gk.globally_forbidden.contains(&fixed) {
                    return Err(format!("Conflict: Character '{}' is fixed at position {} but also globally forbidden", fixed, i + 1));
                }
                if gk.cannot_be_at[i].contains(&fixed) {
                    return Err(format!("Conflict: Character '{}' is fixed at position {} but also marked as cannot be at that position", fixed, i + 1));
                }
                let min_count = *gk.must_appear_min_count.get(&fixed).unwrap_or(&0);
                gk.must_appear_min_count.insert(fixed, min_count.max(1));
                if let Some(&exact_count) = gk.must_appear_exact_count.get(&fixed) {
                    if exact_count < *gk.must_appear_min_count.get(&fixed).unwrap_or(&0) {
                        return Err(format!("Conflict: Character '{}' exact count ({}) is less than minimum fixed requirement", fixed, exact_count));
                    }
                }
            }
        }

        for (char, &exact) in &gk.must_appear_exact_count {
            let min = *gk.must_appear_min_count.get(char).unwrap_or(&0);
            if exact < min {
                return Err(format!("Conflict: Character '{}' exact count ({}) is less than minimum required ({})", char, exact, min));
            }
        }

        for &char in &gk.globally_forbidden {
            if *gk.must_appear_min_count.get(&char).unwrap_or(&0) > 0 {
                return Err(format!("Conflict: Character '{}' is globally forbidden but also required to appear", char));
            }
            if let Some(&count) = gk.must_appear_exact_count.get(&char) {
                if count > 0 {
                    return Err(format!("Conflict: Character '{}' is globally forbidden but also required to appear exactly {} times", char, count));
                }
            }
        }

        Ok(gk)
    }

    // Check if a character can be placed at a given position
    fn can_place_char(&self, 
                      char: char, 
                      index: usize, 
                      current_expression: &[char], 
                      main_op_so_far: Option<char>, 
                      current_expression_counts: &HashMap<char, usize>, 
                      floor_context: &FloorContext,
                      gk: &GlobalKnowledge) -> bool {
        // Check global constraints
        if gk.globally_forbidden.contains(&char) {
            return false;
        }
        if let Some(fixed) = gk.fixed_chars[index] {
            if fixed != char {
                return false;
            }
        }
        if gk.cannot_be_at[index].contains(&char) {
            return false;
        }

        // Check character count constraints
        let current_count = *current_expression_counts.get(&char).unwrap_or(&0);
        if let Some(&exact_count) = gk.must_appear_exact_count.get(&char) {
            if current_count >= exact_count {
                return false;
            }
        }

        // Check floor context constraints
        if floor_context.in_floor {
            if char == '[' {
                return false;
            }
            if self.is_operator(char) && char != '/' {
                return false;
            }
            if self.is_main_operator(char) {
                return false;
            }
            if char == '(' {
                return false;
            }
            if char == 'A' || char == '!' {
                return false;
            }

            if char == '/' {
                if floor_context.has_slash_in_current_floor {
                    return false;
                }
                let prev_char = if index > 0 { current_expression[index - 1] } else { '\0' };
                if !self.is_digit(prev_char) || index == 0 {
                    return false;
                }
            } else if char == ']' {
                let prev_char = if index > 0 { current_expression[index - 1] } else { '\0' };
                if !self.is_digit(prev_char) {
                    return false;
                }
                if !floor_context.has_slash_in_current_floor {
                    return false;
                }
            } else if !self.is_digit(char) {
                return false;
            }
        }

        // Check bracket context constraints
        if char == '[' && floor_context.in_floor {
            return false;
        }
        if char == ']' && !floor_context.in_floor {
            return false;
        }
        if char == '[' && index >= self.length - 3 {
            return false;
        }

        // Check number constraints
        if self.is_digit(char) && main_op_so_far != Some('=') {
            let mut temp_num_str = char.to_string();
            let mut k = index as isize - 1;
            while k >= 0 && self.is_digit(current_expression[k as usize]) {
                temp_num_str = format!("{}{}", current_expression[k as usize], temp_num_str);
                k -= 1;
            }

            if temp_num_str.len() > 1 && temp_num_str.starts_with('0') {
                return false;
            }

            let char_before_number_sequence = if k >= 0 { Some(current_expression[k as usize]) } else { None };
            if char_before_number_sequence.is_none() || 
               char_before_number_sequence.map_or(false, |c| self.is_operator(c) || self.is_open_bracket(c) || self.is_main_operator(c)) {
                if let Ok(num) = temp_num_str.parse::<i32>() {
                    if num > self.max_operand_value {
                        return false;
                    }
                }
            }
        }

        // Check syntax constraints
        let prev_char = if index > 0 { Some(current_expression[index - 1]) } else { None };

        if index == 0 {
            if self.is_binary_operator(char) || self.is_close_bracket(char) || self.is_main_operator(char) || self.is_unary_post_operator(char) {
                return false;
            }
        }

        if let Some(prev) = prev_char {
            if self.is_digit(prev) {
                if self.is_open_bracket(char) && char != '[' {
                    return false;
                }
                if char == '[' && floor_context.in_floor {
                    return false;
                }
            } else if self.is_operator(prev) {
                if self.is_binary_operator(char) && !(prev == 'A' && (self.is_open_bracket(char) || self.is_digit(char))) && !self.is_unary_post_operator(prev) {
                    return false;
                }
                if self.is_close_bracket(char) {
                    return false;
                }
                if self.is_main_operator(char) && !self.is_unary_post_operator(prev) {
                    return false;
                }
                if self.is_unary_post_operator(prev) && (self.is_digit(char) || self.is_open_bracket(char)) {
                    return false;
                }
            } else if self.is_open_bracket(prev) {
                if prev == '[' && char == '(' {
                    return false;
                }
                if self.is_binary_operator(char) {
                    return false;
                }
                if self.is_close_bracket(char) && self.get_matching_bracket(prev) != Some(char) {
                    return false;
                }
                if self.is_main_operator(char) {
                    return false;
                }
                if self.is_unary_post_operator(char) {
                    return false;
                }
            } else if self.is_close_bracket(prev) {
                if self.is_digit(char) {
                    return false;
                }
                if self.is_open_bracket(char) {
                    return false;
                }
            } else if self.is_main_operator(prev) {
                if prev == '=' {
                    if !self.is_digit(char) && char != '-' {
                        return false;
                    }
                } else {
                    if self.is_main_operator(char) {
                        return false;
                    }
                    if self.is_close_bracket(char) {
                        return false;
                    }
                }
            }
        }

        if main_op_so_far == Some('=') {
            if !self.is_digit(char) && char != '-' {
                return false;
            }
            if char == '-' {
                if prev_char != Some('=') || index >= self.length - 1 {
                    if prev_char != Some('=') {
                        // Standard operator rules apply
                    } else if index >= self.length - 1 {
                        return false; // - at the very end like ...=-
                    }
                }
            }
        }

        if index == self.length - 1 {
            if self.is_binary_operator(char) || self.is_open_bracket(char) || self.is_main_operator(char) {
                return false;
            }
        }

        // Check bracket matching
        let mut temp_expression = current_expression[0..index].to_vec();
        temp_expression.push(char);
        let mut open_paren_depth = 0;
        let mut open_square_depth = 0;
        let mut open_brackets_stack = Vec::new();

        for &c in &temp_expression {
            if c == '(' {
                open_paren_depth += 1;
                open_brackets_stack.push(c);
            } else if c == '[' {
                open_square_depth += 1;
                open_brackets_stack.push(c);
            } else if c == ')' {
                open_paren_depth -= 1;
                if open_paren_depth < 0 || open_brackets_stack.pop() != Some('(') {
                    return false;
                }
            } else if c == ']' {
                open_square_depth -= 1;
                if open_square_depth < 0 || open_brackets_stack.pop() != Some('[') {
                    return false;
                }
            }
        }

        if index == self.length - 1 && (open_paren_depth != 0 || open_square_depth != 0) {
            return false;
        }

        // Check main operator constraints
        if self.is_main_operator(char) {
            if let Some(main_op) = main_op_so_far {
                if main_op != char && !(main_op == '>' && char == '=') {
                    return false;
                }
                if main_op == char && char == '=' {
                    return false;
                }
            }
            if index == 0 || index >= self.length - 1 {
                return false;
            }
        }

        // Check special character constraints
        if char == 'A' {
            if prev_char.is_none() || prev_char.map_or(true, |c| !self.is_digit(c) && !self.is_close_bracket(c)) {
                return false;
            }
        }

        if prev_char == Some('A') {
            if !self.is_digit(char) && !self.is_open_bracket(char) {
                return false;
            }
        }

        if char == '!' {
            if prev_char.is_none() {
                return false;
            }
            if let Some(prev) = prev_char {
                if self.is_digit(prev) {
                    if prev == '0' && self.evaluate_expression("0!").is_none() {
                        return false;
                    }
                } else if self.is_close_bracket(prev) {
                    if prev == ']' {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }

    // Get the optimized order of characters to try at a given position
    fn get_optimized_char_order(&self, 
                               index: usize, 
                               current_expression: &[char], 
                               main_op_so_far: Option<char>, 
                               floor_context: &FloorContext,
                               gk: &GlobalKnowledge) -> Vec<char> {
        if let Some(fixed) = gk.fixed_chars[index] {
            return vec![fixed];
        }

        let mut ordered_chars = Vec::new();
        let prev_char = if index > 0 { current_expression[index - 1] } else { '\0' };

        if floor_context.in_floor {
            if floor_context.has_slash_in_current_floor {
                ordered_chars.extend("0123456789]".chars());
            } else {
                ordered_chars.extend("0123456789/".chars());
            }
        } else if main_op_so_far == Some('=') {
            if prev_char == '=' {
                ordered_chars.extend("-0123456789".chars());
            } else {
                ordered_chars.extend("0123456789".chars());
            }
        } else if index == 0 {
            ordered_chars.extend("123456789([".chars());
        } else if self.is_digit(prev_char) {
            ordered_chars.extend("0123456789+-*/%^A!)]=>[".chars());
        } else if self.is_binary_operator(prev_char) || prev_char == 'A' || (self.is_main_operator(prev_char) && prev_char != '=') {
            ordered_chars.extend("1234567890([".chars());
        } else if self.is_open_bracket(prev_char) {
            ordered_chars.extend("1234567890([".chars());
        } else if self.is_close_bracket(prev_char) || self.is_unary_post_operator(prev_char) {
            ordered_chars.extend("+-*/%^A!)]=>[".chars());
        } else {
            ordered_chars.extend("1234567890+-*/=()[]%^!A>".chars());
        }

        if index == self.length - 1 && !floor_context.in_floor {
            let end_chars: Vec<char> = "0123456789)]!".chars().collect();
            ordered_chars.retain(|c| end_chars.contains(c));
            if ordered_chars.is_empty() && prev_char != '\0' {
                ordered_chars = end_chars;
            } else if ordered_chars.is_empty() && index == 0 && self.length == 1 {
                ordered_chars.extend("0123456789".chars());
            }
        }

        // Remove duplicates and filter by constraints
        let mut unique_chars = Vec::new();
        for &c in ordered_chars.iter() {
            if !unique_chars.contains(&c) && 
               !gk.globally_forbidden.contains(&c) && 
               !gk.cannot_be_at[index].contains(&c) {
                unique_chars.push(c);
            }
        }

        unique_chars
    }

    // Recursive search function
    fn recursive_search(&self, 
                       index: usize, 
                       current_expression: &mut Vec<char>, 
                       main_op_so_far: Option<char>, 
                       current_expression_counts: &mut HashMap<char, usize>, 
                       floor_context: FloorContext,
                       gk: &GlobalKnowledge,
                       results: &mut Vec<String>,
                       searched_count: &mut usize) {
        // Check if we've reached the end of the expression
        if index == self.length {
            *searched_count += 1;

            // Check if the expression has a main operator
            if main_op_so_far.is_none() {
                return;
            }

            // Check if brackets are balanced
            let expr_str: String = current_expression.iter().collect();
            if !self.check_brackets(&expr_str) {
                return;
            }

            // Check character count constraints
            for (&char, &exact_count) in &gk.must_appear_exact_count {
                if current_expression_counts.get(&char).unwrap_or(&0) != &exact_count {
                    return;
                }
            }

            for (&char, &min_count) in &gk.must_appear_min_count {
                if !gk.must_appear_exact_count.contains_key(&char) {
                    if current_expression_counts.get(&char).unwrap_or(&0) < &min_count {
                        return;
                    }
                }
            }

            // Check if the expression is a valid solution
            if self.is_valid_solution(&expr_str) {
                results.push(expr_str);
            }

            return;
        }

        // Check if there's a fixed character for this position
        if let Some(fixed) = gk.fixed_chars[index] {
            let mut next_floor_context = floor_context;
            if fixed == '[' {
                next_floor_context = FloorContext { in_floor: true, has_slash_in_current_floor: false };
            } else if fixed == ']' && floor_context.in_floor {
                next_floor_context = FloorContext { in_floor: false, has_slash_in_current_floor: false };
            } else if fixed == '/' && floor_context.in_floor {
                next_floor_context = FloorContext { in_floor: true, has_slash_in_current_floor: true };
            }

            if self.can_place_char(fixed, index, current_expression, main_op_so_far, current_expression_counts, &floor_context, gk) {
                current_expression[index] = fixed;
                *current_expression_counts.entry(fixed).or_insert(0) += 1;

                let new_main_op = if self.is_main_operator(fixed) { Some(fixed) } else { main_op_so_far };

                self.recursive_search(index + 1, current_expression, new_main_op, current_expression_counts, next_floor_context, gk, results, searched_count);

                *current_expression_counts.get_mut(&fixed).unwrap() -= 1;
                if current_expression_counts[&fixed] == 0 {
                    current_expression_counts.remove(&fixed);
                }
            }
        } else {
            // Try each character in the optimized order
            let optimized_char_order = self.get_optimized_char_order(index, current_expression, main_op_so_far, &floor_context, gk);

            for &char_to_try in &optimized_char_order {
                let mut next_floor_context = floor_context;
                if char_to_try == '[' {
                    next_floor_context = FloorContext { in_floor: true, has_slash_in_current_floor: false };
                } else if char_to_try == ']' && floor_context.in_floor {
                    next_floor_context = FloorContext { in_floor: false, has_slash_in_current_floor: false };
                } else if char_to_try == '/' && floor_context.in_floor {
                    next_floor_context = FloorContext { in_floor: true, has_slash_in_current_floor: true };
                }

                if self.can_place_char(char_to_try, index, current_expression, main_op_so_far, current_expression_counts, &floor_context, gk) {
                    current_expression[index] = char_to_try;
                    *current_expression_counts.entry(char_to_try).or_insert(0) += 1;

                    let new_main_op = if self.is_main_operator(char_to_try) { Some(char_to_try) } else { main_op_so_far };

                    self.recursive_search(index + 1, current_expression, new_main_op, current_expression_counts, next_floor_context, gk, results, searched_count);

                    *current_expression_counts.get_mut(&char_to_try).unwrap() -= 1;
                    if current_expression_counts[&char_to_try] == 0 {
                        current_expression_counts.remove(&char_to_try);
                    }
                }
            }
        }
    }

    // Implementation of the search algorithm
    #[wasm_bindgen]
    pub fn search(&self, constraints_json: &str) -> JsValue {
        console_log!("Search called with constraints: {}", constraints_json);

        // Preprocess constraints
        let gk = match self.preprocess_constraints(constraints_json) {
            Ok(knowledge) => knowledge,
            Err(e) => {
                console_log!("Error preprocessing constraints: {}", e);
                return JsValue::from_serde(&Vec::<String>::new()).unwrap();
            }
        };

        // Initialize search
        let mut current_expression = vec!['\0'; self.length];
        let mut current_expression_counts = HashMap::new();
        let floor_context = FloorContext { in_floor: false, has_slash_in_current_floor: false };
        let mut results = Vec::new();
        let mut searched_count = 0;

        // Start recursive search
        self.recursive_search(0, &mut current_expression, None, &mut current_expression_counts, floor_context, &gk, &mut results, &mut searched_count);

        console_log!("Search completed. Found {} results. Searched {} expressions.", results.len(), searched_count);

        // Return results
        JsValue::from_serde(&results).unwrap()
    }
}
