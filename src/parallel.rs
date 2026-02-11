use crate::constraints::build_constraints;
use crate::solver::{generate_prefixes, Solver};
use crate::types::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 多线程并行求解器（单机多核）
pub struct ParallelSolver {
    constraints: Constraints,
    max_solutions: usize,
    num_threads: usize,
}

impl ParallelSolver {
    pub fn new(constraints: Constraints, max_solutions: usize, num_threads: Option<usize>) -> Self {
        let num_threads = num_threads.unwrap_or_else(num_cpus::get);
        Self {
            constraints,
            max_solutions,
            num_threads,
        }
    }

    /// 基于 Rayon 的并行求解
    pub fn solve_rayon(&self) -> Vec<String> {
        let start = Instant::now();

        // 确定前缀深度：目标是任务数 >> 线程数
        let prefix_len = self.determine_prefix_len();
        log::info!(
            "Using prefix length {} with {} threads",
            prefix_len,
            self.num_threads
        );

        let prefixes = generate_prefixes(&self.constraints, prefix_len);
        log::info!("Generated {} prefix tasks", prefixes.len());

        if prefixes.is_empty() {
            log::warn!("No valid prefixes generated!");
            return Vec::new();
        }

        // 配置 Rayon 线程池
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.num_threads)
            .build()
            .expect("Failed to build thread pool");

        let found_count = Arc::new(AtomicUsize::new(0));
        let should_stop = Arc::new(AtomicBool::new(false));
        let max_sol = self.max_solutions;
        let constraints = self.constraints.clone();

        let all_solutions: Vec<String> = pool.install(|| {
            prefixes
                .par_iter()
                .flat_map(|prefix| {
                    if should_stop.load(Ordering::Relaxed) {
                        return Vec::new();
                    }

                    let mut solver = Solver::new(constraints.clone(), max_sol);
                    solver.solve_with_prefix(prefix);

                    let count = found_count.fetch_add(solver.solutions.len(), Ordering::Relaxed);
                    if count + solver.solutions.len() >= max_sol {
                        should_stop.store(true, Ordering::Relaxed);
                    }

                    solver.solutions
                })
                .collect()
        });

        let elapsed = start.elapsed();
        log::info!(
            "Found {} solutions in {:.2}s",
            all_solutions.len(),
            elapsed.as_secs_f64()
        );

