use std::{
    collections::HashSet,
    sync::{Arc, LazyLock, Mutex},
};

use tracing::{error, info};

use crate::fakers::{Command, CommandRunner, FdMode};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Terminal {
    pub name: String,
    pub program: String,
    /// Arguments that come after the program but before the separator_arg.
    /// For example, for flatpak terminals: ["run", "org.gnome.Console"]
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub separator_arg: String,
    pub read_only: bool,
}

impl Terminal {
    /// Build `program [extra_args…] separator_arg <enter argv>` and spawn it
    /// through the RUNNER (D8; architecture.md §6.2): the Flatpak/host-exec mapping
    /// applies to the *terminal* too — today's Dart path launched it from
    /// inside the sandbox. Fire-and-forget: the child is detached (the UI
    /// owns no output subscription for it).
    pub fn launch(&self, runner: &CommandRunner, enter: &Command) -> anyhow::Result<()> {
        let mut cmd = Command::new(&self.program);
        for arg in &self.extra_args {
            cmd.arg(arg);
        }
        cmd.arg(&self.separator_arg);
        cmd.arg(&enter.program);
        for arg in &enter.args {
            cmd.arg(arg);
        }
        runner
            .spawn(cmd)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("could not launch {}: {e}", self.name))
    }

    /// Returns a unique identifier for this terminal combining program and extra_args.
    /// This is used for deduplication since multiple terminals may use the same program
    /// (e.g., multiple flatpak terminals all use "flatpak" as the program).
    pub fn full_command_id(&self) -> String {
        if self.extra_args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.extra_args.join(" "))
        }
    }
}

static SUPPORTED_TERMINALS: LazyLock<Vec<Terminal>> = LazyLock::new(|| {
    [
        ("GNOME Console", "kgx", "--"),
        ("GNOME Terminal", "gnome-terminal", "--"),
        ("Konsole", "konsole", "-e"),
        ("Xfce Terminal", "xfce4-terminal", "-x"),
        ("Tilix", "tilix", "-e"),
        ("Kitty", "kitty", "--"),
        ("Alacritty", "alacritty", "-e"),
        ("WezTerm", "wezterm", "-e"),
        ("elementary Terminal", "io.elementary.terminal", "--"),
        ("Ptyxis", "ptyxis", "--"),
        ("Foot", "footclient", "-e"),
        ("Xterm", "xterm", "-e"),
        ("COSMIC Terminal", "cosmic-term", "-e"),
        ("Ghostty", "ghostty", "-e"),
        ("Terminator", "terminator", "-x"),
        ("QTerminal", "qterminal", "-e"),
        ("Deepin Terminal", "deepin-terminal", "-e"),
    ]
    .iter()
    .map(|(name, program, separator_arg)| Terminal {
        name: name.to_string(),
        program: program.to_string(),
        extra_args: vec![],
        separator_arg: separator_arg.to_string(),
        read_only: true,
    })
    .collect()
});

/// The built-in terminal table (D8): always listable without a repository
/// (no filesystem, no runner). The repository adds custom + flatpak entries.
pub fn builtin_terminals() -> Vec<Terminal> {
    SUPPORTED_TERMINALS.clone()
}

static FLATPAK_TERMINAL_CANDIDATES: LazyLock<Vec<Terminal>> = LazyLock::new(|| {
    let base_terminals = [
        ("Ptyxis", "app.devsuite.Ptyxis", "--"),
        ("GNOME Console", "org.gnome.Console", "--"),
        // ("BlackBox", "com.raggesilver.BlackBox", "--"), for some reason it doesn't work
        ("WezTerm", "org.wezfurlong.wezterm", "start --"),
        ("Foot", "page.codeberg.dnkl.foot", "-e"),
    ];

    let mut candidates = Vec::new();
    for (name, app_id, separator_arg) in base_terminals {
        // Stable
        candidates.push(Terminal {
            name: format!("{} (Flatpak)", name),
            program: "flatpak".to_string(),
            extra_args: vec!["run".to_string(), app_id.to_string()],
            separator_arg: separator_arg.to_string(),
            read_only: true,
        });
        // Devel
        candidates.push(Terminal {
            name: format!("{} Devel (Flatpak)", name),
            program: "flatpak".to_string(),
            extra_args: vec!["run".to_string(), format!("{}.Devel", app_id)],
            separator_arg: separator_arg.to_string(),
            read_only: true,
        });
    }
    candidates
});

