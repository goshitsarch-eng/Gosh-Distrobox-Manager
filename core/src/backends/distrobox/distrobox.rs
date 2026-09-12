use crate::fakers::{Child, Command, CommandRunner, FdMode, NullCommandRunnerBuilder};

use serde::{Deserialize, Deserializer};
use std::{
    cell::LazyCell,
    collections::BTreeMap,
    ffi::OsString,
    io,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    process::Output,
    str::FromStr,
    sync::Arc,
};
use tracing::{debug, error, info, warn};

use crate::backends::container_runtime::{PODMAN_FIRST, retarget};
use crate::backends::desktop_file::*;
use crate::backends::distrobox::command::{CmdFactory, default_cmd_factory};

const POSIX_FIND_AND_CONCAT_DESKTOP_FILES: &str =
    include_str!("POSIX_FIND_AND_CONCAT_DESKTOP_FILES.sh");

/// Encode a string as hex (matching the shell script's base16 function)
fn to_hex(s: &str) -> String {
    s.bytes().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Deserialize, Debug)]
struct DesktopFiles {
    #[serde(deserialize_with = "DesktopFiles::deserialize_path")]
    home_dir: PathBuf,
    #[serde(deserialize_with = "DesktopFiles::deserialize_desktop_files")]
    system: BTreeMap<PathBuf, String>,
    #[serde(deserialize_with = "DesktopFiles::deserialize_desktop_files")]
    user: BTreeMap<PathBuf, String>,
}

impl DesktopFiles {
    fn decode_hex<E: serde::de::Error>(hex_str: &str) -> Result<Vec<u8>, E> {
        if !hex_str.len().is_multiple_of(2) {
            return Err(E::invalid_length(
                hex_str.len(),
                &"hex string to have an even length",
            ));
        }

        (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..=i + 1], 16))
            .collect::<Result<_, _>>()
            .map_err(|e| {
                E::custom(format_args!(
                    "hex string contains non hex characters: {e:?}"
                ))
            })
    }

    fn decode_utf8_from_hex<E: serde::de::Error>(hex_str: &str) -> Result<String, E> {
        String::from_utf8(Self::decode_hex(hex_str)?).map_err(|e| {
            E::custom(format_args!(
                "decoded hex string does not represent valid UTF-8: {e:?}"
            ))
        })
    }

    fn decode_path_from_hex<E: serde::de::Error>(hex_str: &str) -> Result<PathBuf, E> {
        Ok(PathBuf::from(OsString::from_vec(Self::decode_hex(
            hex_str,
        )?)))
    }

    fn deserialize_path<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        Self::decode_path_from_hex(&String::deserialize(deserializer)?)
    }

    fn deserialize_desktop_files<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<PathBuf, String>, D::Error> {
        BTreeMap::<String, String>::deserialize(deserializer)?
            .into_iter()
            .map(|(path, content)| {
                Ok((
                    Self::decode_path_from_hex(&path)?,
                    Self::decode_utf8_from_hex(&content)?,
                ))
            })
            .collect()
    }

    fn into_map(self, host_home: Option<PathBuf>) -> BTreeMap<PathBuf, String> {
        let mut desktop_files = self.system;
        // Only include user desktop files if the container's home directory is different from the host's
        // This avoids showing duplicate entries when the container shares the host's home directory
        if host_home.as_ref() != Some(&self.home_dir) {
            desktop_files.extend(self.user)
        }
        desktop_files
    }
}

/// When a podman attempt should hand over to docker (B7). The six hand-rolled
/// pairs in `Distrobox` did not agree on this, so it is an explicit argument
/// rather than a rule baked into the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fallback {
    /// Retry when the command fails (non-zero exit, spawn error). This is the
    /// common case, and it also covers `list_snapshots`, whose old fallback
    /// retried with argv that was ALREADY identical to podman's — the `?`
    /// there only looked like error-handling because the retry sat inside the
    /// `Err` arm.
    OnError,
    /// Retry only when the command SUCCEEDED but printed nothing.
    /// `get_container_id`'s podman branch ends with `?`, so a non-zero podman
    /// exit propagates and must NOT reach docker; only an empty-but-successful
    /// listing falls through. Collapsing this into `OnError` would change that
    /// error path.
    OnEmpty,
}

#[derive(Clone)]
pub struct Distrobox {
    cmd_runner: CommandRunner,
    cmd_factory: CmdFactory,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Status {
    Up(String),
    Created(String),
    Exited(String),
    // I don't want the app to crash if the parsing fails because distrobox changed with an update.
    // We will just disable some features, but still show the status value.
    Other(String),
}

impl Default for Status {
    fn default() -> Self {
        Self::Other("".into())
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Up(s) => write!(f, "Up {}", s),
            Status::Created(s) => write!(f, "Created {}", s),
            Status::Exited(s) => write!(f, "Exited {}", s),
            Status::Other(s) => write!(f, "{}", s),
        }
    }
}

impl Status {
    fn from_str(s: &str) -> Self {
        if let Some(rest) = s.strip_prefix("Up") {
            Status::Up(rest.trim().to_string())
        } else if let Some(rest) = s.strip_prefix("Exited") {
            Status::Exited(rest.trim().to_string())
        } else if let Some(rest) = s.strip_prefix("Created") {
            Status::Created(rest.trim().to_string())
        } else {
            Status::Other(s.to_string())
        }
    }
}

#[derive(Debug, PartialEq, Hash, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub image: String,
}

impl ContainerInfo {
    fn field_missing_error(text: &str, line: &str) -> Error {
        Error::ParseOutput(format!("{text} missing in line: {}", line))
    }
}

/// `distrobox list`'s header row, recognized by its leading literal field.
///
/// `distrobox-list` prints it before any container row, and every row after it
/// is `id|name|status|image`. That lets `list` tell "the header we expect"
/// apart from "a first line we failed to recognize" — a distinction a
/// positional `.skip(1)` cannot make.
///
/// **Keyed on the first field alone, because the header's *width* is not
/// stable.** The column count is part of the release, not the format:
/// distrobox 1.5.0.2 prints six — `printf "%-12s | %-20s | %-18s | %-16s | %-5s
/// | %-30s\n" "ID" "NAME" "STATUS" "MEM" "CPU%" "IMAGE"` (line 192) — and
/// 1.6.0.1 through 1.8.2.5 print four (`printf "%-12s | %-20s | %-18s |
/// %-30s\n" "ID" "NAME" "STATUS" "IMAGE"`). Matching the four full names, as
/// this did, is therefore version-locked: on 1.5.x the real header fails the
/// test, survives into the row loop, and is counted as an unreadable
/// *container* — the UI then blames a header for a malformed row that does not
/// exist, one row too many in the count.
///
/// Exact equality on field 0 keeps the safety property the four-literal form
/// had: a container id is a hash, never the literal `ID`, so no real row can
/// match at any position. It must stay `==` and not `starts_with` — a data row
/// whose id is `ID42…` is a container, and eating it is the silent-loss failure
/// this whole change exists to remove (pinned by test).
fn is_distrobox_header(line: &str) -> bool {
    let fields: Vec<&str> = line.split('|').map(str::trim).collect();
    // `ID` alone would also match a two-field line no release prints, so the
    // arity floor keeps the predicate honest about what a header looks like.
    fields.len() >= 4 && fields.first() == Some(&"ID")
}

impl FromStr for ContainerInfo {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() != 4 {
            return Err(Error::ParseOutput(format!(
                "Invalid field count (expected 4, got {}) in line: {}",
                parts.len(),
                s
            )));
        }

        let id = parts[0].trim();
        let name = parts[1].trim();
        let status = parts[2].trim();
        let image = parts[3].trim();

        // Check for empty fields
        if id.is_empty() {
            return Err(ContainerInfo::field_missing_error("id", s));
        }
        if name.is_empty() {
            return Err(ContainerInfo::field_missing_error("name", s));
        }
        if status.is_empty() {
            return Err(ContainerInfo::field_missing_error("status", s));
        }
        if image.is_empty() {
            return Err(ContainerInfo::field_missing_error("image", s));
        }

        Ok(ContainerInfo {
            id: id.to_string(),
            name: name.to_string(),
            status: Status::from_str(status),
            image: image.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExportableApp {
    pub entry: DesktopEntry,
    pub desktop_file_path: String,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct ExportableBinary {
    pub name: String,
    pub source_path: String,
    pub exported_path: String,
}

/// Information about an installed or available package
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

/// Information about a container snapshot
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub id: String,
    pub name: String,
    pub created: String,
    pub size: String,
}

/// One row a parser refused to guess at (B3, §6.4). Carries the raw text and
/// the parser's complaint so a skipped row is *accounted for* rather than
/// silently dropped: each one is logged at the parse site with both fields,
/// and the count reaches the UI (the Dashboard's "N rows skipped" caption).
///
/// **What the app does NOT do (T13/D27):** render the fields themselves. No UI
/// lists the skipped lines — only `skipped.len()` crosses the boundary, and
/// `show_skipped_lines` gates the count alone. Carrying the detail anyway is
/// deliberate: the fields are what the log needs, and a future detail view can
/// read them without a re-plumb. The fields are `String` because
/// `DistroboxError` is not `Clone` and `ParseIssue` must be (`Message` is).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIssue {
    /// The raw line that failed. For `list_apps` there is no source line —
    /// the desktop file's PATH is the identity of the skipped row.
    pub line: String,
    /// The parser's error, rendered.
    pub error: String,
}

impl ParseIssue {
    /// For test fixtures (I23) that need a `skipped` entry without hand-writing
    /// a malformed table row.
    pub fn new(line: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            error: error.into(),
        }
    }
}

/// `list()`'s result: the containers that parsed, plus the rows that did not
/// (B3, §6.4).
///
/// `containers` is a `Vec`, **not** the `BTreeMap<String, ContainerInfo>` that
/// architecture.md §2.3 (the DRAFT `Message` struct) and §6.4 (row B3)
/// specify — a
/// recorded spec correction. The map
/// `list()` builds internally is already erased at the `Backend` boundary
/// (`Backend::containers` does `into_values().collect()`), no consumer wants
/// keyed access to it, and keeping it here is actively harmful in two ways:
/// `Deref` to a map would turn the container pickers' *positional* `.get(i)`
/// into a *keyed* lookup (silently selecting a different row or none), and it
/// would break all 28 `&self.containers` slice consumers, since a map cannot
/// coerce to `&[ContainerInfo]`.
///
/// Order matches the old map's: name-sorted (see `list`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContainerList {
    pub containers: Vec<ContainerInfo>,
    pub skipped: Vec<ParseIssue>,
}

/// Read-through to `containers`, so the ~28 existing `self.containers.*` reads
/// (`.len`, `.is_empty`, `.first`, `.iter`, `.get(i)`, and `&self.containers`
/// coercing to `&[ContainerInfo]` via `Vec`'s own `Deref<Target = [T]>`) keep
/// working untouched. `skipped` is the one field reached by name through the
/// wrapper. A `BTreeMap` target would not give the slice coercion — see the
/// struct doc.
///
/// **What `Deref` does NOT cover:** `for c in &self.containers`. There is no
/// auto-deref in a `for` head, and `&ContainerList` is not `IntoIterator`, so
/// the one `for` loop over the list had to become `.iter()`. That is the whole
/// of the app-side churn; D27 records it rather than claiming "zero".
impl std::ops::Deref for ContainerList {
    type Target = Vec<ContainerInfo>;
    fn deref(&self) -> &Self::Target {
        &self.containers
    }
}

impl ContainerList {
    /// "Nothing parsed AND nothing was dropped" — a genuinely empty account,
    /// as opposed to "every row failed", which the UI must not render as
    /// "no containers yet".
    pub fn is_clean_empty(&self) -> bool {
        self.containers.is_empty() && self.skipped.is_empty()
    }

    /// Attach already-recorded skips to a list built from parsed rows.
    ///
    /// Exists for tests (I23): the app-level fixtures reach `Backend` through
    /// `DistroboxCommandRunnerResponse::List`, which renders well-formed rows,
    /// so `skipped` is structurally empty there and a mutation that clears it —
    /// in `Backend::containers`, or hardcoded upstream in `app/` — leaves the
    /// suite green. A fixture that starts from real stdout should use
    /// `RawList`; this is for the cases that need a `skipped` list without
    /// hand-writing a table.
    pub fn with_skipped(mut self, skipped: Vec<ParseIssue>) -> Self {
        self.skipped = skipped;
        self
    }
}

/// The same contract as `ContainerList` for the four per-container peers
/// (`list_apps`, `get_exported_binaries`, `list_installed_packages`,
/// `list_snapshots`), whose item type differs.
///
/// A peer's `skipped` does **not** ride to the app in T13: those `Backend`
/// methods already erase their result to `Vec<T>` for the `AppInfo` /
/// `ExportedBinary` / `PackageInfo` / `SnapshotInfo` DTO mapping, and
/// `show_skipped_lines` (architecture.md §5.3) is specified against
/// `ContainerList.skipped` alone. The wrapper exists so each parse loop has
/// somewhere to record a refusal that the boundary then logs.
#[derive(Debug, Clone)]
pub struct TolerantList<T> {
    pub items: Vec<T>,
    pub skipped: Vec<ParseIssue>,
}

// Hand-written: the derive would demand `T: Default` (`ExportableApp`,
// `ExportableBinary`, `PackageInfo` and `SnapshotInfo` all lack it), but an
// empty list needs nothing from its element type.
impl<T> Default for TolerantList<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            skipped: Vec::new(),
        }
    }
}

