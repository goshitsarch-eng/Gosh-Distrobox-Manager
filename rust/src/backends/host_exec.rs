use crate::fakers::Command;
use std::env;
use std::path::Path;

pub fn map_distrobox_host_exec(mut command: Command) -> Command {
    let mut args = Vec::with_capacity(1 + command.args.len());
    args.push(command.program);
    args.extend(command.args);

    command.program = "distrobox-host-exec".into();
    command.args = args;
    command
}

pub fn is_distrobox_container() -> bool {
    env::var("DISTROBOX_ENTERED").is_ok()
        || env::var("DISTROBOX_CONTAINER_NAME").is_ok()
        || env::var("DISTROBOX_HOST_HOME").is_ok()
}

pub fn has_distrobox_host_exec() -> bool {
    [
        "/usr/bin/distrobox-host-exec",
        "/usr/local/bin/distrobox-host-exec",
        "/bin/distrobox-host-exec",
    ]
    .iter()
    .any(|path| Path::new(path).exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::FdMode;

    #[test]
    fn map_distrobox_host_exec_simple() {
        let cmd = Command::new("distrobox");
        let mapped = map_distrobox_host_exec(cmd);

        assert_eq!(mapped.program.to_string_lossy(), "distrobox-host-exec");
        assert_eq!(mapped.args.len(), 1);
        assert_eq!(mapped.args[0].to_string_lossy(), "distrobox");
    }

    #[test]
    fn map_distrobox_host_exec_with_args() {
        let mut cmd = Command::new("distrobox");
        cmd.args(["create", "--compatibility"]);
        let mapped = map_distrobox_host_exec(cmd);

        assert_eq!(mapped.program.to_string_lossy(), "distrobox-host-exec");
        assert_eq!(mapped.args.len(), 3);
        assert_eq!(mapped.args[0].to_string_lossy(), "distrobox");
        assert_eq!(mapped.args[1].to_string_lossy(), "create");
        assert_eq!(mapped.args[2].to_string_lossy(), "--compatibility");
    }

    #[test]
    fn map_distrobox_host_exec_preserves_fd_modes() {
        let mut cmd = Command::new("echo");
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        let mapped = map_distrobox_host_exec(cmd);
        matches!(mapped.stdout, FdMode::Pipe);
        matches!(mapped.stderr, FdMode::Pipe);
    }
}
