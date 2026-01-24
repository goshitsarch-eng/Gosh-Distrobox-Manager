use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub struct Task {
    pub id: String,
    pub label: String,
    pub tx: broadcast::Sender<String>,
    pub handle: JoinHandle<anyhow::Result<()>>,
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
            tx,
            handle,
        }
    }
}