/// Read-through to `items`, for the same reason as `ContainerList`: `.len`,
/// indexing and `.iter()` keep working, and `skipped` is the one field reached
/// by name.
///
/// **What `Deref` does NOT cover:** `for x in &wrapper`. `Deref` coercion does
/// not apply to the `for` head — `IntoIterator` is resolved on the type written
/// there — so `&TolerantList<T>` is not `IntoIterator` and that form is a
/// compile error. Write `for x in wrapper.iter()` (which goes through
/// `Deref` to the slice, and does work), or `for x in &*wrapper`.
impl<T> std::ops::Deref for TolerantList<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

/// Resource usage statistics for a container
#[derive(Debug, Clone, Default)]
pub struct ContainerStats {
    pub cpu_percent: f64,
    pub memory_usage: String,
    pub memory_limit: String,
    pub memory_percent: f64,
    pub network_io: String,
    pub block_io: String,
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct CreateArgName(pub String);

impl std::fmt::Display for CreateArgName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl CreateArgName {
    pub fn new(value: &str) -> Result<Self, Error> {
        let re = regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$").unwrap();
        if re.is_match(value) {
            Ok(CreateArgName(value.to_string()))
        } else {
            Err(Error::InvalidField(
                "name".into(),
                "Must respect the format [a-zA-Z0-9][a-zA-Z0-9_.-]*".into(),
            ))
        }
    }
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct CreateArgs {
    pub init: bool,
    pub nvidia: bool,
    pub home_path: Option<String>,
    pub image: String,
    pub name: CreateArgName,
    pub volumes: Vec<Volume>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeMode {
    ReadOnly,
}

impl std::fmt::Display for VolumeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VolumeMode::ReadOnly => write!(f, "ro"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub host_path: String,
    pub container_path: String,
    pub mode: Option<VolumeMode>,
}

impl FromStr for Volume {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.as_slice() {
            [host] => Ok(Volume {
                host_path: host.to_string(),
                container_path: host.to_string(),
                mode: None,
            }),
            [host, target] => Ok(Volume {
                host_path: host.to_string(),
                container_path: target.to_string(),
                mode: None,
            }),
            [host, target, "ro"] => Ok(Volume {
                host_path: host.to_string(),
                container_path: target.to_string(),
                mode: Some(VolumeMode::ReadOnly),
            }),
            _ => Err(Error::InvalidField(
                "volume".into(),
                format!("Invalid volume descriptor: {}", s),
            )),
        }
    }
}

impl std::fmt::Display for Volume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host_path, self.container_path)?;
        if let Some(mode) = &self.mode {
            write!(f, ":{}", mode)?;
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to read command stdout: {0}")]
    StdoutRead(#[from] io::Error),

    #[error("failed to spawn command {command}: {source}")]
    Spawn { source: io::Error, command: String },

    #[error("failed to parse command output: {0}")]
    ParseOutput(String),

    #[error("invalid field {0}: {1}")]
    InvalidField(String, String),

    #[error("command failed with exit code {exit_code:?}: {command}\n{stderr}")]
    CommandFailed {
        exit_code: Option<i32>,
        command: String,
        stderr: String,
    },

    #[error("failed to resolve host path: {0}. getfattr may not be installed on the host")]
    ResolveHostPath(String),
}

/// Represents mock responses for the NullCommandRunner used in previews and testing.
///
/// These responses simulate the output of various distrobox commands without
/// actually executing them. This is essential for:
/// - UI previews in development (via DistroboxStoreTy::NullWorking)
/// - Unit testing without requiring a real distrobox installation
/// - Flatpak sandbox testing
#[derive(Clone)]
pub enum DistroboxCommandRunnerResponse {
    /// Mock response for `distrobox version` command
    /// Returns a successful version string like "distrobox: 1.7.2.1"
    Version,
    /// Mock response for when distrobox is not installed
    /// Returns an error when version is queried
    NoVersion,
    /// Mock response for `distrobox ls --no-color` command
    /// Returns a list of containers in the expected pipe-delimited format
    List(Vec<ContainerInfo>),
    /// Mock response for `distrobox ls --no-color` that returns **raw** stdout,
    /// bypassing the generated table.
    ///
    /// B3 (`ContainerList.skipped`) is unobservable through `List`: that variant
    /// renders well-formed rows from `ContainerInfo`s, so `skipped` is
    /// structurally empty in every fixture built on it and a mutation that clears
    /// it — or that hardcodes `skipped: 0` upstream in `app/` — leaves the suite
    /// green. Reproducing real `distrobox` output (a header, some good rows, one
    /// malformed row) needs the bytes, so this variant hands them over verbatim.
    /// See I23 in `docs/migration/PLAN.md`.
    RawList(String),
    /// Mock response for `distrobox create --compatibility` command
    /// Returns a list of compatible container images
    Compatibility(Vec<String>),
    /// Mock response for listing exportable applications from a container
    /// Contains: (distrobox_name, [(filename, app_name, icon_name)])
    /// Generates the TOML hex-encoded format expected by the desktop file parser
    ExportedApps(String, Vec<(String, String, String)>),
}

/// A canned stdout producer attached to a `Command`: the closure returns the text
/// the fake runner should report, or an I/O error for the failure paths.
type ResponseFn = Arc<dyn Fn() -> io::Result<String> + Send + Sync>;

impl DistroboxCommandRunnerResponse {
    pub fn common_distros() -> LazyCell<Vec<ContainerInfo>> {
        LazyCell::new(|| {
            [
                ("1", "Ubuntu", "docker.io/library/ubuntu:latest"),
                ("2", "Fedora", "docker.io/library/fedora:latest"),
                ("3", "Kali", "docker.io/kalilinux/kali-rolling"),
                ("4", "Debian", "docker.io/library/debian:latest"),
                ("5", "Arch Linux", "docker.io/library/archlinux:latest"),
                ("6", "CentOS", "docker.io/library/centos:latest"),
                ("7", "Alpine", "docker.io/library/alpine:latest"),
                ("8", "OpenSUSE", "docker.io/library/opensuse:latest"),
                ("9", "Gentoo", "docker.io/library/gentoo:latest"),
                ("10", "Slackware", "docker.io/library/slackware:latest"),
                ("11", "Void Linux", "docker.io/library/voidlinux:latest"),
                ("13", "Deepin", "docker.io/library/deepin:latest"),
                ("16", "Rocky Linux", "docker.io/library/rockylinux:latest"),
                (
                    "17",
                    "Crystal Linux",
                    "docker.io/library/crystal-linux:latest",
                ),
            ]
            .iter()
            .map(|(id, name, image)| ContainerInfo {
                id: id.to_string(),
                name: name.to_string(),
                status: Status::Created("2 minutes ago".into()),
                image: image.to_string(),
            })
            .collect()
        })
    }

    pub fn new_list_common_distros() -> Self {
        Self::List(Self::common_distros().to_owned())
    }

    pub fn new_common_exported_apps() -> Self {
        let dummy_exported_apps = vec![
            ("vim.desktop".into(), "Vim".into(), "vim".into()),
            ("matlab.desktop".into(), "MATLAB".into(), "matlab".into()),
            (
                "vscode.desktop".into(),
                "Visual Studio Code".into(),
                "code".into(),
            ),
            ("rstudio.desktop".into(), "RStudio".into(), "rstudio".into()),
            (
                "sublime_text.desktop".into(),
                "Sublime Text".into(),
                "subl".into(),
            ),
            ("zoom.desktop".into(), "Zoom".into(), "zoom".into()),
            ("slack.desktop".into(), "Slack".into(), "slack".into()),
            ("postman.desktop".into(), "Postman".into(), "postman".into()),
        ];
        DistroboxCommandRunnerResponse::ExportedApps("Ubuntu".into(), dummy_exported_apps)
    }

    pub fn new_common_images() -> Self {
        DistroboxCommandRunnerResponse::Compatibility(
            Self::common_distros()
                .iter()
                .map(|x| x.image.clone())
                .collect(),
        )
    }

    fn build_version_response() -> (Command, String) {
        let mut cmd = default_cmd_factory()();
        cmd.arg("version");
        (cmd, "distrobox: 1.7.2.1".to_string())
    }

    fn build_no_version_response() -> (Command, Arc<dyn Fn() -> io::Result<String> + Send + Sync>) {
        let mut cmd = default_cmd_factory()();
        cmd.arg("version");
        (cmd, Arc::new(|| Err(io::Error::from_raw_os_error(0))))
    }

    fn build_list_response(containers: &[ContainerInfo]) -> (Command, String) {
        let mut output = String::new();
        output.push_str("ID           | NAME                 | STATUS             | IMAGE  \n");
        for container in containers {
            output.push_str(&container.id);
            output.push_str(" | ");
            output.push_str(&container.name);
            output.push_str(" | ");
            let status = container.status.to_string();
            output.push_str(&format!("{status} | "));
            output.push_str(&container.image);
            output.push('\n');
        }
        let mut cmd = default_cmd_factory()();
        cmd.arg("ls").arg("--no-color");
        (cmd, output.clone())
    }

    fn build_compatibility_response(images: &[String]) -> (Command, String) {
        let output = images.join("\n");
        let mut cmd = default_cmd_factory()();
        cmd.arg("create").arg("--compatibility");
        (cmd, output)
    }

    fn build_exported_apps_commands(
        box_name: &str,
        apps: &[(String, String, String)],
    ) -> Vec<(Command, String)> {
        let mut commands = Vec::new();

        // Get XDG_DATA_HOME (mocked via printenv)
        commands.push((
            Command::new_with_args("printenv", ["XDG_DATA_HOME"]),
            String::new(),
        ));

        // Get HOME if XDG_DATA_HOME is empty (mocked via printenv)
        commands.push((
            Command::new_with_args("printenv", ["HOME"]),
            "/home/me".to_string(),
        ));

        // List desktop files - these are the exported files in the user's local applications folder
        // Format: {box_name}-{filename}
        let file_list = apps
            .iter()
            .map(|(filename, _, _)| format!("{box_name}-{}", filename))
            .collect::<Vec<_>>()
            .join("\n");
        commands.push((
            Command::new_with_args("ls", ["/home/me/.local/share/applications"]),
            file_list,
        ));

        // Build desktop files TOML with hex encoding (matching POSIX_FIND_AND_CONCAT_DESKTOP_FILES.sh output)
        let mut toml = format!("home_dir=\"{}\"\n", to_hex("/home/me"));

        toml.push_str("[system]\n");
        for (filename, name, icon) in apps {
            let path = format!("/usr/share/applications/{}", filename);
            let content = format!(
                "[Desktop Entry]\n\
                Type=Application\n\
                Name={}\n\
                Exec=/path/to/{}\n\
                Icon={}\n\
                Categories=Utility;Network;",
                name, name, icon
            );
            toml.push_str(&format!("\"{}\"=\"{}\"\n", to_hex(&path), to_hex(&content)));
        }

        toml.push_str("[user]\n");

        let mut db_cmd = default_cmd_factory()();
        db_cmd.args([
            "enter",
            box_name,
            "--",
            "sh",
            "-c",
            POSIX_FIND_AND_CONCAT_DESKTOP_FILES,
        ]);
        commands.push((db_cmd, toml));

        commands
    }

    fn wrap_err_fn(output: (Command, String)) -> (Command, ResponseFn) {
        (output.0, Arc::new(move || Ok(output.1.clone())))
    }

    pub fn to_commands(self) -> Vec<(Command, ResponseFn)> {
        match self {
            Self::Version => {
                let working_response = Self::build_version_response();
                vec![Self::wrap_err_fn(working_response)]
            }
            Self::NoVersion => {
                vec![Self::build_no_version_response()]
            }
            Self::List(containers) => {
                vec![Self::wrap_err_fn(Self::build_list_response(&containers))]
            }
            Self::RawList(output) => {
                let mut cmd = default_cmd_factory()();
                cmd.arg("ls").arg("--no-color");
                vec![Self::wrap_err_fn((cmd, output))]
            }
            Self::Compatibility(images) => vec![Self::wrap_err_fn(
                Self::build_compatibility_response(&images),
            )],
            Self::ExportedApps(box_name, apps) => {
                Self::build_exported_apps_commands(&box_name, &apps)
                    .into_iter()
                    .map(Self::wrap_err_fn)
                    .collect()
            }
        }
    }
}

impl Distrobox {
    // The command factory ensures we can customize the distrobox executable path, e.g. to use a bundled version.
    pub fn new(cmd_runner: CommandRunner, cmd_factory: CmdFactory) -> Self {
        Self {
            cmd_runner,
            cmd_factory,
        }
    }

    /// The env-mapped runner (D8): terminal launch reuses it so the
    /// Flatpak/host-exec mapping applies to the terminal too.
    pub fn runner(&self) -> &CommandRunner {
        &self.cmd_runner
    }

    fn dbcmd(&self) -> Command {
        (self.cmd_factory)()
    }

    pub fn null_command_runner(responses: &[DistroboxCommandRunnerResponse]) -> CommandRunner {
        let mut builder = NullCommandRunnerBuilder::new();
        for res in responses {
            for (cmd, out) in res.clone().to_commands() {
                builder.cmd_full(cmd, move || out());
            }
        }
        builder.build()
    }

    pub fn cmd_spawn(&self, mut cmd: Command) -> Result<Box<dyn Child + Send>, Error> {
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        let program = cmd.program.to_string_lossy().to_string();
        let args = cmd
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        debug!(command = %program, args = ?args, "Spawning command");
        let child = self.cmd_runner.spawn(cmd.clone()).map_err(|e| {
            let full_command = format!("{:?} {:?}", program, args);
            error!(error = ?e, command = %full_command, "Command spawn failed");
            Error::Spawn {
                source: e,
                command: full_command,
            }
        })?;

        Ok(child)
    }

    async fn cmd_output(&self, mut cmd: Command) -> Result<Output, Error> {
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        let program = cmd.program.to_string_lossy().to_string();
        let args = cmd
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        info!(command = %program, args = ?args, "Executing command");
        let command_str = format!("{:?} {:?}", program, args);

        let output = self.cmd_runner.output(cmd).await.map_err(|e| {
            error!(error = ?e, command = %program, "Command execution failed");
            Error::Spawn {
                source: e,
                command: command_str.clone(),
            }
        })?;

        let exit_code = output.status.code();
        debug!(
            exit_code = ?exit_code,
            "Command completed successfully"
        );
        Ok(output)
    }

    async fn cmd_output_string(&self, cmd: Command) -> Result<String, Error> {
        let command_str = format!("{:?} {:?}", cmd.program, cmd.args);
        let output = self.cmd_output(cmd).await?;
        let s = String::from_utf8_lossy(&output.stdout);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code();
            error!(
                exit_code = ?exit_code,
                stderr = %stderr,
                "Command failed"
            );
            return Err(Error::CommandFailed {
                exit_code,
                command: command_str,
                stderr,
            });
        }

        Ok(s.to_string())
    }

    async fn host_applications_path(&self) -> Result<PathBuf, Error> {
        // Resolve XDG_DATA_HOME via runner (works in Flatpak via map_flatpak_spawn_host)
        let xdg_data_home_opt =
            match crate::fakers::resolve_host_env_via_runner(&self.cmd_runner, "XDG_DATA_HOME")
                .await
            {
                Ok(Some(s)) if !s.trim().is_empty() => Some(Path::new(s.trim()).to_path_buf()),
                Ok(_) => None,
                Err(e) => {
                    tracing::warn!("failed to resolve XDG_DATA_HOME via CommandRunner: {e:?}");
                    None
                }
            };

        let apps_base = if let Some(p) = xdg_data_home_opt {
            p
        } else {
            // Fallback to HOME
            match crate::fakers::resolve_host_env_via_runner(&self.cmd_runner, "HOME").await {
                Ok(Some(s)) if !s.trim().is_empty() => Path::new(s.trim()).join(".local/share"),
                Ok(_) => {
                    return Err(Error::ResolveHostPath(
                        "XDG_DATA_HOME and HOME are not set on the host".into(),
                    ));
                }
                Err(e) => {
                    tracing::warn!("failed to resolve HOME via CommandRunner: {e:?}");
                    return Err(Error::ResolveHostPath("failed to resolve host HOME".into()));
                }
            }
        };

        let apps_path = apps_base.join("applications");
        Ok(apps_path)
    }
    async fn get_exported_desktop_files(&self) -> Result<Vec<String>, Error> {
        // We do everything with the command line to ensure we can access the files and environment variables
        // even when inside a flatpak sandbox, with only the permissions to run `flatpak-spawn`
        let mut cmd = Command::new("ls");
        cmd.arg(self.host_applications_path().await?);
        let ls_out = self.cmd_output_string(cmd).await?;
        let apps = ls_out
            .trim()
            .split("\n")
            .map(|app| app.to_string())
            .collect::<Vec<_>>();
        Ok(apps)
    }

    async fn get_desktop_files(&self, box_name: &str) -> Result<Vec<(String, String)>, Error> {
        let mut cmd = self.dbcmd();
        cmd.args([
            "enter",
            box_name,
            "--",
            "sh",
            "-c",
            POSIX_FIND_AND_CONCAT_DESKTOP_FILES,
        ]);
        let desktop_files: DesktopFiles = toml::from_str(&self.cmd_output_string(cmd).await?)
            .map_err(|e| Error::ParseOutput(format!("{e:?}")))?;
        debug!(desktop_files = format_args!("{desktop_files:#?}"));

        // Resolve host HOME via CommandRunner so this works inside Flatpak as well
        let host_home_opt =
            match crate::fakers::resolve_host_env_via_runner(&self.cmd_runner, "HOME").await {
                Ok(Some(s)) => Some(PathBuf::from(s)),
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!("failed to resolve host HOME via CommandRunner: {e:?}");
                    None
                }
            };

        Ok(desktop_files
            .into_map(host_home_opt)
            .into_iter()
            .map(|(path, content)| (path.to_string_lossy().into_owned(), content))
            .collect::<Vec<_>>())
    }

