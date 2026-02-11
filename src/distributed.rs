use crate::solver::Solver;
use crate::types::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const MAX_MSG_SIZE: usize = 16 * 1024 * 1024; // 16MB

/// 分布式协调器（运行在主节点）
pub struct Coordinator {
    bind_addr: String,
    constraints_data: ConstraintsData,
    tasks: Arc<Mutex<Vec<SearchTask>>>,
    results: Arc<Mutex<Vec<String>>>,
    max_solutions: usize,
}

impl Coordinator {
    pub fn new(
        bind_addr: String,
        constraints: &Constraints,
        prefixes: Vec<Vec<u8>>,
        max_solutions: usize,
    ) -> Self {
        let constraints_data = ConstraintsData::from(constraints);
        let tasks: Vec<SearchTask> = prefixes
            .into_iter()
            .enumerate()
            .map(|(id, prefix)| SearchTask {
                task_id: id as u64,
                prefix,
                constraints_data: constraints_data.clone(),
            })
            .collect();

        Self {
            bind_addr,
            constraints_data,
            tasks: Arc::new(Mutex::new(tasks)),
            results: Arc::new(Mutex::new(Vec::new())),
            max_solutions,
        }
    }

    pub async fn run(&self) -> Vec<String> {
        let listener = TcpListener::bind(&self.bind_addr)
            .await
            .expect("Failed to bind coordinator");
        log::info!("Coordinator listening on {}", self.bind_addr);

        let tasks = self.tasks.clone();
        let results = self.results.clone();
        let max_solutions = self.max_solutions;

        loop {
            // 检查是否所有任务完成或达到上限
            {
                let r = results.lock().await;
                if r.len() >= max_solutions {
                    log::info!("Reached max solutions, stopping");
                    break;
                }
                let t = tasks.lock().await;
                if t.is_empty() {
                    // 检查是否还有正在执行的任务（简化：等一下）
                    // 实际中需要更精确的跟踪
                    drop(t);
                    drop(r);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    let t2 = tasks.lock().await;
                    if t2.is_empty() {
                        log::info!("All tasks distributed");
                        // 等待剩余结果
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        break;
                    }
                }
            }

            tokio::select! {
                Ok((socket, addr)) = listener.accept() => {
                    log::info!("Worker connected from {}", addr);
                    let tasks = tasks.clone();
                    let results = results.clone();
                    tokio::spawn(async move {
                        handle_worker(socket, tasks, results, max_solutions).await;
                    });
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                    // 定期检查
                }
            }
        }

        let results = results.lock().await;
        let mut solutions = results.clone();
        solutions.truncate(max_solutions);
        solutions
    }
}

async fn handle_worker(
    mut socket: TcpStream,
    tasks: Arc<Mutex<Vec<SearchTask>>>,
    results: Arc<Mutex<Vec<String>>>,
    max_solutions: usize,
) {
    loop {
        // 读取工作节点请求
        match recv_message(&mut socket).await {
            Ok(Message::RequestTask) => {
                // 检查是否应该停止
                {
                    let r = results.lock().await;
                    if r.len() >= max_solutions {
                        let _ = send_message(&mut socket, &Message::Shutdown).await;
                        return;
                    }
                }

                // 分配任务
                let task = {
                    let mut t = tasks.lock().await;
                    t.pop()
                };

                match task {
                    Some(task) => {
                        log::debug!("Assigning task {}", task.task_id);
                        let _ = send_message(&mut socket, &Message::AssignTask(task)).await;
                    }
                    None => {
                        let _ = send_message(&mut socket, &Message::Shutdown).await;
                        return;
                    }
                }
            }
            Ok(Message::TaskResult(result)) => {
                log::info!(
                    "Task {} returned {} solutions",
                    result.task_id,
                    result.solutions.len()
                );
                let mut r = results.lock().await;
                r.extend(result.solutions);
            }
            Ok(_) => {
                log::warn!("Unexpected message from worker");
            }
            Err(e) => {
                log::error!("Error reading from worker: {}", e);
                return;
            }
        }
    }
}

/// 分布式工作节点
pub struct Worker {
    coordinator_addr: String,
    num_threads: usize,
}

impl Worker {
    pub fn new(coordinator_addr: String, num_threads: usize) -> Self {
        Self {
            coordinator_addr,
            num_threads,
        }
    }

    pub async fn run(&self) {
        let mut socket = TcpStream::connect(&self.coordinator_addr)
            .await
            .expect("Failed to connect to coordinator");
        log::info!("Connected to coordinator at {}", self.coordinator_addr);

        loop {
            // 请求任务
            if let Err(e) = send_message(&mut socket, &Message::RequestTask).await {
                log::error!("Failed to request task: {}", e);
                break;
            }

            // 接收任务或关机命令
            match recv_message(&mut socket).await {
                Ok(Message::AssignTask(task)) => {
                    log::info!("Received task {}", task.task_id);

                    let constraints = Constraints::from(&task.constraints_data);
                    let prefix = task.prefix.clone();
                    let task_id = task.task_id;

                    // 使用本地多线程求解
                    let solutions = self.solve_task(&constraints, &prefix).await;

                    let result = SearchResult {
                        task_id,
                        solutions,
                    };

                    if let Err(e) = send_message(&mut socket, &Message::TaskResult(result)).await {
                        log::error!("Failed to send result: {}", e);
                        break;
                    }
                }
                Ok(Message::Shutdown) => {
                    log::info!("Received shutdown, exiting");
                    break;
                }
                Ok(msg) => {
                    log::warn!("Unexpected message: {:?}", msg);
                }
                Err(e) => {
                    log::error!("Error receiving: {}", e);
                    break;
                }
            }
        }
    }

    async fn solve_task(&self, constraints: &Constraints, prefix: &[u8]) -> Vec<String> {
        let constraints = constraints.clone();
        let prefix = prefix.to_vec();
        let num_threads = self.num_threads;

        // 在阻塞线程中运行CPU密集计算
        tokio::task::spawn_blocking(move || {
            // 可以进一步分割任务用 rayon
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .unwrap();

            pool.install(|| {
                let mut solver = Solver::new(constraints, 10000);
                solver.solve_with_prefix(&prefix);
                solver.solutions
            })
        })
        .await
        .unwrap_or_default()
    }
}

// ===== 网络通信辅助 =====

async fn send_message(socket: &mut TcpStream, msg: &Message) -> Result<(), String> {
    let data = bincode::serialize(msg).map_err(|e| format!("Serialize error: {}", e))?;
    let len = data.len() as u32;
    socket
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| format!("Write len error: {}", e))?;
    socket
        .write_all(&data)
        .await
        .map_err(|e| format!("Write data error: {}", e))?;
    socket
        .flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

async fn recv_message(socket: &mut TcpStream) -> Result<Message, String> {
    let mut len_buf = [0u8; 4];
    socket
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("Read len error: {}", e))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_MSG_SIZE {
        return Err(format!("Message too large: {} bytes", len));
    }

    let mut data = vec![0u8; len];
    socket
        .read_exact(&mut data)
        .await
        .map_err(|e| format!("Read data error: {}", e))?;

    bincode::deserialize(&data).map_err(|e| format!("Deserialize error: {}", e))
}
