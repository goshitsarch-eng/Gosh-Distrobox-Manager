use crate::backends::Distrobox;
use crate::fakers::CommandRunner;
use crate::models::Task;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

pub struct AppState {
    pub distrobox: Distrobox,
    pub tasks: Arc<RwLock<HashMap<String, Task>>>,
}

impl AppState {
    pub fn new() -> Self {
        let runner = CommandRunner::new_real();

        let runner = if Path::new("/.flatpak-info").exists() {
            runner.map_cmd(crate::backends::flatpak::map_flatpak_spawn_host)
        } else {
            runner
        };

        let distrobox = Distrobox::new(
            runner,
            crate::backends::distrobox::command::default_cmd_factory(),
        );

        Self {
            distrobox,
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