    pub async fn list_apps(&self, box_name: &str) -> Result<TolerantList<ExportableApp>, Error> {
        let files = self.get_desktop_files(box_name).await?;
        debug!(desktop_files=?files);
        let exported = self.get_exported_desktop_files().await?;
        debug!(exported_files=?exported);
        // B3: this peer already warned-and-continued; it now also records the
        // skip so the four parsers report the same way. `line` carries the
        // desktop file's PATH — there is no source line to quote.
        let mut out = TolerantList::default();
        for (path, content) in files {
            let entry = match parse_desktop_file(&content) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Failed to parse desktop file {}: {}", path, e);
                    out.skipped.push(ParseIssue {
                        line: path,
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            let file_name = Path::new(&path)
                .file_name()
                .map(|x| x.to_str())
                .unwrap_or_default()
                .unwrap_or_default();

            let exported_as = format!("{box_name}-{file_name}");
            let is_exported = exported.contains(&exported_as);
            if is_exported {
                debug!(found_exported = exported_as);
            }
            out.items.push(ExportableApp {
                desktop_file_path: path,
                entry,
                exported: is_exported,
            });
        }

        Ok(out)
    }

    /// Lists only the binaries that have already been exported from the container.
    pub async fn get_exported_binaries(
        &self,
        box_name: &str,
    ) -> Result<TolerantList<ExportableBinary>, Error> {
        let mut cmd = self.dbcmd();
        cmd.args([
            "enter",
            box_name,
            "--",
            "distrobox-export",
            "--list-binaries",
        ]);
        // Example output: '/usr/bin/vim' | /home/user/.local/bin/vim
        let output = self.cmd_output_string(cmd).await?;
        debug!(binaries_output = output);

        let mut out = TolerantList::default();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // B3: a row with no separator is a shape this parser cannot read,
            // and it used to be dropped without a word.
            if !line.contains('|') {
                warn!(line = %line, "Skipping binary row with no '|' separator");
                out.skipped.push(ParseIssue {
                    line: line.to_string(),
                    error: "expected '<source> | <exported>'".to_string(),
                });
                continue;
            }

            let parts: Vec<&str> = line.split('|').collect();
            // Unreachable past the delimiter check above (a line containing
            // '|' always splits into >= 2 parts), so this stays the original
            // defensive guard rather than becoming a second, dead ParseIssue.
            if parts.len() >= 2 {
                let source_path = parts[0].trim().to_string();
                // For some reason distrobox formats the source path between single quotes, so we need to remove those
                let source_path = source_path.trim_matches('\'').to_string();

                let exported_path_str = parts[1].trim();

                // Only include binaries that have a non-empty exported path. It should always be the case, but BoxBuddy defensively checks it.
                // In this case we try to follow BoxBuddy's behavior to keep consistency for users.
                if !exported_path_str.is_empty() {
                    let exported_path = exported_path_str.to_string();

                    // If source_path is empty (due to a bug in distrobox's --list-binaries when
                    // sudo_prefix is not set, common in Arch Linux containers), try to extract
                    // the actual binary path from the exported wrapper script.
                    let source_path = if source_path.is_empty() {
                        self.extract_binary_path_from_wrapper(&exported_path)
                            .await
                            .unwrap_or_else(|| exported_path.clone())
                    } else {
                        source_path
                    };

                    // Extract binary name from source path, falling back to exported_path if source_path is still problematic.
                    let name = Path::new(&source_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .filter(|s| !s.is_empty())
                        .or_else(|| {
                            Path::new(&exported_path)
                                .file_name()
                                .and_then(|n| n.to_str())
                        })
                        .unwrap_or(&source_path)
                        .to_string();

                    // Note: an empty exported path is a deliberate policy
                    // drop (BoxBuddy parity), not a parse failure — it is not
                    // a ParseIssue.
                    out.items.push(ExportableBinary {
                        name,
                        source_path,
                        exported_path,
                    });
                }
            }
        }

        Ok(out)
    }

    /// Extracts the original binary path from a distrobox exported wrapper script.
    /// The wrapper script contains lines like: exec '/usr/bin/binary' "$@"
    ///
    /// Uses the shared `extract_quoted_string` utility from desktop_file module for
    /// consistent string parsing across the codebase.
    async fn extract_binary_path_from_wrapper(&self, wrapper_path: &str) -> Option<String> {
        // Read the wrapper script content
        let cmd = Command::new_with_args("cat", [wrapper_path]);
        let output = self.cmd_output_string(cmd).await.ok()?;

        // Look for the pattern: exec ... '/path/to/binary' or exec '/path/to/binary'
        // The binary path is typically in single quotes in the else branch
        for line in output.lines() {
            let trimmed = line.trim();
            // Look for lines with exec that contain a quoted path
            if trimmed.starts_with("exec") {
                // Reuse the shared quoted string extraction logic from desktop_file module
                if let Some(path) = extract_quoted_string(trimmed, '\'') {
                    // Validate it looks like an absolute path to the actual binary
                    // (not a distrobox wrapper command)
                    if path.starts_with('/') && !path.contains("distrobox") {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    pub fn launch_app(
        &self,
        container: &str,
        app: &ExportableApp,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // B4: real argv elements. The old code stripped four field codes with
        // a `str::replace` fold and then passed the remainder as a SINGLE
        // argument. `distrobox-enter` ends in `exec "$@"` (distrobox 1.8.2.5,
        // the `--` branch at script line 257 — `shift; break` then `exec "$@"`,
        // with no `eval` and no re-split), so one element is one argv slot and
        // `exec "/usr/bin/foo --title My Document"` searches for a program
        // whose name is that whole literal string: the app did not launch with
        // mangled arguments, it failed to launch at all. Splitting here is what
        // makes the Exec work, and it keeps a container-supplied Exec from
        // dictating argv shape.
        let argv = split_exec(&app.entry.exec);
        if argv.is_empty() {
            // An `Exec` that is empty or entirely field codes would otherwise
            // emit a dangling `enter --name <box> --`, which enters the
            // container with no command at all. Fail loudly instead.
            return Err(Error::CommandFailed {
                exit_code: None,
                command: "launch_app".into(),
                stderr: format!(
                    "desktop entry '{}' has no executable command in its Exec key",
                    app.entry.name
                ),
            });
        }
        let mut cmd = self.dbcmd();
        cmd.arg("enter").arg("--name").arg(container).arg("--");
        cmd.args(argv);
        self.cmd_spawn(cmd)
    }

    pub async fn export_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["--app", desktop_file_path]),
        );

        self.cmd_output_string(cmd).await
    }
    pub async fn unexport_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["-d", "--app", desktop_file_path]),
        );

        self.cmd_output_string(cmd).await
    }

    pub async fn export_binary(
        &self,
        container: &str,
        binary_name_or_path: &str,
    ) -> Result<String, Error> {
        // Check if the input is a path or just a binary name
        // If it doesn't contain a '/' it's likely just a binary name
        let resolved_path = if !binary_name_or_path.contains('/') {
            // Resolve the binary name to its full path using 'which'
            self.resolve_binary_path(container, binary_name_or_path)
                .await?
        } else {
            binary_name_or_path.to_string()
        };

        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["--bin", &resolved_path]),
        );

