// A container runtime is docker/podman/etc.

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;
use tracing::info;

use super::docker::Docker;

use crate::{
    backends::podman::Podman,
    fakers::{Command, CommandRunner},
};

/// Which of the two container runtimes a command is aimed at
/// (architecture.md §6.4, row B7).
///
/// The fallback blocks in `distrobox.rs` were written out longhand at every
/// call site and had drifted — different triggers, different argv (by id vs
/// by name), different trimming — so this names the two runtimes once and
/// lets one loop in `Distrobox` drive all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Podman,
    Docker,
}

impl Runtime {
    /// The program name to exec.
    pub fn program(self) -> &'static str {
        match self {
            Runtime::Podman => "podman",
            Runtime::Docker => "docker",
        }
    }
}

/// Order the fallback tries: podman first, because it is rootless by default
/// (`get_container_runtime` below makes the same choice).
pub const PODMAN_FIRST: [Runtime; 2] = [Runtime::Podman, Runtime::Docker];

/// The same command aimed at `to` instead of `from`: the program is swapped
/// and the arguments and stdio modes are carried over untouched.
///
/// This is not `podman::map_docker_to_podman`, and the difference is not
/// stylistic: that helper maps the program field **one way**, `docker →
/// podman`, so it can never *produce* a docker retry — it can only undo one.
/// (`Podman::new` installs it so a `Docker` backend's commands reach the podman
/// runner, which is why routing this retry through a `Podman`-constructed
/// runner would silently rewrite it straight back.) Choosing a target is what
/// `retarget` is for.
pub fn retarget(cmd: &Command, to: Runtime) -> Command {
    let mut out = cmd.clone();
    out.program = to.program().into();
    out
}

#[async_trait(?Send)]
pub trait ContainerRuntime {
    fn name(&self) -> &'static str;
    async fn version(&self) -> anyhow::Result<String>;
    async fn usage(&self, container_id: &str) -> anyhow::Result<Usage>;
    async fn downloaded_images(&self) -> anyhow::Result<HashSet<String>>;
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    #[serde(rename = "mem_usage", alias = "MemUsage")]
    pub mem_usage: String,
    #[serde(rename = "mem_percent", alias = "MemPerc")]
    pub mem_perc: String,
    #[serde(rename = "cpu_percent", alias = "CPU")]
    pub cpu_perc: String,
    #[serde(rename = "net_io", alias = "NetIO")]
    pub net_io: String,
    #[serde(rename = "block_io", alias = "BlockIO")]
    pub block_io: String,
    #[serde(rename = "pids", alias = "PIDs")]
    pub pids: String,
}

pub async fn get_container_runtime(
    command_runner: CommandRunner,
) -> Option<Arc<dyn ContainerRuntime>> {
    let runner = Arc::new(command_runner);
    // Prefer Podman when both are available because Podman is rootless by default
    let podman = Podman::new(runner.clone());
    if let Err(podman_err) = podman.version().await {
        let docker = Docker::new(runner);
        if let Err(docker_err) = docker.version().await {
            info!(docker = ?docker_err, podman = ?podman_err, "Container runtime check results");
            None
        } else {
            Some(Arc::new(docker) as Arc<dyn ContainerRuntime>)
        }
    } else {
        Some(Arc::new(podman) as Arc<dyn ContainerRuntime>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::Command;

    #[test]
    fn runtime_program_names_are_the_executables() {
        assert_eq!(Runtime::Podman.program(), "podman");
        assert_eq!(Runtime::Docker.program(), "docker");
    }

    #[test]
    fn podman_first_puts_podman_ahead_of_docker() {
        assert_eq!(PODMAN_FIRST, [Runtime::Podman, Runtime::Docker]);
    }

    /// `retarget` swaps the program and nothing else — arguments, order and
    /// stdio modes all ride along.
    #[test]
    fn retarget_swaps_only_the_program() {
        let cmd = Command::new_with_args("podman", ["start", "ubuntu"]);
        let docker = retarget(&cmd, Runtime::Docker);
        assert_eq!(docker.program.to_string_lossy(), "docker");
        assert_eq!(
            docker
                .args
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["start", "ubuntu"]
        );
        // The source command is untouched (retarget clones).
        assert_eq!(cmd.program.to_string_lossy(), "podman");
    }

    /// Aiming at the runtime the command already uses is a no-op.
    #[test]
    fn retarget_to_the_same_runtime_is_a_no_op() {
        let cmd = Command::new_with_args("docker", ["images"]);
        assert_eq!(
            retarget(&cmd, Runtime::Docker).program.to_string_lossy(),
            "docker"
        );
    }

    /// A container that happens to be named after the other runtime is an
    /// argument, not a program — it must survive the swap intact. (`retarget`
    /// only ever assigns `Command::program`; this pins that it does not grow a
    /// textual pass over the argv later.)
    #[test]
    fn retarget_does_not_rewrite_arguments_that_look_like_programs() {
        let cmd = Command::new_with_args("podman", ["start", "docker"]);
        let docker = retarget(&cmd, Runtime::Docker);
        assert_eq!(
            docker
                .args
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["start", "docker"]
        );
    }
}
