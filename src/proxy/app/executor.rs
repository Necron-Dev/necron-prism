use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use tracing::warn;

use super::lifecycle::run_connection;
use super::state::AppState;
use crate::proxy::transport::ConnectionContext;

pub struct ConnectionExecutor {
    sender: Sender<ConnectionTask>,
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
    shared: Arc<SharedExecutor>,
}

impl ConnectionExecutor {
    pub fn new(state: AppState) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            workers: Mutex::new(Vec::new()),
            shared: Arc::new(SharedExecutor::new(state, receiver)),
        }
    }

    pub fn submit(&self, stream: TcpStream, context: ConnectionContext) {
        self.spawn_worker_if_needed();

        if let Err(error) = self.sender.send(ConnectionTask { stream, context }) {
            warn!(
                error = %error,
                connection_id = error.0.context.id,
                "failed to queue connection task"
            );
        }
    }

    fn spawn_worker_if_needed(&self) {
        let mut workers = self.workers.lock().expect("connection executor poisoned");
        if workers.len() >= self.shared.worker_limit {
            return;
        }

        let index = workers.len();
        workers.push(self.shared.spawn_worker(index));
    }
}

struct SharedExecutor {
    state: AppState,
    receiver: Mutex<Receiver<ConnectionTask>>,
    worker_limit: usize,
}

impl SharedExecutor {
    fn new(state: AppState, receiver: Receiver<ConnectionTask>) -> Self {
        Self {
            state,
            receiver: Mutex::new(receiver),
            worker_limit: default_worker_count(),
        }
    }

    fn spawn_worker(self: &Arc<Self>, index: usize) -> thread::JoinHandle<()> {
        let shared = Arc::clone(self);
        thread::Builder::new()
            .name(format!("conn-worker-{index}"))
            .stack_size(worker_stack_size())
            .spawn(move || shared.run_worker())
            .expect("failed to spawn connection worker")
    }

    fn run_worker(&self) {
        loop {
            let task = self
                .receiver
                .lock()
                .expect("connection executor poisoned")
                .recv();

            match task {
                Ok(task) => run_connection(self.state.clone(), task.stream, task.context),
                Err(_) => break,
            }
        }
    }
}

struct ConnectionTask {
    stream: TcpStream,
    context: ConnectionContext,
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| if parallelism.get() <= 2 { 2 } else { 3 })
        .unwrap_or(2)
}

fn worker_stack_size() -> usize {
    128 * 1024
}