        // 截断到 max_solutions
        let mut results = all_solutions;
        results.truncate(self.max_solutions);
        results
    }

    /// 基于 Channel + 工作线程的并行求解（更精细控制）
    pub fn solve_channel(&self) -> Vec<String> {
        let start = Instant::now();
        let prefix_len = self.determine_prefix_len();
        let prefixes = generate_prefixes(&self.constraints, prefix_len);

        log::info!(
            "Channel mode: {} tasks, {} threads",
            prefixes.len(),
            self.num_threads
        );

        let (task_tx, task_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(prefixes.len());
        let (result_tx, result_rx): (Sender<Vec<String>>, Receiver<Vec<String>>) =
            bounded(self.num_threads * 2);
        let should_stop = Arc::new(AtomicBool::new(false));

        // 发送所有任务
        for prefix in prefixes {
            task_tx.send(prefix).unwrap();
        }
        drop(task_tx); // 关闭发送端，工作线程收完后退出

        // 启动工作线程
        let mut workers = Vec::new();
        for worker_id in 0..self.num_threads {
            let rx = task_rx.clone();
            let tx = result_tx.clone();
            let constraints = self.constraints.clone();
            let stop_flag = should_stop.clone();
            let max_sol = self.max_solutions;

            let handle = std::thread::spawn(move || {
                let mut total_found = 0;
                while let Ok(prefix) = rx.recv() {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    let mut solver = Solver::new(constraints.clone(), max_sol);
                    solver.solve_with_prefix(&prefix);

                    if !solver.solutions.is_empty() {
                        total_found += solver.solutions.len();
                        if tx.send(solver.solutions).is_err() {
                            break;
                        }
                    }
                }
                log::debug!("Worker {} finished, found {} solutions", worker_id, total_found);
            });
            workers.push(handle);
        }
        drop(task_rx);
        drop(result_tx);

        // 收集结果
        let mut all_solutions = Vec::new();
        while let Ok(solutions) = result_rx.recv() {
            all_solutions.extend(solutions);
            if all_solutions.len() >= self.max_solutions {
                should_stop.store(true, Ordering::Relaxed);
                break;
            }
        }

        // 等待所有工作线程完成
        for handle in workers {
            let _ = handle.join();
        }

        let elapsed = start.elapsed();
        log::info!(
            "Found {} solutions in {:.2}s",
            all_solutions.len(),
            elapsed.as_secs_f64()
        );

        all_solutions.truncate(self.max_solutions);
        all_solutions
    }

    /// 基于 work-stealing 的并行求解
    pub fn solve_work_stealing(&self) -> Vec<String> {
        let start = Instant::now();
        let prefix_len = self.determine_prefix_len();
        let prefixes = generate_prefixes(&self.constraints, prefix_len);

        log::info!(
            "Work-stealing mode: {} initial tasks, {} threads",
            prefixes.len(),
            self.num_threads
        );

        let task_queue = Arc::new(crossbeam::queue::SegQueue::new());
        for prefix in prefixes {
            task_queue.push(prefix);
        }

        let all_solutions = Arc::new(dashmap::DashMap::<usize, String>::new());
        let solution_count = Arc::new(AtomicUsize::new(0));
        let should_stop = Arc::new(AtomicBool::new(false));

        let mut handles = Vec::new();
        for worker_id in 0..self.num_threads {
            let queue = task_queue.clone();
            let solutions = all_solutions.clone();
            let count = solution_count.clone();
            let stop = should_stop.clone();
            let constraints = self.constraints.clone();
            let max_sol = self.max_solutions;

            let handle = std::thread::spawn(move || {
                while let Some(prefix) = queue.pop() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }

                    let mut solver = Solver::new(constraints.clone(), max_sol);
                    solver.solve_with_prefix(&prefix);

                    for sol in solver.solutions {
                        let idx = count.fetch_add(1, Ordering::Relaxed);
                        if idx >= max_sol {
                            stop.store(true, Ordering::Relaxed);
                            break;
                        }
                        solutions.insert(idx, sol);
                    }
                }
                log::debug!("Worker {} finished", worker_id);
            });
            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.join();
        }

        let mut result: Vec<String> = all_solutions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        result.truncate(self.max_solutions);

        let elapsed = start.elapsed();
        log::info!(
            "Found {} solutions in {:.2}s",
            result.len(),
            elapsed.as_secs_f64()
        );

        result
    }

    fn determine_prefix_len(&self) -> usize {
        let length = self.constraints.length;
        let fixed_count = self.constraints.fixed_chars.len();
        let free_positions = length - fixed_count;

        // 目标：任务数 ≈ 线程数 * 8 ~ 64
        // 每个自由位置约有 ~15 个可选字符（扣除禁用后）
        // prefix_len 个自由位置大约产生 15^prefix_len 个前缀
        let target_tasks = self.num_threads * 16;

        let available_chars = CHARSET.len() - self.constraints.globally_forbidden.len();
        let mut prefix_len = 1;
        let mut estimated_tasks = 1usize;

        for pos in 0..length {
            if prefix_len >= length.min(4) {
                break;
            }
            if self.constraints.fixed_chars.contains_key(&pos) {
                prefix_len += 1; // 固定位置不增加任务数
                continue;
            }
            estimated_tasks = estimated_tasks.saturating_mul(available_chars);
            prefix_len += 1;
            if estimated_tasks >= target_tasks {
                break;
            }
        }

        prefix_len.min(length.saturating_sub(1)).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_rayon() {
        let mut constraints = Constraints::new(5);
        constraints.fixed_chars.insert(1, b'+');
        constraints.fixed_chars.insert(3, b'=');

        let solver = ParallelSolver::new(constraints, 20, Some(4));
        let solutions = solver.solve_rayon();
        assert!(!solutions.is_empty());
        for sol in &solutions {
            println!("Parallel found: {}", sol);
        }
    }

    #[test]
    fn test_parallel_channel() {
        let mut constraints = Constraints::new(5);
        constraints.fixed_chars.insert(1, b'+');
        constraints.fixed_chars.insert(3, b'=');

        let solver = ParallelSolver::new(constraints, 20, Some(4));
        let solutions = solver.solve_channel();
        assert!(!solutions.is_empty());
    }

    #[test]
    fn test_parallel_work_stealing() {
        let mut constraints = Constraints::new(5);
        constraints.fixed_chars.insert(1, b'+');
        constraints.fixed_chars.insert(3, b'=');

        let solver = ParallelSolver::new(constraints, 20, Some(4));
        let solutions = solver.solve_work_stealing();
        assert!(!solutions.is_empty());
    }
}