        self.cmd_output_string(cmd).await
    }

    /// Resolves a binary name to its full path using 'which' inside the container
    async fn resolve_binary_path(
        &self,
        container: &str,
        binary_name: &str,
    ) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container, "--", "which", binary_name]);

        let output = self.cmd_output_string(cmd).await?;
        let path = output.trim();

        if path.is_empty() {
            return Err(Error::CommandFailed {
                exit_code: Some(1),
                command: format!("which {}", binary_name),
                stderr: format!("Binary '{}' not found in container", binary_name),
            });
        }

        Ok(path.to_string())
    }

    pub async fn unexport_binary(
        &self,
        container: &str,
        binary_path: &str,
    ) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["-d", "--bin", binary_path]),
        );

        self.cmd_output_string(cmd).await
    }

    // assemble
    pub fn assemble(&self, file_path: &str) -> Result<Box<dyn Child + Send>, Error> {
        if file_path.is_empty() {
            return Err(Error::InvalidField(
                "file_path".into(),
                "File path cannot be empty".into(),
            ));
        }
        let mut cmd = self.dbcmd();
        cmd.arg("assemble")
            .arg("create")
            .arg("--file")
            .arg(file_path);
        self.cmd_spawn(cmd)
    }

    pub fn assemble_from_url(&self, url: &str) -> Result<Box<dyn Child + Send>, Error> {
        if url.is_empty() {
            return Err(Error::InvalidField(
                "url".into(),
                "URL cannot be empty".into(),
            ));
        }
        let mut cmd = self.dbcmd();
        cmd.arg("assemble").arg("create").arg("--file").arg(url);
        self.cmd_spawn(cmd)
    }
    fn create_cmd(&self, args: CreateArgs) -> Command {
        let mut cmd = self.dbcmd();
        cmd.arg("create").arg("--yes");
        if !args.image.is_empty() {
            cmd.arg("--image").arg(args.image);
        }
        if !args.name.0.is_empty() {
            cmd.arg("--name").arg(args.name.0);
        }
        if args.init {
            cmd.arg("--init")
                .arg("--additional-packages")
                .arg("systemd");
        }
        if args.nvidia {
            cmd.arg("--nvidia");
        }
        if let Some(home_path) = args.home_path {
            cmd.arg("--home").arg(home_path);
        }
        for volume in args.volumes {
            cmd.arg("--volume").arg(volume.to_string());
        }
        cmd
    }
    // create
    pub async fn create(&self, args: CreateArgs) -> Result<Box<dyn Child + Send>, Error> {
        let cmd = self.create_cmd(args);
        self.cmd_spawn(cmd)
    }
    // create --compatibility
    pub async fn list_images(&self) -> Result<Vec<String>, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("create").arg("--compatibility");
        let text = self.cmd_output_string(cmd).await?;
        let lines = text
            .lines()
            .filter_map(|x| {
                if !x.is_empty() {
                    Some(x.to_string())
                } else {
                    None
                }
            })
            .collect();
        Ok(lines)
    }
    // enter
    pub fn enter_cmd(&self, name: &str) -> Command {
        let mut cmd = self.dbcmd();
        cmd.arg("enter").arg(name);
        cmd
    }
    // clone from an existing container using create args to customize the clone
    pub async fn clone_from(
        &self,
        source_name: &str,
        args: CreateArgs,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = self.create_cmd(args);
        cmd.remove_flag_value_arg("--image");
        cmd.arg("--clone").arg(source_name);
        self.cmd_spawn(cmd)
    }
    // list | ls
    pub async fn list(&self) -> Result<ContainerList, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("ls").arg("--no-color");
        let text = self.cmd_output_string(cmd).await?;
        let mut out = ContainerList::default();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // B3: drop the header by IDENTITY, not by position. This used to
            // be `text.lines().skip(1)`, which removed whatever happened to
            // be first — so a response without a header silently lost a real
            // container, and the drop was invisible: not logged, not counted,
            // not shown. That is the same silent-loss class B3 removes for
            // unparseable rows, so the header no longer gets to skip
            // inspection. The predicate is a four-literal match, which no real
            // container row can satisfy (its id is a container hash, never the
            // literal `ID`), so this is safe at any position and also survives
            // a leading line that is not the header.
            if is_distrobox_header(line) {
                continue;
            }
            match line.parse::<ContainerInfo>() {
                Ok(item) => {
                    debug!(
                        container_id = %item.id,
                        container_name = %item.name,
                        image = %item.image,
                        status = ?item.status,
                        "Discovered container"
                    );
                    out.containers.push(item);
                }
                Err(e) => {
                    // B3: one unparseable row no longer discards the rest.
                    // Before this, a single malformed line (an image name
                    // containing `|`, a future distrobox column change) made
                    // the whole list an error screen over containers that had
                    // parsed perfectly well.
                    warn!(error = %e, line = %line, "Skipping unparseable container row");
                    out.skipped.push(ParseIssue {
                        line: line.to_string(),
                        error: e.to_string(),
                    });
                }
            }
        }
        // Preserve the ordering the old `BTreeMap` produced (name-sorted) so
        // this is not also a display-order change. Podman container names are
        // unique, so the map's dedup-by-name had nothing to do.
        out.containers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
    // rm
    pub async fn remove(&self, name: &str) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("rm").arg("--force").arg(name);
        self.cmd_output_string(cmd).await
    }
    // stop
    pub async fn stop(&self, name: &str) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("stop").arg("--yes").arg(name);
        self.cmd_output_string(cmd).await
    }
    // start (architecture.md §6.4, row B5): there is NO `distrobox start` subcommand
    // (verified against distrobox 1.8.2.5's dispatch table) — the spec
    // prescribes `podman start <name>` with a Docker fallback, mirroring
    // `get_container_id`. A UI button labelled "Start" means a true start
    // leaving the container `Up` with no attached process. Callers refresh
    // `list()` afterwards: the `Created|Exited → Up` transition is observed,
    // not assumed.
    pub async fn start(&self, name: &str) -> Result<String, Error> {
        // Podman first, Docker fallback (by NAME — podman/docker accept
        // names, avoiding the ID-namespace mismatch between runtimes).
        let mut cmd = Command::new("podman");
        cmd.args(["start", name]);
        self.runtime_output(cmd, Fallback::OnError).await
    }
    pub async fn stop_all(&self) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("stop").arg("--all").arg("--yes");
        self.cmd_output_string(cmd).await
    }
    // upgrade
    pub fn upgrade(&self, name: &str) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("upgrade").arg(name);

        self.cmd_spawn(cmd)
    }
    pub async fn upgrade_all(&mut self) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("upgrade").arg("--all");
        self.cmd_output_string(cmd).await
    }
    // ephemeral
    // generate-entry
    // version
    pub async fn version(&self) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.arg("version");
        let text = self.cmd_output_string(cmd).await?;
        let mut parts = text.split(':');
        if let Some(v) = parts.nth(1) {
            let version = v.trim().to_string();
            info!(
                distrobox_version = %version,
                raw_output = %text,
                "Successfully parsed distrobox version"
            );
            Ok(version)
        } else {
            warn!(output = %text, "Failed to parse version from output");
            Err(Error::ParseOutput(format!(
                "Failed to parse version from output: {}",
                text
            )))
        }
    }

    // help

    // ============================================================================
    // Command Execution APIs
    // ============================================================================

    /// Run an arbitrary command inside a container and return its output
    /// This is useful for package management, system queries, etc.
    pub async fn run_in_container(&self, container: &str, command: &str) -> Result<String, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container, "--", "sh", "-c", command]);
        self.cmd_output_string(cmd).await
    }

    /// Run a command inside a container and return a Child for streaming output
    /// Used for long-running operations like package installs
    pub fn run_in_container_streaming(
        &self,
        container: &str,
        command: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = self.dbcmd();
        cmd.args(["enter", "--name", container, "--", "sh", "-c", command]);
        self.cmd_spawn(cmd)
    }

    /// Detect the package manager in use inside a container (B1: typed).
    /// ONE detection script (the three divergent copies in
    /// `detect/install/remove` are collapsed here). `yum` reports through
    /// the shared script and maps to `Dnf` in `from_detected`.
    /// The single detection script (B1). Reports one word; `yum` and `dnf`
    /// both reported (callers map both to `Dnf`).
    fn detect_script() -> &'static str {
        r#"
            if command -v apt >/dev/null 2>&1; then echo "apt";
            elif command -v dnf >/dev/null 2>&1; then echo "dnf";
            elif command -v yum >/dev/null 2>&1; then echo "yum";
            elif command -v pacman >/dev/null 2>&1; then echo "pacman";
            elif command -v zypper >/dev/null 2>&1; then echo "zypper";
            elif command -v apk >/dev/null 2>&1; then echo "apk";
            elif command -v xbps-install >/dev/null 2>&1; then echo "xbps";
            elif command -v emerge >/dev/null 2>&1; then echo "emerge";
            else echo "unknown"; fi
        "#
    }

    pub async fn detect_package_manager(
        &self,
        container: &str,
    ) -> Result<crate::models::PackageManager, Error> {
        // Single shared script (B1) — no local copy.
        let output = self
            .run_in_container(container, Self::detect_script())
            .await?;
        Ok(crate::models::PackageManager::from_detected(output.trim()))
    }

    // ============================================================================
    // Package Management APIs
    // ============================================================================

    /// List installed packages in a container.
    ///
    /// Tolerant of *rows* like its peers (a malformed line lands in
    /// `skipped`), but deliberately **not** of a missing prerequisite: an
    /// unrecognized package manager is a typed `Err`, not an empty list.
    /// Those are different failures — a manager we cannot identify produces no
    /// rows at all, and returning `Ok(vec![])` for it would report "0 packages
    /// installed" for a container that may have hundreds. Every peer draws the
    /// line in the same place (a missing podman is an `Err` too); only
    /// row-level refusal is tolerated. See D27.
    pub async fn list_installed_packages(
        &self,
        container: &str,
    ) -> Result<TolerantList<PackageInfo>, Error> {
        use crate::models::PackageManager;
        let pkg_manager = self.detect_package_manager(container).await?;

        let script = match pkg_manager {
            PackageManager::Apt => {
                r#"dpkg-query -W -f='${Package}\t${Version}\t${Description}\n' 2>/dev/null | head -500"#
            }
            PackageManager::Dnf => {
                r#"rpm -qa --queryformat '%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\n' 2>/dev/null | head -500"#
            }
            PackageManager::Pacman => {
                r#"pacman -Q 2>/dev/null | while read name ver; do desc=$(pacman -Qi "$name" 2>/dev/null | grep "^Description" | cut -d: -f2- | xargs); echo -e "$name\t$ver\t$desc"; done | head -500"#
            }
            PackageManager::Zypper => {
                r#"rpm -qa --queryformat '%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\n' 2>/dev/null | head -500"#
            }
            PackageManager::Apk => {
                r#"apk list --installed 2>/dev/null | sed 's/ \[installed\]//' | while read pkg; do name=$(echo "$pkg" | cut -d- -f1); ver=$(echo "$pkg" | cut -d- -f2-); echo -e "$name\t$ver\t"; done | head -500"#
            }
            PackageManager::Xbps => {
                r#"xbps-query -l 2>/dev/null | awk '{print $2}' | while read pkg; do ver=$(xbps-query "$pkg" 2>/dev/null | grep "^pkgver:" | cut -d: -f2 | xargs); desc=$(xbps-query "$pkg" 2>/dev/null | grep "^short_desc:" | cut -d: -f2- | xargs); echo -e "$pkg\t$ver\t$desc"; done | head -500"#
            }
            PackageManager::Emerge => {
                r#"qlist -Iv 2>/dev/null | head -500 | while read atom; do echo -e "$atom\t\t"; done"#
            }
            PackageManager::Unknown => {
                return Err(Error::CommandFailed {
                    exit_code: Some(1),
                    command: "detect_package_manager".into(),
                    stderr: "No supported package manager found in container".to_string(),
                });
            }
        };

        let output = self.run_in_container(container, script).await?;
        let mut out = TolerantList::default();

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() >= 2 {
                out.items.push(PackageInfo {
                    name: parts[0].trim().to_string(),
                    version: parts[1].trim().to_string(),
                    description: parts.get(2).unwrap_or(&"").trim().to_string(),
                    installed: true,
                });
            } else {
                // B3: every PM script above tab-separates name/version, so a
                // tab-less row is usually a one-line error message the script
                // leaked onto stdout — reported, not silently eaten.
                warn!(line = %line, "Skipping package row with no tab separator");
                out.skipped.push(ParseIssue {
                    line: line.to_string(),
                    error: "expected '<name>\\t<version>\\t<description>'".to_string(),
                });
            }
        }

        Ok(out)
    }

    /// Search for packages in a container
    pub async fn search_packages(
        &self,
        container: &str,
        query: &str,
    ) -> Result<Vec<PackageInfo>, Error> {
        use crate::models::PackageManager;
        let pkg_manager = self.detect_package_manager(container).await?;
        let query_escaped = query.replace("'", "'\\''");

        let script = match pkg_manager {
            PackageManager::Apt => format!(
                r#"apt-cache search '{}' 2>/dev/null | head -100 | while read name rest; do echo -e "$name\t\t$rest"; done"#,
                query_escaped
            ),
            PackageManager::Dnf => format!(
                r#"dnf search '{}' 2>/dev/null | grep -v "^=" | grep -v "^Last metadata" | head -100 | sed 's/\..*:/\t\t/'"#,
                query_escaped
            ),
            PackageManager::Pacman => format!(
                r#"pacman -Ss '{}' 2>/dev/null | grep -v "^    " | head -100 | sed 's|/| |' | while read repo name ver; do echo -e "$name\t$ver\t"; done"#,
                query_escaped
            ),
            PackageManager::Zypper => format!(
                r#"zypper search '{}' 2>/dev/null | tail -n +4 | head -100 | awk -F'|' '{{print $2"\t"$4"\t"$3}}'"#,
                query_escaped
            ),
            PackageManager::Apk => format!(
                r#"apk search -d '{}' 2>/dev/null | head -100 | while read pkg desc; do name=$(echo "$pkg" | cut -d- -f1); ver=$(echo "$pkg" | cut -d- -f2-); echo -e "$name\t$ver\t$desc"; done"#,
                query_escaped
            ),
            PackageManager::Xbps => format!(
                r#"xbps-query -Rs '{}' 2>/dev/null | head -100 | awk '{{print $2"\t"$1"\t"}}'"#,
                query_escaped
            ),
            PackageManager::Emerge => format!(
                r#"emerge --search '{}' 2>/dev/null | grep "^\*" | head -100 | sed 's/^\* *//;s/ *\[.*//'"#,
                query_escaped
            ),
            PackageManager::Unknown => {
                return Err(Error::CommandFailed {
                    exit_code: Some(1),
                    command: "search_packages".into(),
                    stderr: "No supported package manager found in container".to_string(),
                });
            }
        };

        let output = self.run_in_container(container, &script).await?;
        let mut packages = Vec::new();

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if !parts.is_empty() && !parts[0].trim().is_empty() {
                packages.push(PackageInfo {
                    name: parts[0].trim().to_string(),
                    version: parts.get(1).unwrap_or(&"").trim().to_string(),
                    description: parts.get(2).unwrap_or(&"").trim().to_string(),
                    installed: false,
                });
            }
        }

        Ok(packages)
    }

    /// Install a package in a container (returns Child for streaming output).
    /// B1: detection runs through the shared script; the verb comes from the
    /// single `PackageManager` table (`install_verb` — emerge `--ask=n`).
    pub fn install_package(
        &self,
        container: &str,
        package: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // Sync context (returns Child, not async): detect inside the script
        // via the shared words, then dispatch on the single table. The words
        // match `detect_script` one-to-one so behaviour cannot diverge.
        let package_escaped = package.replace("'", "'\\''");
        let install_script = format!(
            r#"
            PKG_MGR=$({})
            case "$PKG_MGR" in
                apt) sudo apt-get install -y '{}' ;;
                dnf|yum) sudo dnf install -y '{}' ;;
                pacman) sudo pacman -S --noconfirm '{}' ;;
                zypper) sudo zypper install -y '{}' ;;
                apk) sudo apk add '{}' ;;
                xbps) sudo xbps-install -y '{}' ;;
                emerge) sudo emerge --ask=n '{}' ;;
                *) echo "No supported package manager found in container" >&2; exit 1 ;;
            esac
        "#,
            Self::detect_script().trim(),
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped
        );

        self.run_in_container_streaming(container, &install_script)
    }

    /// Remove a package from a container (returns Child for streaming output).
    /// B1: same shared script + single `remove_verb` table as install.
    pub fn remove_package(
        &self,
        container: &str,
        package: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let package_escaped = package.replace("'", "'\\''");
        let remove_script = format!(
            r#"
            PKG_MGR=$({})
            case "$PKG_MGR" in
                apt) sudo apt-get remove -y '{}' ;;
                dnf|yum) sudo dnf remove -y '{}' ;;
                pacman) sudo pacman -R --noconfirm '{}' ;;
                zypper) sudo zypper remove -y '{}' ;;
                apk) sudo apk del '{}' ;;
                xbps) sudo xbps-remove -y '{}' ;;
                emerge) sudo emerge --unmerge '{}' ;;
                *) echo "No supported package manager found in container" >&2; exit 1 ;;
            esac
        "#,
            Self::detect_script().trim(),
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped,
            package_escaped
        );

        self.run_in_container_streaming(container, &remove_script)
    }

    // ============================================================================
    // Snapshot/Backup APIs (via podman/docker)
    // ============================================================================

    /// Run `cmd` against podman, falling back to docker per `trigger`
    /// (architecture.md §6.2, which names this helper, and §6.4 row B7). This
    /// is the "one place" the row asks for: the
    /// six hand-rolled pairs in this file disagreed on trigger, on argv, and
    /// on whether the result was trimmed, and now share one loop.
    ///
    /// The command is built ONCE for podman and re-aimed with
    /// `container_runtime::retarget`, which swaps only the program — so
    /// arguments, order and stdio modes cannot drift between the two
    /// attempts. The runner is always `self.cmd_runner` (the env-mapped one,
    /// `env.rs`): routing through a `Podman`-constructed runner would rewrite
    /// the docker retry straight back to podman, since `Podman::new` installs
    /// `map_docker_to_podman`.
    ///
    /// The raw string is returned; trimming stays with each caller, because
    /// the callers disagree about it (`create_snapshot` trims, `start` does
    /// not) and unifying that here would be a silent behaviour change.
    ///
    /// `trigger` decides what a FAILURE means, and the two triggers are not
    /// interchangeable — they reproduce what the six originals each did:
    ///
    /// * `OnError` (five sites): a failure means "wrong runtime", so the other
    ///   one is tried, and the last runtime's failure is what surfaces.
    /// * `OnEmpty` (`get_container_id` alone): a failure IS the answer. Its
    ///   original podman branch ended in `?`, so an error propagated and
    ///   docker was never consulted. Retrying there would swap a podman
    ///   diagnostic for a misleading "container not found", or hand back a
    ///   same-named container from a different runtime's store.
    async fn runtime_output(&self, cmd: Command, trigger: Fallback) -> Result<String, Error> {
        let mut last_error: Option<Error> = None;
        let last_runtime = PODMAN_FIRST[PODMAN_FIRST.len() - 1];
        for runtime in PODMAN_FIRST {
            let attempt = retarget(&cmd, runtime);
            match self.cmd_output_string(attempt).await {
                Ok(out) => {
                    // `OnEmpty` is the "this runtime is working, it just does
                    // not know the name" case: an empty answer is worth
                    // retrying only while another runtime remains to ask.
                    if trigger == Fallback::OnEmpty
                        && out.trim().is_empty()
                        && runtime != last_runtime
                    {
                        debug!(
                            runtime = runtime.program(),
                            "runtime produced no output; trying the next one"
                        );
                        continue;
                    }
                    return Ok(out);
                }
                Err(e) => {
                    if trigger == Fallback::OnEmpty {
                        return Err(e);
                    }
                    if runtime == last_runtime {
                        last_error = Some(e);
                    } else {
                        debug!(
                            error = %e,
                            runtime = runtime.program(),
                            "runtime failed; trying the next one"
                        );
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| Error::CommandFailed {
            exit_code: None,
            command: "runtime_output".into(),
            stderr: "no container runtime available".into(),
        }))
    }

    /// Get the container ID for a distrobox container by name
    async fn get_container_id(&self, container_name: &str) -> Result<String, Error> {
        // `ps` BY NAME (podman/docker accept names, which avoids the
        // ID-namespace mismatch between runtimes); retry on docker only when
        // podman SUCCEEDED with nothing — see `Fallback::OnEmpty`.
        let mut cmd = Command::new("podman");
        cmd.args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{}$", container_name),
            "--format",
            "{{.ID}}",
        ]);

        let id = self
            .runtime_output(cmd, Fallback::OnEmpty)
            .await?
            .trim()
            .to_string();

        if id.is_empty() {
            return Err(Error::CommandFailed {
                exit_code: Some(1),
                command: "get_container_id".into(),
                stderr: format!("Container not found: {}", container_name),
            });
        }
        Ok(id)
    }

    /// Create a snapshot (image) of a container using podman/docker commit
    pub async fn create_snapshot(
        &self,
        container_name: &str,
        snapshot_name: &str,
    ) -> Result<String, Error> {
        let container_id = self.get_container_id(container_name).await?;

        // By ID — resolved above, so both runtimes commit the same object.
        //
        // The one intentional behaviour change B7 makes: the original trimmed
        // ONLY the podman branch and returned the docker retry's output raw.
        // The unified helper trims whatever comes back, so a trailing newline
        // from the docker path now goes away too. Recorded here rather than
        // buried, because it is the single place the refactor is not exactly
        // behaviour-preserving — and a stray newline in an image ID is not
        // worth a second code path to reproduce.
        let mut cmd = Command::new("podman");
        cmd.args(["commit", &container_id, snapshot_name]);
        Ok(self
            .runtime_output(cmd, Fallback::OnError)
            .await?
            .trim()
            .to_string())
    }

    /// List snapshots (images) created from containers
    pub async fn list_snapshots(
        &self,
        filter_prefix: Option<&str>,
    ) -> Result<TolerantList<SnapshotInfo>, Error> {
        // List images with podman, filter by optional prefix
        let filter = filter_prefix
            .map(|p| format!("reference={}*", p))
            .unwrap_or_default();

        // Built once, including the optional filter; the docker retry used to
        // rebuild this argv from scratch and had to keep the filter clause in
        // step by hand.
        let mut cmd = Command::new("podman");
        cmd.args([
            "images",
            "--format",
            "{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Created}}\t{{.Size}}",
        ]);
        if !filter.is_empty() {
            cmd.args(["--filter", &filter]);
        }

        let output = self.runtime_output(cmd, Fallback::OnError).await?;

        let mut out = TolerantList::default();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                out.items.push(SnapshotInfo {
                    id: parts[0].to_string(),
                    name: parts[1].to_string(),
                    created: parts[2].to_string(),
                    size: parts[3].to_string(),
                });
            } else {
                // B3: `--format` above asks for exactly four tab-separated
                // fields, so a shorter row means the format did not take —
                // reported rather than silently dropped.
                warn!(line = %line, "Skipping snapshot row with fewer than 4 fields");
                out.skipped.push(ParseIssue {
                    line: line.to_string(),
                    error: "expected '<id>\\t<name>\\t<created>\\t<size>'".to_string(),
                });
            }
        }

        Ok(out)
    }

    /// Delete a snapshot (image)
    pub async fn delete_snapshot(&self, snapshot_name_or_id: &str) -> Result<String, Error> {
        let mut cmd = Command::new("podman");
        cmd.args(["rmi", snapshot_name_or_id]);
        self.runtime_output(cmd, Fallback::OnError).await
    }

    /// Restore a container from a snapshot by creating a new container from the image
    pub async fn restore_from_snapshot(
        &self,
        snapshot_name: &str,
        new_container_name: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // Create a new distrobox container from the snapshot image
        let args = CreateArgs {
            image: snapshot_name.to_string(),
            name: CreateArgName::new(new_container_name)?,
            ..Default::default()
        };
        self.create(args).await
    }

    // ============================================================================
    // Container Export/Import APIs
    // ============================================================================

    /// Export a container to a tar archive (returns Child for streaming)
    ///
    /// B7 DECISION: this stays podman-literal (no docker fallback), like
    /// `import_container` below and unlike the six sites that now route
    /// through `runtime_output`.
    ///
    /// Both are STREAMING (`cmd_spawn` → a `Child` whose output the task
    /// registry reads), and `runtime_output` is an OUTPUT helper — it returns
    /// a `String` and has no way to hand back a live child, so it cannot serve
    /// them at all. A streaming fallback would have to spawn, detect the
    /// failure, and respawn, which is a task-runtime concern rather than an
    /// argv one. Nothing in the row's scope requires it, and inventing it here
    /// would be the kind of unrequested behaviour change B7 is trying to avoid.
    pub fn export_container(
        &self,
        container_name: &str,
        output_path: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // Use podman export
        let mut cmd = Command::new("podman");
        cmd.args(["export", "-o", output_path, container_name]);
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;
        self.cmd_spawn(cmd)
    }

    /// Import a container from a tar archive (returns Child for streaming).
    /// Podman-literal — see `export_container` for the B7 reasoning.
    pub fn import_container(
        &self,
        archive_path: &str,
        image_name: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // Use podman import
        let mut cmd = Command::new("podman");
        cmd.args(["import", archive_path, image_name]);
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;
        self.cmd_spawn(cmd)
    }

    // ============================================================================
    // Resource Monitoring APIs
    // ============================================================================

    /// Get resource usage statistics for a container
    pub async fn get_container_stats(&self, container_name: &str) -> Result<ContainerStats, Error> {
        let mut cmd = Command::new("podman");
        cmd.args([
            "stats",
            "--no-stream",
            "--format",
            "{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}\t{{.NetIO}}\t{{.BlockIO}}",
            container_name,
        ]);

        let output = self.runtime_output(cmd, Fallback::OnError).await?;

        let line = output.lines().next().unwrap_or_default();
        let parts: Vec<&str> = line.split('\t').collect();

        if parts.len() >= 5 {
            // Parse CPU percentage (e.g., "5.25%")
            let cpu_str = parts[0].trim_end_matches('%');
            let cpu_percent = cpu_str.parse::<f64>().unwrap_or(0.0);

            // Parse memory usage (e.g., "256MiB / 8GiB")
            let mem_parts: Vec<&str> = parts[1].split('/').collect();
            let memory_usage = mem_parts.first().unwrap_or(&"0").trim().to_string();
            let memory_limit = mem_parts.get(1).unwrap_or(&"0").trim().to_string();

            // Parse memory percentage
            let mem_perc_str = parts[2].trim_end_matches('%');
            let memory_percent = mem_perc_str.parse::<f64>().unwrap_or(0.0);

            Ok(ContainerStats {
                cpu_percent,
                memory_usage,
                memory_limit,
                memory_percent,
                network_io: parts[3].to_string(),
                block_io: parts[4].to_string(),
            })
        } else {
            Ok(ContainerStats::default())
        }
    }
}