#[derive(Clone)]
pub struct TerminalRepository {
    pub list: Arc<Mutex<Vec<Terminal>>>,
    pub command_runner: CommandRunner,
}

impl Default for TerminalRepository {
    fn default() -> Self {
        Self::new(CommandRunner::default())
    }
}

impl TerminalRepository {
    pub fn new(command_runner: CommandRunner) -> Self {
        // The pre-rename `distroshelf-terminals.json` custom list is NOT
        // read here any more (T12/D11): it is imported once into
        // `AppConfig::custom_terminals` through the env-mapped runner
        // (host-side under Flatpak), and customs arrive via
        // `with_customs`. Reading it again here would resolve through the
        // SANDBOX path and silently drop everything under Flatpak, and it
        // would write back with `std::fs::write` — inside the sandbox, also
        // silently. One reader, one writer, both through the mapping.
        let mut list = builtin_terminals();
        list.sort_by(|a, b| a.name.cmp(&b.name));

        Self {
            list: Arc::new(Mutex::new(list)),
            command_runner: command_runner.clone(),
        }
    }

    /// Build a list of built-ins plus `customs` (T12: the persisted /
    /// imported `AppConfig::custom_terminals`). Customs win on identity —
    /// a custom whose `full_command_id` matches a built-in replaces it —
    /// and duplicates among the customs collapse on the same key, first
    /// winning. The result is the ONE list the pickers index into (see
    /// `Backend::terminals`), so an index can never mean two different
    /// things at send time and at receive time.
    pub fn with_customs(command_runner: CommandRunner, customs: Vec<Terminal>) -> Self {
        let repo = Self::new(command_runner);
        if customs.is_empty() {
            return repo;
        }
        let mut list = customs;
        let custom_ids: HashSet<String> = list.iter().map(|t| t.full_command_id()).collect();
        let mut seen = HashSet::new();
        list.retain(|t| seen.insert(t.full_command_id()));
        list.extend(
            repo.all_terminals()
                .into_iter()
                .filter(|t| !custom_ids.contains(&t.full_command_id())),
        );
        list.sort_by(|a, b| a.name.cmp(&b.name));
        *repo.list.lock().unwrap() = list;
        repo
    }

    pub async fn fetch_flatpak_terminals(runner: &CommandRunner) -> anyhow::Result<Vec<Terminal>> {
        // Get list of installed flatpaks
        let mut cmd = Command::new_with_args("flatpak", ["list", "--app", "--columns=application"]);
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        let output = runner.output_string(cmd).await?;
        let installed_apps: HashSet<&str> = output.lines().collect();

        let mut found_terminals = Vec::new();
        for terminal in FLATPAK_TERMINAL_CANDIDATES.iter() {
            // Extract app_id from extra_args (e.g., ["run", "org.gnome.Console"])
            if let Some(app_id) = terminal.extra_args.get(1)
                && installed_apps.contains(app_id.as_str())
            {
                found_terminals.push(terminal.clone());
            }
        }

        Ok(found_terminals)
    }

    pub fn merge_flatpak_terminals(&self, terminals: Vec<Terminal>) {
        if terminals.is_empty() {
            return;
        }

        let mut list = self.list.lock().unwrap();
        // Build a set of existing terminal identifiers to avoid duplicates
        let existing_ids: HashSet<String> = list.iter().map(|t| t.full_command_id()).collect();

        // Only add terminals that don't already exist
        let new_terminals: Vec<Terminal> = terminals
            .into_iter()
            .filter(|t| !existing_ids.contains(&t.full_command_id()))
            .collect();

        if !new_terminals.is_empty() {
            list.extend(new_terminals);
            list.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }

    pub fn is_read_only(&self, name: &str) -> bool {
        self.list
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.name == name)
            .is_some_and(|x| x.read_only)
    }

