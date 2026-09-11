use crate::backends::Distrobox;
use crate::backends::host_exec::{
    has_distrobox_host_exec, is_distrobox_container, map_distrobox_host_exec,
};
use crate::fakers::CommandRunner;
use crate::task_runtime::{TaskRegistry, new_registry};
use std::path::Path;

pub struct AppState {
    pub distrobox: Distrobox,
    pub tasks: TaskRegistry,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let runner = CommandRunner::new_real();

        let runner = if Path::new("/.flatpak-info").exists() {
            runner.map_cmd(crate::backends::flatpak::map_flatpak_spawn_host)
        } else if is_distrobox_container() && has_distrobox_host_exec() {
            runner.map_cmd(map_distrobox_host_exec)
        } else {
            runner
        };

        let distrobox = Distrobox::new(
            runner,
            crate::backends::distrobox::command::default_cmd_factory(),
        );

        Self {
            distrobox,
            tasks: new_registry(),
        }
    }
}
