use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// 字符集: 0123456789+-*/%^=()![]>A
pub const CHARSET: &[u8] = b"0123456789+-*/%^=()![]>A";
pub const DIGITS: &[u8] = b"0123456789";
pub const OPERATORS: &[u8] = b"+-*/%^";
pub const RELATION_OPS: &[u8] = b"=>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellColor {
    Green,
    Yellow,
    Gray,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellGuess {
    pub ch: char,
    pub color: CellColor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuessRow {
    pub cells: Vec<CellGuess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuzzleInput {
    pub length: usize,
    pub rows: Vec<GuessRow>,
}

#[derive(Debug, Clone)]
pub struct Constraints {
    pub length: usize,
    /// 某位置已确定的字符
    pub fixed_chars: HashMap<usize, u8>,
    /// 某位置不能放的字符集
    pub cannot_be_at: HashMap<usize, HashSet<u8>>,
    /// 某字符最少出现次数
    pub must_appear_min_count: HashMap<u8, usize>,
    /// 某字符精确出现次数（若已知）
    pub must_appear_exact_count: HashMap<u8, usize>,
    /// 全局禁用字符
    pub globally_forbidden: HashSet<u8>,
}

impl Constraints {
    pub fn new(length: usize) -> Self {
        Self {
            length,
            fixed_chars: HashMap::new(),
            cannot_be_at: HashMap::new(),
            must_appear_min_count: HashMap::new(),
            must_appear_exact_count: HashMap::new(),
            globally_forbidden: HashSet::new(),
        }
    }
}

/// 搜索任务：指定前缀范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTask {
    pub task_id: u64,
    /// 前缀字符列表
    pub prefix: Vec<u8>,
    /// 约束数据（序列化）
    pub constraints_data: ConstraintsData,
}

/// 可序列化的约束（用于网络传输）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintsData {
    pub length: usize,
    pub fixed_chars: Vec<(usize, u8)>,
    pub cannot_be_at: Vec<(usize, Vec<u8>)>,
    pub must_appear_min_count: Vec<(u8, usize)>,
    pub must_appear_exact_count: Vec<(u8, usize)>,
    pub globally_forbidden: Vec<u8>,
}

impl From<&Constraints> for ConstraintsData {
    fn from(c: &Constraints) -> Self {
        ConstraintsData {
            length: c.length,
            fixed_chars: c.fixed_chars.iter().map(|(&k, &v)| (k, v)).collect(),
            cannot_be_at: c
                .cannot_be_at
                .iter()
                .map(|(&k, v)| (k, v.iter().cloned().collect()))
                .collect(),
            must_appear_min_count: c
                .must_appear_min_count
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
            must_appear_exact_count: c
                .must_appear_exact_count
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
            globally_forbidden: c.globally_forbidden.iter().cloned().collect(),
        }
    }
}

impl From<&ConstraintsData> for Constraints {
    fn from(cd: &ConstraintsData) -> Self {
        let mut c = Constraints::new(cd.length);
        for &(pos, ch) in &cd.fixed_chars {
            c.fixed_chars.insert(pos, ch);
        }
        for (pos, chars) in &cd.cannot_be_at {
            c.cannot_be_at
                .insert(*pos, chars.iter().cloned().collect());
        }
        for &(ch, count) in &cd.must_appear_min_count {
            c.must_appear_min_count.insert(ch, count);
        }
        for &(ch, count) in &cd.must_appear_exact_count {
            c.must_appear_exact_count.insert(ch, count);
        }
        c.globally_forbidden = cd.globally_forbidden.iter().cloned().collect();
        c
    }
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub task_id: u64,
    pub solutions: Vec<String>,
}

/// 分布式消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// 协调器 -> 工作节点: 分发任务
    AssignTask(SearchTask),
    /// 工作节点 -> 协调器: 返回结果
    TaskResult(SearchResult),
    /// 协调器 -> 工作节点: 无更多任务
    Shutdown,
    /// 工作节点 -> 协调器: 请求任务
    RequestTask,
}

impl fmt::Display for Constraints {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Constraints (length={}) ===", self.length)?;
        if !self.fixed_chars.is_empty() {
            write!(f, "  Fixed: ")?;
            let mut sorted: Vec<_> = self.fixed_chars.iter().collect();
            sorted.sort_by_key(|(&pos, _)| pos);
            for (&pos, &ch) in &sorted {
                write!(f, "[{}]='{}' ", pos, ch as char)?;
            }
            writeln!(f)?;
        }
        if !self.globally_forbidden.is_empty() {
            write!(f, "  Forbidden globally: ")?;
            for &ch in &self.globally_forbidden {
                write!(f, "'{}' ", ch as char)?;
            }
            writeln!(f)?;
        }
        if !self.must_appear_min_count.is_empty() {
            write!(f, "  Min counts: ")?;
            for (&ch, &cnt) in &self.must_appear_min_count {
                write!(f, "'{}'>={} ", ch as char, cnt)?;
            }
            writeln!(f)?;
        }
        if !self.must_appear_exact_count.is_empty() {
            write!(f, "  Exact counts: ")?;
            for (&ch, &cnt) in &self.must_appear_exact_count {
                write!(f, "'{}' = {} ", ch as char, cnt)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