impl Default for Distrobox {
    fn default() -> Self {
        Self::new(CommandRunner::new_null(), default_cmd_factory())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::{CommandRunnerEvent, OutputTracker};
    use smol::block_on;

    /// Helper to generate TOML output matching the shell script format
    fn make_desktop_files_toml(
        home_dir: &str,
        system_files: &[(&str, &str)],
        user_files: &[(&str, &str)],
    ) -> String {
        let mut toml = format!("home_dir=\"{}\"\n", to_hex(home_dir));

        toml.push_str("[system]\n");
        for (path, content) in system_files {
            toml.push_str(&format!("\"{}\"=\"{}\"\n", to_hex(path), to_hex(content)));
        }

        toml.push_str("[user]\n");
        for (path, content) in user_files {
            toml.push_str(&format!("\"{}\"=\"{}\"\n", to_hex(path), to_hex(content)));
        }

        toml
    }

    #[test]
    fn list() -> Result<(), Error> {
        block_on(async {
            let output = "ID           | NAME                 | STATUS             | IMAGE                         
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(&["distrobox", "ls", "--no-color"], output)
                    .build(),
                default_cmd_factory(),
            );
            let got = db.list().await?;
            assert_eq!(
                got.containers,
                vec![ContainerInfo {
                    id: "d24405b14180".into(),
                    name: "ubuntu".into(),
                    status: Status::Created("".into()),
                    image: "ghcr.io/ublue-os/ubuntu-toolbox:latest".into(),
                }]
            );
            assert!(got.skipped.is_empty(), "a clean fixture skips nothing");
            Ok(())
        })
    }