    pub fn terminal_by_name(&self, name: &str) -> Option<Terminal> {
        self.list
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.name == name)
            .cloned()
    }

    pub fn terminal_by_program(&self, program: &str) -> Option<Terminal> {
        self.list
            .lock()
            .unwrap()
            .iter()
            .find(|x| x.program == program || x.full_command_id() == program)
            .cloned()
    }

    pub fn all_terminals(&self) -> Vec<Terminal> {
        self.list.lock().unwrap().clone()
    }

    pub async fn default_terminal(&self) -> Option<Terminal> {
        let mut command = Command::new_with_args(
            "gsettings",
            [
                "get",
                "org.gnome.desktop.default-applications.terminal",
                "exec",
            ],
        );
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

        let runner = &self.command_runner;

        let Ok(output) = runner.output(command.clone()).await else {
            error!("Failed to get default terminal, running {:?}", &command);
            return None;
        };

        let Ok(terminal_program) = String::from_utf8(output.stdout) else {
            error!("Default terminal output is not valid UTF-8");
            return None;
        };

        let terminal_program = terminal_program.trim().trim_matches('\'');
        if terminal_program.is_empty() {
            return None;
        }
        info!("Default terminal program: {}", terminal_program);
        self.terminal_by_program(terminal_program).or_else(|| {
            error!(
                "Terminal program {} not found in the list",
                terminal_program
            );
            None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(name: &str, program: &str, extra: &[&str]) -> Terminal {
        Terminal {
            name: name.into(),
            program: program.into(),
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
            separator_arg: "--".into(),
            read_only: false,
        }
    }

    /// T12: the customs a user imported must be *selectable*, not merely
    /// resolvable — `with_customs` is what puts them in the indexed list.
    #[test]
    fn with_customs_adds_and_sorts() {
        let repo = TerminalRepository::with_customs(
            CommandRunner::default(),
            vec![custom("AAA Mine", "my-term", &[])],
        );
        let list = repo.all_terminals();
        assert!(list.iter().any(|t| t.name == "AAA Mine"));
        assert!(list.iter().any(|t| t.name == "GNOME Terminal"));
        let names: Vec<&String> = list.iter().map(|t| &t.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "list stays name-sorted");
    }

    /// A custom that shadows a built-in replaces it rather than appearing
    /// twice — otherwise one program would occupy two picker slots and the
    /// index the user picked would be ambiguous.
    #[test]
    fn custom_shadows_builtin_by_full_command_id() {
        let repo = TerminalRepository::with_customs(
            CommandRunner::default(),
            vec![custom("Mine", "gnome-terminal", &[])],
        );
        let list = repo.all_terminals();
        let matches: Vec<&Terminal> = list
            .iter()
            .filter(|t| t.full_command_id() == "gnome-terminal")
            .collect();
        assert_eq!(matches.len(), 1, "no duplicate ids: {matches:?}");
        assert_eq!(matches[0].name, "Mine");
    }

    #[test]
    fn duplicate_customs_collapse_first_wins() {
        let repo = TerminalRepository::with_customs(
            CommandRunner::default(),
            vec![
                custom("First", "dup-term", &[]),
                custom("Second", "dup-term", &[]),
            ],
        );
        let list = repo.all_terminals();
        let found = list
            .iter()
            .find(|t| t.program == "dup-term")
            .expect("present");
        assert_eq!(found.name, "First");
        assert_eq!(
            list.iter().filter(|t| t.program == "dup-term").count(),
            1,
            "the duplicate is dropped"
        );
    }

    #[test]
    fn empty_customs_is_just_builtins() {
        let empty = TerminalRepository::with_customs(CommandRunner::default(), vec![]);
        assert_eq!(empty.all_terminals(), builtin_terminals_sorted());
    }

    fn builtin_terminals_sorted() -> Vec<Terminal> {
        let mut list = builtin_terminals();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}
