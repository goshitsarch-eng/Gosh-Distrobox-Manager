use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use std::time::Instant;

pub struct Task {
    pub id: String,
    pub label: String,
    pub tx: Option<broadcast::Sender<String>>,
    pub handle: JoinHandle<anyhow::Result<()>>,
    pub output: Vec<String>,
    pub completed: bool,
    pub success: bool,
    pub completed_at: Option<Instant>,
}

impl Task {
    pub fn new(
        id: String,
        label: String,
        handle: JoinHandle<anyhow::Result<()>>,
        tx: broadcast::Sender<String>,
    ) -> Self {
        Self {
            id,
            label,
            tx: Some(tx),
            handle,
            output: Vec::new(),
            completed: false,
            success: false,
            completed_at: None,
        }
    }

    pub fn push_output(&mut self, line: String, max_lines: usize) {
        self.output.push(line);
        if self.output.len() > max_lines {
            let drain_to = self.output.len() - max_lines;
            self.output.drain(0..drain_to);
        }
    }
}