    /// B3's whole point: one malformed row must not discard its well-formed
    /// neighbours, and must be *reported* rather than silently dropped.
    ///
    /// This is the fixture I23 called for. Every other `list()` test — and every
    /// app-level one — drives `DistroboxCommandRunnerResponse::List`, which
    /// renders well-formed rows from `ContainerInfo`s, so `skipped` is
    /// structurally empty and an `is_empty()` assertion passes whether or not
    /// the code under test actually preserves it. Here the stdout is raw, so a
    /// genuinely unparseable row crosses the parser for real.
    #[test]
    fn list_reports_unparseable_rows_instead_of_dropping_them() -> Result<(), Error> {
        block_on(async {
            // Real distrobox shape: a header, two rows that parse, and one that
            // cannot (its columns were split by a `|` inside an image tag).
            let output = "\
ID           | NAME                 | STATUS             | IMAGE
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest
77aa11cc22dd | broken               | Created
9008f7e6d5c4 | fedora               | Up 2 hours         | registry.fedoraproject.org/fedora:39";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(&["distrobox", "ls", "--no-color"], output)
                    .build(),
                default_cmd_factory(),
            );

            let got = db.list().await?;

            // Both good rows survive, name-sorted.
            let names: Vec<&str> = got.containers.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(
                names,
                vec!["fedora", "ubuntu"],
                "one bad row must not cost the good ones"
            );
            // And the bad one is reported, not swallowed.
            assert_eq!(got.skipped.len(), 1, "the malformed row must be counted");
            assert!(
                got.skipped[0].line.contains("77aa11cc22dd"),
                "the issue must quote the offending line, got {:?}",
                got.skipped[0].line
            );
            assert!(
                !got.skipped[0].error.is_empty(),
                "the issue must carry why it failed"
            );
            Ok(())
        })
    }

    #[test]
    fn version() -> Result<(), Error> {
        block_on(async {
            let output = "distrobox: 1.7.2.1";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(&["distrobox", "version"], output)
                    .build(),
                default_cmd_factory(),
            );
            assert_eq!(db.version().await?, "1.7.2.1".to_string(),);
            Ok(())
        })
    }

    #[test]
    fn list_apps() -> Result<(), Error> {
        let vim_desktop = "[Desktop Entry]
Type=Application
Name=Vim
Exec=/path/to/vim
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;";

        let fish_desktop = "[Desktop Entry]
Type=Application
Name=Fish
Exec=/path/to/fish
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;";

        let desktop_files_toml = make_desktop_files_toml(
            "/home/me",
            &[
                ("/usr/share/applications/vim.desktop", vim_desktop),
                ("/usr/share/applications/fish.desktop", fish_desktop),
            ],
            &[],
        );

        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd(&["printenv", "HOME"], "/home/me")
                .cmd(&["printenv", "XDG_DATA_HOME"], "")
                .cmd(&["printenv", "HOME"], "/home/me")
                .cmd(
                    &["ls", "/home/me/.local/share/applications"],
                    "ubuntu-vim.desktop\n",
                )
                .cmd(
                    &[
                        "distrobox",
                        "enter",
                        "ubuntu",
                        "--",
                        "sh",
                        "-c",
                        POSIX_FIND_AND_CONCAT_DESKTOP_FILES,
                    ],
                    &desktop_files_toml,
                )
                .build(),
            default_cmd_factory(),
        );

        let apps = block_on(db.list_apps("ubuntu"))?;
        assert_eq!(&apps[0].entry.name, "Fish");
        assert_eq!(&apps[0].entry.exec, "/path/to/fish");
        assert!(!apps[0].exported);
        assert_eq!(&apps[1].entry.name, "Vim");
        assert_eq!(&apps[1].entry.exec, "/path/to/vim");
        assert!(apps[1].exported);
        Ok(())
    }

    #[test]
    fn list_apps_with_space_in_filename() -> Result<(), Error> {
        // Simulate a desktop file with a space in its filename and ensure it's parsed/export-detected correctly
        let proton_desktop = "[Desktop Entry]
Type=Application
Name=Proton Authenticator
Exec=/usr/bin/proton-authenticator %u
Icon=proton-authenticator
Categories=Utility;Security;";

        let desktop_files_toml = make_desktop_files_toml(
            "/home/me",
            &[(
                "/usr/share/applications/Proton Authenticator.desktop",
                proton_desktop,
            )],
            &[],
        );

        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd(&["printenv", "HOME"], "/home/me")
                .cmd(&["printenv", "XDG_DATA_HOME"], "")
                .cmd(&["printenv", "HOME"], "/home/me")
                .cmd(
                    &["ls", "/home/me/.local/share/applications"],
                    "ubuntu-Proton Authenticator.desktop\n",
                )
                .cmd(
                    &[
                        "distrobox",
                        "enter",
                        "ubuntu",
                        "--",
                        "sh",
                        "-c",
                        POSIX_FIND_AND_CONCAT_DESKTOP_FILES,
                    ],
                    &desktop_files_toml,
                )
                .build(),
            default_cmd_factory(),
        );

        let apps = block_on(db.list_apps("ubuntu"))?;
        assert_eq!(apps.len(), 1);
        assert_eq!(&apps[0].entry.name, "Proton Authenticator");
        assert_eq!(&apps[0].entry.exec, "/usr/bin/proton-authenticator %u");
        assert_eq!(
            &apps[0].desktop_file_path,
            "/usr/share/applications/Proton Authenticator.desktop"
        );
        // Ensure exported detection matches the filename with space
        assert!(apps[0].exported);
        Ok(())
    }
    #[test]
    fn create() -> Result<(), Error> {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let output_tracker = db.cmd_runner.output_tracker();
        debug!("Testing container creation");
        let args = CreateArgs {
            image: "docker.io/library/ubuntu:latest".into(),
            init: true,
            nvidia: true,
            home_path: Some("/home/me".into()),
            volumes: vec![
                Volume::from_str("/mnt/sdb1:/mnt/sdb1")?,
                Volume::from_str("/mnt/sdb4:/mnt/sdb4:ro")?,
            ],
            ..Default::default()
        };
        smol::block_on(db.create(args))?;
        let expected = "distrobox create --yes --image docker.io/library/ubuntu:latest --init --additional-packages systemd --nvidia --home /home/me --volume /mnt/sdb1:/mnt/sdb1 --volume /mnt/sdb4:/mnt/sdb4:ro";
        assert_eq!(
            output_tracker.items()[0].command().unwrap().to_string(),
            expected
        );
        Ok(())
    }
    #[test]
    fn assemble() -> Result<(), Error> {
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let output_tracker = db.cmd_runner.output_tracker();
        db.assemble("/path/to/assemble.yml")?;
        assert_eq!(
            output_tracker.items()[0].command().unwrap().to_string(),
            "distrobox assemble create --file /path/to/assemble.yml"
        );
        Ok(())
    }

    #[test]
    fn remove() -> Result<(), Error> {
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let output_tracker = db.cmd_runner.output_tracker();
        block_on(db.remove("ubuntu"))?;
        assert_eq!(
            output_tracker.items()[0].command().unwrap().to_string(),
            "distrobox rm --force ubuntu"
        );
        Ok(())
    }

    #[test]
    fn start_sends_podman_start_argv() -> Result<(), Error> {
        // B5: `podman start <name>` (there is no `distrobox start`
        // subcommand); the docker fallback is covered by
        // `start_falls_back_to_docker_when_podman_fails`.
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let output_tracker = db.cmd_runner.output_tracker();
        block_on(db.start("ubuntu"))?;
        // Positional AND exhaustive: reading `items()[0]` alone would still
        // pass if a retry had fired behind it. A null runner answers every
        // command with success, so a correct implementation stops after one.
        let seen: Vec<_> = output_tracker
            .items()
            .iter()
            .filter_map(|e| e.command().map(|c| c.to_string()))
            .collect();
        assert_eq!(seen, vec!["podman start ubuntu".to_string()]);
        Ok(())
    }

    /// B4: the argv a `launch_app` call actually hands the runner, as strings.
    fn launch_argv(exec: &str) -> Vec<String> {
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let output_tracker = db.cmd_runner.output_tracker();
        let app = ExportableApp {
            entry: DesktopEntry {
                name: "App".into(),
                exec: exec.into(),
                icon: "app".into(),
            },
            desktop_file_path: "/tmp/app.desktop".into(),
            exported: false,
        };
        db.launch_app("ubuntu", &app).expect("spawn succeeds");
        output_tracker.items()[0]
            .command()
            .expect("a command was spawned")
            .args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    /// B4 regression: the Exec becomes real argv elements. Before this the
    /// whole cleaned string was ONE argument, so `--title "My Document"`
    /// reached distrobox fused as `--title "My Document"` in a single slot
    /// and the declared boundary (and the quoted space) were at the mercy of
    /// distrobox's own re-split.
    #[test]
    fn launch_app_splits_exec_into_argv_elements() {
        assert_eq!(
            launch_argv("/usr/bin/foo --title \"My Document\" %u"),
            vec![
                "enter",
                "--name",
                "ubuntu",
                "--",
                "/usr/bin/foo",
                "--title",
                "My Document"
            ]
        );
    }

    /// The security-shaped pin: shell metacharacters in a container-supplied
    /// Exec are inert characters inside one element. Nothing here is a shell
    /// word and no element is ever re-split.
    #[test]
    fn launch_app_keeps_metacharacters_inside_one_element() {
        assert_eq!(
            launch_argv("/usr/bin/foo \"bar; rm -rf /\" %u"),
            vec![
                "enter",
                "--name",
                "ubuntu",
                "--",
                "/usr/bin/foo",
                "bar; rm -rf /"
            ]
        );
    }

    /// A code at offset 0 (which `parse_desktop_file`'s trim makes the normal
    /// shape for `%U firefox`) used to survive into argv, because the old
    /// needles all began with a space.
    #[test]
    fn launch_app_strips_a_leading_field_code() {
        assert_eq!(
            launch_argv("%U firefox"),
            vec!["enter", "--name", "ubuntu", "--", "firefox"]
        );
    }

    /// `%i` is dropped outright and introduces no phantom argument, and a
    /// field-code-only Exec contributes no empty slot.
    #[test]
    fn launch_app_drops_field_codes_without_adding_arguments() {
        assert_eq!(
            launch_argv("/usr/bin/foo %i --evil"),
            vec!["enter", "--name", "ubuntu", "--", "/usr/bin/foo", "--evil"]
        );
    }

    /// An `Exec` with nothing left after field-code removal is a refusal, not
    /// an interactive shell. Draining it to a bare `enter --name <box> --`
    /// would silently hand the user a shell in the container where they asked
    /// to launch an app — a wrong action taken without a word. The signature
    /// can report this, so it does.
    #[test]
    fn launch_app_refuses_an_exec_with_no_command_left() {
        let db = Distrobox::new(CommandRunner::new_null(), default_cmd_factory());
        let tracker = db.cmd_runner.output_tracker();
        let app = ExportableApp {
            entry: DesktopEntry {
                name: "Ghost".into(),
                exec: "%u".into(),
                icon: "ghost".into(),
            },
            desktop_file_path: "/tmp/ghost.desktop".into(),
            exported: false,
        };
        // `.err()` rather than `.expect_err()`: the Ok type is `Box<dyn Child>`,
        // which is not `Debug`.
        let err = db
            .launch_app("ubuntu", &app)
            .err()
            .expect("no command → Err");
        assert!(
            err.to_string().contains("Ghost"),
            "the refusal names the entry: {err}"
        );
        assert!(
            tracker.items().is_empty(),
            "nothing may be spawned for a refused launch"
        );
        // The same for an Exec that is empty or only whitespace.
        for exec in ["", "   "] {
            let mut empty = app.clone();
            empty.entry.exec = exec.into();
            assert!(
                db.launch_app("ubuntu", &empty).is_err(),
                "{exec:?} must not spawn a shell"
            );
        }
    }

    #[test]
    fn stub_responses() {
        let cmd_outputs = DistroboxCommandRunnerResponse::new_list_common_distros().to_commands();
        assert_eq!(
            cmd_outputs[0].1().unwrap(),
            "ID           | NAME                 | STATUS             | IMAGE  
1 | Ubuntu | Created 2 minutes ago | docker.io/library/ubuntu:latest
2 | Fedora | Created 2 minutes ago | docker.io/library/fedora:latest
3 | Kali | Created 2 minutes ago | docker.io/kalilinux/kali-rolling
4 | Debian | Created 2 minutes ago | docker.io/library/debian:latest
5 | Arch Linux | Created 2 minutes ago | docker.io/library/archlinux:latest
6 | CentOS | Created 2 minutes ago | docker.io/library/centos:latest
7 | Alpine | Created 2 minutes ago | docker.io/library/alpine:latest
8 | OpenSUSE | Created 2 minutes ago | docker.io/library/opensuse:latest
9 | Gentoo | Created 2 minutes ago | docker.io/library/gentoo:latest
10 | Slackware | Created 2 minutes ago | docker.io/library/slackware:latest
11 | Void Linux | Created 2 minutes ago | docker.io/library/voidlinux:latest
13 | Deepin | Created 2 minutes ago | docker.io/library/deepin:latest
16 | Rocky Linux | Created 2 minutes ago | docker.io/library/rockylinux:latest
17 | Crystal Linux | Created 2 minutes ago | docker.io/library/crystal-linux:latest\n"
        );
    }

    #[test]
    fn stub_exported_apps_generates_valid_toml() {
        // Verify that new_common_exported_apps generates valid TOML that can be parsed
        let exported_apps = DistroboxCommandRunnerResponse::new_common_exported_apps();
        let commands = exported_apps.to_commands();

        // Find the command that should contain TOML output (distrobox enter ... sh -c ...)
        let toml_command = commands
            .iter()
            .find(|(cmd, _)| {
                cmd.program.to_string_lossy().contains("distrobox")
                    && cmd.args.iter().any(|arg| arg.to_string_lossy() == "enter")
            })
            .expect("Should have a TOML-generating command");

        let toml_output = toml_command.1().expect("Should generate output");

        // Verify the TOML is parseable
        let desktop_files: DesktopFiles =
            toml::from_str(&toml_output).expect("Generated TOML should be valid and parseable");

        // Verify home_dir is set
        assert_eq!(
            desktop_files.home_dir.to_string_lossy(),
            "/home/me",
            "home_dir should be /home/me"
        );

        // Verify we have system files (the mock apps should be in system)
        assert!(
            !desktop_files.system.is_empty(),
            "Should have system desktop files"
        );

        // Verify all system files are valid desktop entries
        for (path, content) in &desktop_files.system {
            assert!(
                path.to_string_lossy().ends_with(".desktop"),
                "Path should end with .desktop: {:?}",
                path
            );
            assert!(
                content.contains("[Desktop Entry]"),
                "Content should be a valid desktop entry"
            );
            assert!(
                content.contains("Name="),
                "Content should have a Name field"
            );
        }
    }

    #[test]
    fn status_parsing() {
        // Test "Up" status with details
        assert_eq!(
            Status::from_str("Up 2 hours"),
            Status::Up("2 hours".to_string())
        );
        assert_eq!(
            Status::from_str("Up (Paused)"),
            Status::Up("(Paused)".to_string())
        );

        // Test "Created" status
        assert_eq!(
            Status::from_str("Created 5 minutes ago"),
            Status::Created("5 minutes ago".to_string())
        );

        // Test "Exited" status
        assert_eq!(
            Status::from_str("Exited (0) 10 seconds ago"),
            Status::Exited("(0) 10 seconds ago".to_string())
        );

        // Test unknown status falls back to Other
        assert_eq!(
            Status::from_str("Unknown status"),
            Status::Other("Unknown status".to_string())
        );

        // Test empty string
        assert_eq!(Status::from_str(""), Status::Other("".to_string()));
    }

    #[test]
    fn status_display() {
        assert_eq!(Status::Up("2 hours".to_string()).to_string(), "Up 2 hours");
        assert_eq!(
            Status::Created("5 minutes ago".to_string()).to_string(),
            "Created 5 minutes ago"
        );
        assert_eq!(
            Status::Exited("(0) 10 seconds ago".to_string()).to_string(),
            "Exited (0) 10 seconds ago"
        );
        assert_eq!(Status::Other("Unknown".to_string()).to_string(), "Unknown");
    }

    #[test]
    fn volume_parsing() -> Result<(), Error> {
        // Test single path (host only, container path same as host)
        let vol = Volume::from_str("/data")?;
        assert_eq!(vol.host_path, "/data");
        assert_eq!(vol.container_path, "/data");
        assert_eq!(vol.mode, None);

        // Test host:container path
        let vol = Volume::from_str("/host/path:/container/path")?;
        assert_eq!(vol.host_path, "/host/path");
        assert_eq!(vol.container_path, "/container/path");
        assert_eq!(vol.mode, None);

        // Test host:container:ro (read-only)
        let vol = Volume::from_str("/data:/data:ro")?;
        assert_eq!(vol.host_path, "/data");
        assert_eq!(vol.container_path, "/data");
        assert_eq!(vol.mode, Some(VolumeMode::ReadOnly));

        // Test invalid volume descriptor
        let result = Volume::from_str("/a:/b:/c:/d");
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn volume_display() {
        let vol = Volume {
            host_path: "/host".to_string(),
            container_path: "/container".to_string(),
            mode: None,
        };
        assert_eq!(vol.to_string(), "/host:/container");

        let vol_ro = Volume {
            host_path: "/host".to_string(),
            container_path: "/container".to_string(),
            mode: Some(VolumeMode::ReadOnly),
        };
        assert_eq!(vol_ro.to_string(), "/host:/container:ro");
    }

    #[test]
    fn container_info_parsing() -> Result<(), Error> {
        // Test valid container line with "Up" status
        let line = "abc123 | my-container | Up 5 hours | docker.io/library/ubuntu:latest";
        let info = ContainerInfo::from_str(line)?;
        assert_eq!(info.id, "abc123");
        assert_eq!(info.name, "my-container");
        assert_eq!(info.status, Status::Up("5 hours".to_string()));
        assert_eq!(info.image, "docker.io/library/ubuntu:latest");

        // Test container with "Created" status
        let line =
            "def456 | fedora | Created 2 minutes ago | ghcr.io/ublue-os/fedora-toolbox:latest";
        let info = ContainerInfo::from_str(line)?;
        assert_eq!(info.id, "def456");
        assert_eq!(info.name, "fedora");
        assert_eq!(info.status, Status::Created("2 minutes ago".to_string()));
        assert_eq!(info.image, "ghcr.io/ublue-os/fedora-toolbox:latest");

        // Test container with "Exited" status
        let line = "789ghi | arch | Exited (0) 1 day ago | docker.io/library/archlinux:latest";
        let info = ContainerInfo::from_str(line)?;
        assert_eq!(info.id, "789ghi");
        assert_eq!(info.name, "arch");
        assert_eq!(info.status, Status::Exited("(0) 1 day ago".to_string()));
        assert_eq!(info.image, "docker.io/library/archlinux:latest");

        Ok(())
    }

    #[test]
    fn container_info_parsing_errors() {
        // Too few fields
        let result = ContainerInfo::from_str("abc123 | my-container | Up");
        assert!(result.is_err());

        // Too many fields shouldn't happen in normal distrobox output, but test behavior
        let result = ContainerInfo::from_str("a | b | c | d | e");
        assert!(result.is_err());

        // Empty fields should fail
        let result = ContainerInfo::from_str(" | my-container | Up | image");
        assert!(result.is_err());

        let result = ContainerInfo::from_str("abc123 |  | Up | image");
        assert!(result.is_err());
    }

    // ---- B3: tolerant list parsing --------------------------------------

    const LS_HEADER: &str = "ID           | NAME                 | STATUS             | IMAGE";

    fn list_db(output: &str) -> Distrobox {
        Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd(&["distrobox", "ls", "--no-color"], output)
                .build(),
            default_cmd_factory(),
        )
    }

    /// The headline B3 fix: one malformed row must not discard the rows that
    /// parsed. Before this, `list()` logged the parse error and returned
    /// `Err`, so a single bad line turned a perfectly good container list
    /// into an error screen.
    #[test]
    fn list_keeps_good_rows_and_collects_bad_ones() -> Result<(), Error> {
        block_on(async {
            let output = format!(
                "{LS_HEADER}
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest
garbage-line-with-no-separators
aaa111bbb222 | fedora               | Up 2 hours         | registry.fedoraproject.org/fedora:40"
            );
            let got = list_db(&output).list().await?;
            assert_eq!(got.containers.len(), 2, "both good rows survive");
            // Name-sorted, matching what the old `BTreeMap` returned.
            assert_eq!(got.containers[0].name, "fedora");
            assert_eq!(got.containers[1].name, "ubuntu");
            assert_eq!(got.skipped.len(), 1);
            assert_eq!(got.skipped[0].line, "garbage-line-with-no-separators");
            assert!(
                !got.skipped[0].error.is_empty(),
                "a skip must carry why it was skipped"
            );
            Ok(())
        })
    }

    /// The other half: every row bad is still a success with an empty list,
    /// not an `Err` — the UI distinguishes this from a real failure (and
    /// from a genuinely empty account) to say "5 rows skipped".
    #[test]
    fn list_with_every_row_bad_is_empty_but_not_an_error() -> Result<(), Error> {
        block_on(async {
            let output = format!("{LS_HEADER}\nbad-one\nbad-two");
            let got = list_db(&output).list().await?;
            assert!(got.containers.is_empty());
            assert_eq!(got.skipped.len(), 2);
            assert!(!got.is_clean_empty(), "all-bad is not a clean empty");
            Ok(())
        })
    }

    /// A blank line is not a row, so it is neither parsed nor reported — a
    /// trailing newline must not read as a skipped row.
    #[test]
    fn list_ignores_blank_rows_without_reporting_them() -> Result<(), Error> {
        block_on(async {
            let output = format!(
                "{LS_HEADER}
d24405b14180 | ubuntu | Created | img

aaa111bbb222 | fedora | Up 2 hours | img2
"
            );
            let got = list_db(&output).list().await?;
            assert_eq!(got.containers.len(), 2);
            assert!(got.skipped.is_empty(), "blank lines are not parse issues");
            assert!(!got.is_clean_empty());
            Ok(())
        })
    }

    /// The header is skipped by identity, so a response WITHOUT one keeps its
    /// first row. `.skip(1)` (the old behaviour) dropped whatever line was
    /// first: here that is a real, perfectly parseable container, which
    /// vanished without being logged, counted, or shown anywhere.
    #[test]
    fn list_without_a_header_keeps_its_first_row() -> Result<(), Error> {
        block_on(async {
            let output = "\
d24405b14180 | ubuntu | Created | ghcr.io/ublue-os/ubuntu-toolbox:latest
aaa111bbb222 | fedora | Up 2 hours | registry.fedoraproject.org/fedora:40";
            let got = list_db(output).list().await?;
            assert_eq!(got.containers.len(), 2, "no row may be dropped silently");
            assert_eq!(got.containers[0].name, "fedora");
            assert_eq!(got.containers[1].name, "ubuntu");
            assert!(got.skipped.is_empty());
            Ok(())
        })
    }

    /// A header-shaped line is recognized wherever it appears, which also
    /// means a leading line that is NOT the header no longer shadows the real
    /// one (nor is it mistaken for the header and dropped).
    #[test]
    fn list_recognizes_the_header_at_any_position_and_keeps_a_leading_warning() -> Result<(), Error>
    {
        block_on(async {
            let output = format!(
                "WARN: some runtime notice
{LS_HEADER}
d24405b14180 | ubuntu | Created | ghcr.io/ublue-os/ubuntu-toolbox:latest"
            );
            let got = list_db(&output).list().await?;
            assert_eq!(got.containers.len(), 1);
            assert_eq!(got.containers[0].name, "ubuntu");
            // The warning line is not a container row, so it is reported
            // rather than discarded — B3's rule for anything unreadable.
            assert_eq!(got.skipped.len(), 1);
            assert_eq!(got.skipped[0].line, "WARN: some runtime notice");
            Ok(())
        })
    }

    /// The predicate matches the id field by equality, so a *data* row that
    /// merely starts with `ID` is a container, not a header. Getting this wrong
    /// would silently eat a real row — the failure mode this whole change
    /// exists to remove.
    #[test]
    fn header_predicate_does_not_match_a_container_whose_id_starts_with_id() -> Result<(), Error> {
        block_on(async {
            let output = format!("{LS_HEADER}\nID42 | Identifier | Created | img");
            assert!(!is_distrobox_header("ID42 | Identifier | Created | img"));
            let got = list_db(&output).list().await?;
            assert_eq!(got.containers.len(), 1);
            assert_eq!(got.containers[0].name, "Identifier");
            assert!(got.skipped.is_empty());
            Ok(())
        })
    }

    /// The header is recognized on the **six-column** form too, not just the
    /// four-column one this fixture usually carries.
    ///
    /// The column count is a release detail, not a format constant: distrobox
    /// 1.5.0.2 printed `ID | NAME | STATUS | MEM | CPU% | IMAGE` (line 192) and
    /// 1.6.0.1 onward print the four we see today. An earlier version of the
    /// predicate matched all four names, so on 1.5.x the header failed the test,
    /// fell through to `ContainerInfo::from_str`, and was counted as a skipped
    /// *container* — the caption then asserted an unreadable row that never
    /// existed, and inflated the count by one.
    #[test]
    fn header_predicate_recognizes_the_six_column_1_5_header() -> Result<(), Error> {
        block_on(async {
            let old_header = "ID           | NAME                 | STATUS             | MEM              | CPU%  | IMAGE";
            assert!(
                is_distrobox_header(old_header),
                "a 1.5.x header is a header, not a malformed container row"
            );
            let output = format!(
                "{old_header}
d24405b14180 | ubuntu | Created | ghcr.io/ublue-os/ubuntu-toolbox:latest"
            );
            let got = list_db(&output).list().await?;
            assert_eq!(got.containers.len(), 1);
            assert_eq!(got.containers[0].name, "ubuntu");
            assert!(
                got.skipped.is_empty(),
                "the header must not be reported as an unreadable row"
            );
            // A short header-shaped line is still not a header (arity floor).
            assert!(!is_distrobox_header("ID | NAME"));
            Ok(())
        })
    }

    /// A short row (fewer than four fields) and an empty-field row are both
    /// reported with their raw text.
    #[test]
    fn list_reports_short_and_empty_field_rows() -> Result<(), Error> {
        block_on(async {
            let output = format!(
                "{LS_HEADER}
d24405b14180 | ubuntu | Created
aaa111bbb222 |  | Up 2 hours | img2"
            );
            let got = list_db(&output).list().await?;
            assert!(got.containers.is_empty());
            assert_eq!(got.skipped.len(), 2);
            assert_eq!(got.skipped[0].line, "d24405b14180 | ubuntu | Created");
            assert_eq!(got.skipped[1].line, "aaa111bbb222 |  | Up 2 hours | img2");
            assert!(got.skipped[1].error.contains("name"));
            Ok(())
        })
    }

    /// The peers' delimiter drops are reported too. This row has no `|` at
    /// all, which used to be a bare `continue`.
    #[test]
    fn get_exported_binaries_collects_rows_without_a_separator() -> Result<(), Error> {
        block_on(async {
            let output = "'/usr/bin/vim'       | /home/user/.local/bin/vim\njust-a-bare-line";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "test-box",
                            "--",
                            "distrobox-export",
                            "--list-binaries",
                        ],
                        output,
                    )
                    .build(),
                default_cmd_factory(),
            );
            let got = db.get_exported_binaries("test-box").await?;
            assert_eq!(got.items.len(), 1);
            assert_eq!(got.items[0].name, "vim");
            assert_eq!(got.skipped.len(), 1);
            assert_eq!(got.skipped[0].line, "just-a-bare-line");
            Ok(())
        })
    }

    /// `list_installed_packages` needs two canned commands (the PM probe,
    /// then the list itself). The second script literal mirrors the
    /// `PackageManager::Apt` arm in the function under test.
    #[test]
    fn list_installed_packages_collects_rows_without_a_tab() -> Result<(), Error> {
        block_on(async {
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "--name",
                            "test-box",
                            "--",
                            "sh",
                            "-c",
                            Distrobox::detect_script(),
                        ],
                        "apt",
                    )
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "--name",
                            "test-box",
                            "--",
                            "sh",
                            "-c",
                            r#"dpkg-query -W -f='${Package}\t${Version}\t${Description}\n' 2>/dev/null | head -500"#,
                        ],
                        "vim\t9.0\thello\nthis-row-has-no-tab-at-all",
                    )
                    .build(),
                default_cmd_factory(),
            );
            let got = db.list_installed_packages("test-box").await?;
            assert_eq!(got.items.len(), 1);
            assert_eq!(got.items[0].name, "vim");
            assert_eq!(got.items[0].version, "9.0");
            assert_eq!(got.skipped.len(), 1);
            assert_eq!(got.skipped[0].line, "this-row-has-no-tab-at-all");
            Ok(())
        })
    }

    /// A row shorter than the four fields `--format` asks for.
    #[test]
    fn list_snapshots_collects_rows_with_fewer_than_four_fields() -> Result<(), Error> {
        block_on(async {
            let output = "a1b2c3\tgdm-mybox\tyesterday\t1.2 GB\nonly-two\tfields";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "podman",
                            "images",
                            "--format",
                            "{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Created}}\t{{.Size}}",
                        ],
                        output,
                    )
                    .build(),
                default_cmd_factory(),
            );
            let got = db.list_snapshots(None).await?;
            assert_eq!(got.items.len(), 1);
            assert_eq!(got.items[0].name, "gdm-mybox");
            assert_eq!(got.skipped.len(), 1);
            assert_eq!(got.skipped[0].line, "only-two\tfields");
            Ok(())
        })
    }

    /// `list_apps` already warned-and-continued; B3 only routes that record
    /// into `skipped`. The unparseable desktop file is identified by PATH,
    /// since there is no source line.
    #[test]
    fn list_apps_collects_unparseable_desktop_files() -> Result<(), Error> {
        block_on(async {
            let toml = make_desktop_files_toml(
                "/home/me",
                &[
                    (
                        "/usr/share/applications/good.desktop",
                        "[Desktop Entry]\nType=Application\nName=Good\nExec=/usr/bin/good\nIcon=good",
                    ),
                    (
                        "/usr/share/applications/bad.desktop",
                        "not a desktop file at all",
                    ),
                ],
                &[],
            );
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(&["printenv", "HOME"], "/home/me")
                    .cmd(&["printenv", "XDG_DATA_HOME"], "")
                    .cmd(&["printenv", "HOME"], "/home/me")
                    .cmd(
                        &["ls", "/home/me/.local/share/applications"],
                        "ubuntu-vim.desktop\n",
                    )
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "ubuntu",
                            "--",
                            "sh",
                            "-c",
                            POSIX_FIND_AND_CONCAT_DESKTOP_FILES,
                        ],
                        &toml,
                    )
                    .build(),
                default_cmd_factory(),
            );
            let got = db.list_apps("ubuntu").await?;
            assert_eq!(got.items.len(), 1, "the good entry survives");
            assert_eq!(got.items[0].entry.name, "Good");
            assert_eq!(got.skipped.len(), 1);
            assert!(
                got.skipped[0].line.ends_with("bad.desktop"),
                "a desktop-file skip is identified by path, got: {}",
                got.skipped[0].line
            );
            Ok(())
        })
    }

    #[test]
    fn get_exported_binaries_parses_normal_output() -> Result<(), Error> {
        block_on(async {
            // Normal output with source path present
            let list_output = "'/usr/bin/vim'       | /home/user/.local/bin/vim\n'/usr/bin/htop'      | /home/user/.local/bin/htop";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "test-box",
                            "--",
                            "distrobox-export",
                            "--list-binaries",
                        ],
                        list_output,
                    )
                    .build(),
                default_cmd_factory(),
            );
            let binaries = db.get_exported_binaries("test-box").await?;
            assert_eq!(binaries.len(), 2);
            assert_eq!(binaries[0].name, "vim");
            assert_eq!(binaries[0].source_path, "/usr/bin/vim");
            assert_eq!(binaries[0].exported_path, "/home/user/.local/bin/vim");
            assert_eq!(binaries[1].name, "htop");
            assert_eq!(binaries[1].source_path, "/usr/bin/htop");
            Ok(())
        })
    }

    #[test]
    fn get_exported_binaries_handles_empty_source_path() -> Result<(), Error> {
        block_on(async {
            // Output with empty source path (distrobox bug when sudo_prefix is empty)
            // In this case, the wrapper script should be read to extract the actual path
            let list_output = "                    | /home/user/.local/bin/nvim";
            let wrapper_content = r#"#!/bin/sh
# distrobox_binary
# name: archlinux
if [ -z "${CONTAINER_ID}" ]; then
	exec "distrobox-enter" -n archlinux -- '/usr/bin/nvim' "$@"
elif [ -n "${CONTAINER_ID}" ] && [ "${CONTAINER_ID}" != "archlinux" ]; then
	exec distrobox-host-exec '/home/user/.local/bin/nvim' "$@"
else
	exec '/usr/bin/nvim' "$@"
fi"#;
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "archlinux",
                            "--",
                            "distrobox-export",
                            "--list-binaries",
                        ],
                        list_output,
                    )
                    .cmd(&["cat", "/home/user/.local/bin/nvim"], wrapper_content)
                    .build(),
                default_cmd_factory(),
            );
            let binaries = db.get_exported_binaries("archlinux").await?;
            assert_eq!(binaries.len(), 1);
            assert_eq!(binaries[0].name, "nvim");
            assert_eq!(binaries[0].source_path, "/usr/bin/nvim");
            assert_eq!(binaries[0].exported_path, "/home/user/.local/bin/nvim");
            Ok(())
        })
    }

    #[test]
    fn get_exported_binaries_fallback_to_exported_path_name() -> Result<(), Error> {
        block_on(async {
            // Output with empty source path and wrapper script that can't be read
            let list_output = "                    | /home/user/.local/bin/my-tool";
            let db = Distrobox::new(
                NullCommandRunnerBuilder::new()
                    .cmd(
                        &[
                            "distrobox",
                            "enter",
                            "test-box",
                            "--",
                            "distrobox-export",
                            "--list-binaries",
                        ],
                        list_output,
                    )
                    // No cat command registered, so it will fail to read the wrapper
                    .build(),
                default_cmd_factory(),
            );
            let binaries = db.get_exported_binaries("test-box").await?;
            assert_eq!(binaries.len(), 1);
            // Should fallback to extracting name from exported_path
            assert_eq!(binaries[0].name, "my-tool");
            // source_path will be same as exported_path when wrapper can't be read
            assert_eq!(binaries[0].source_path, "/home/user/.local/bin/my-tool");
            assert_eq!(binaries[0].exported_path, "/home/user/.local/bin/my-tool");
            Ok(())
        })
    }

    // ---- B7: the single podman→docker fallback path ----------------------

    /// Every command the runner was asked to start, in order, as
    /// `(program, args)`. A retry appears here as the podman attempt followed
    /// by the docker one — which is the only way the fallback is observable at
    /// all, since both attempts return a `String`.
    fn started_argv(tracker: &OutputTracker<CommandRunnerEvent>) -> Vec<(String, Vec<String>)> {
        tracker
            .items()
            .iter()
            .filter_map(|e| e.command())
            .map(|c| {
                (
                    c.program.to_string_lossy().to_string(),
                    c.args
                        .iter()
                        .map(|a| a.to_string_lossy().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    /// The `ps` argv `get_container_id` builds, aimed at `program`. Spelled out
    /// rather than built from the same code under test, so the fixture cannot
    /// drift along with an implementation change.
    fn ps_argv(program: &str, container: &str) -> Vec<String> {
        vec![
            program.to_string(),
            "ps".to_string(),
            "-a".to_string(),
            "--filter".to_string(),
            format!("name=^{}$", container),
            "--format".to_string(),
            "{{.ID}}".to_string(),
        ]
    }

    /// The headline half of B7: podman failing is not fatal, because docker is
    /// tried next — and the retry re-uses the caller's argv rather than being
    /// rebuilt.
    #[test]
    fn start_falls_back_to_docker_when_podman_fails() -> Result<(), Error> {
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd_fails(&["podman", "start", "ubuntu"], "podman is not installed")
                .cmd(&["docker", "start", "ubuntu"], "ubuntu")
                .build(),
            default_cmd_factory(),
        );
        let tracker = db.cmd_runner.output_tracker();

        assert_eq!(block_on(db.start("ubuntu"))?, "ubuntu");

        let attempts = started_argv(&tracker);
        assert_eq!(attempts.len(), 2, "podman was tried, then docker");
        assert_eq!(attempts[0].0, "podman");
        assert_eq!(attempts[1].0, "docker");
        // Same arguments, only the program differs — that is `retarget`.
        assert_eq!(attempts[0].1, attempts[1].1);
        Ok(())
    }

    /// The other half, and the reason the two triggers stay distinct: `OnEmpty`
    /// retries docker only when podman SUCCEEDED and said nothing. A podman
    /// that ERRORED must propagate — otherwise a container that merely shares
    /// the name in docker's store would be silently substituted for podman's
    /// answer, and a broken podman install would look like a working one.
    ///
    /// This is the test that would fail if `get_container_id` used
    /// `Fallback::OnError` like its five siblings.
    #[test]
    fn get_container_id_does_not_fall_back_when_podman_errors() {
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd_fails(&ps_argv("podman", "ubuntu"), "podman is not installed")
                .cmd(&ps_argv("docker", "ubuntu"), "deadbeef1234".to_string())
                .build(),
            default_cmd_factory(),
        );
        let tracker = db.cmd_runner.output_tracker();

        let err = block_on(db.get_container_id("ubuntu"))
            .expect_err("a podman error must not be papered over by docker");
        assert!(
            !err.to_string().contains("deadbeef1234"),
            "docker's answer leaked into an error result: {err}"
        );
        // And docker was never even asked.
        let attempts = started_argv(&tracker);
        assert_eq!(
            attempts.len(),
            1,
            "docker must not be tried after a podman error: {attempts:?}"
        );
        assert_eq!(attempts[0].0, "podman");
    }

    /// The `OnEmpty` case it *is* for: podman succeeds with no output (the
    /// container lives in docker), so the ID comes from docker.
    #[test]
    fn get_container_id_falls_back_when_podman_is_empty() -> Result<(), Error> {
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd(&ps_argv("podman", "ubuntu"), String::new())
                .cmd(&ps_argv("docker", "ubuntu"), "deadbeef1234\n".to_string())
                .build(),
            default_cmd_factory(),
        );

        assert_eq!(block_on(db.get_container_id("ubuntu"))?, "deadbeef1234");
        Ok(())
    }

    /// `get_container_id` reports a genuinely absent container — reached only
    /// after BOTH runtimes answered empty — as a typed error, not an empty ID
    /// string that would later be interpolated into a command line.
    #[test]
    fn get_container_id_errors_when_neither_runtime_knows_the_name() {
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd(&ps_argv("podman", "ghost"), String::new())
                .cmd(&ps_argv("docker", "ghost"), String::new())
                .build(),
            default_cmd_factory(),
        );

        let err = block_on(db.get_container_id("ghost")).expect_err("not found is an error");
        assert!(err.to_string().contains("ghost"), "{err}");
    }

    /// `list_snapshots`' argv is byte-identical across the two runtimes,
    /// including the optional `--filter`. Under the old hand-rolled pair the
    /// docker retry rebuilt the argv from scratch and had to keep the filter
    /// clause in step by hand — so this pins the property that replaced it.
    #[test]
    fn list_snapshots_reuses_the_same_argv_on_the_docker_retry() -> Result<(), Error> {
        const FORMAT: &str = "{{.ID}}\t{{.Repository}}:{{.Tag}}\t{{.Created}}\t{{.Size}}";
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd_fails(
                    &[
                        "podman",
                        "images",
                        "--format",
                        FORMAT,
                        "--filter",
                        "reference=gosh-*",
                    ],
                    "podman is not installed",
                )
                .cmd(
                    &[
                        "docker",
                        "images",
                        "--format",
                        FORMAT,
                        "--filter",
                        "reference=gosh-*",
                    ],
                    "abc123\tgosh-1\t2 hours ago\t1.2 MB",
                )
                .build(),
            default_cmd_factory(),
        );
        let tracker = db.cmd_runner.output_tracker();

        let got = block_on(db.list_snapshots(Some("gosh-")))?;
        assert_eq!(got.items.len(), 1);
        assert_eq!(got.items[0].name, "gosh-1");
        assert!(got.skipped.is_empty());

        let attempts = started_argv(&tracker);
        assert_eq!(attempts.len(), 2, "podman, then the docker retry");
        assert_eq!(attempts[0].0, "podman");
        assert_eq!(attempts[1].0, "docker");
        assert_eq!(
            attempts[0].1, attempts[1].1,
            "the retry must not rebuild the argv"
        );
        assert!(
            attempts[1].1.contains(&"--filter".to_string()),
            "the filter clause has to survive the retry: {:?}",
            attempts[1].1
        );
        Ok(())
    }

    /// The `OnError` sites report the LAST runtime's failure, so a machine
    /// with neither runtime installed gets a real diagnostic rather than the
    /// podman error that was actually expected. (`OnEmpty` sites differ by
    /// design — `get_container_id` propagates the *podman* error instead of
    /// retrying on it; see `runtime_output` and D27.)
    #[test]
    fn runtime_output_reports_the_docker_error_when_both_runtimes_fail() {
        let db = Distrobox::new(
            NullCommandRunnerBuilder::new()
                .cmd_fails(&["podman", "start", "ubuntu"], "podman says no")
                .cmd_fails(&["docker", "start", "ubuntu"], "docker says no")
                .build(),
            default_cmd_factory(),
        );

        let err = block_on(db.start("ubuntu")).expect_err("both runtimes failed");
        assert!(
            err.to_string().contains("docker says no"),
            "the final error must be the last runtime's: {err}"
        );
    }
}
