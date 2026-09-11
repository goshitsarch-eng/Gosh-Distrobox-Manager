use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use crate::fakers::Command;

pub static DISTROS: LazyLock<HashMap<String, KnownDistro>> = LazyLock::new(|| {
    [
        ("alma", "#dadada", PackageManager::Dnf),
        ("alpine", "#2147ea", PackageManager::Apk),
        ("amazon", "#de5412", PackageManager::Dnf),
        ("arch", "#12aaff", PackageManager::Pacman),
        ("centos", "#ff6600", PackageManager::Dnf),
        ("clearlinux", "#56bbff", PackageManager::Unknown),
        ("crystal", "#8839ef", PackageManager::Unknown),
        ("debian", "#da5555", PackageManager::Apt),
        ("deepin", "#0050ff", PackageManager::Apt),
        ("fedora", "#3b6db3", PackageManager::Dnf),
        ("gentoo", "#daaada", PackageManager::Emerge),
        ("kali", "#000000", PackageManager::Apt),
        ("mageia", "#b612b6", PackageManager::Dnf),
        ("mint", "#6fbd20", PackageManager::Apt),
        ("neon", "#27ae60", PackageManager::Apt),
        ("opensuse", "#daff00", PackageManager::Zypper),
        ("oracle", "#ff0000", PackageManager::Dnf),
        ("redhat", "#ff6662", PackageManager::Dnf),
        ("rhel", "#ff6662", PackageManager::Dnf),
        ("rocky", "#91ff91", PackageManager::Dnf),
        ("slackware", "#6145a7", PackageManager::Unknown),
        ("ubuntu", "#FF4400", PackageManager::Apt),
        ("vanilla", "#7f11e0", PackageManager::Unknown),
        ("void", "#abff12", PackageManager::Xbps),
    ]
    .iter()
    .map(|(name, color, package_manager)| {
        (
            name.to_string(),
            KnownDistro::new(name, color, *package_manager),
        )
    })
    .collect()
});

/// Package managers a container can report (B1). `Unknown` ONLY when
/// genuinely nothing is found — the UI offers manual command entry instead
/// of an error toast for it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum PackageManager {
    #[default]
    Unknown,
    Apt,
    Dnf,
    Pacman,
    Apk,
    Zypper,
    Xbps,
    Emerge,
}

impl PackageManager {
    /// Parse a `detect_package_manager` script word (`apt`, `emerge`, …).
    /// `yum` maps to `Dnf` (same backend family, one verb set).
    pub fn from_detected(s: &str) -> Self {
        match s.trim() {
            "apt" => PackageManager::Apt,
            "dnf" | "yum" => PackageManager::Dnf,
            "pacman" => PackageManager::Pacman,
            "apk" => PackageManager::Apk,
            "zypper" => PackageManager::Zypper,
            "xbps" => PackageManager::Xbps,
            "emerge" => PackageManager::Emerge,
            _ => PackageManager::Unknown,
        }
    }

    /// Display badge (`APT`, `EMERGE`, … — Flutter showed `.toUpperCase()`).
    pub fn badge(&self) -> &'static str {
        match self {
            PackageManager::Unknown => "UNKNOWN",
            PackageManager::Apt => "APT",
            PackageManager::Dnf => "DNF",
            PackageManager::Pacman => "PACMAN",
            PackageManager::Apk => "APK",
            PackageManager::Zypper => "ZYPPER",
            PackageManager::Xbps => "XBPS",
            PackageManager::Emerge => "EMERGE",
        }
    }

    /// Install verb (one table — B1). NOTE `emerge --ask=n`, not `-y`
    /// (emerge has no `-y`).
    pub fn install_verb(&self, package: &str) -> Option<String> {
        let p = package.replace("'", "'\\''");
        match self {
            PackageManager::Apt => Some(format!("sudo apt-get install -y '{p}'")),
            PackageManager::Dnf => Some(format!("sudo dnf install -y '{p}'")),
            PackageManager::Pacman => Some(format!("sudo pacman -S --noconfirm '{p}'")),
            PackageManager::Apk => Some(format!("sudo apk add '{p}'")),
            PackageManager::Zypper => Some(format!("sudo zypper install -y '{p}'")),
            PackageManager::Xbps => Some(format!("sudo xbps-install -y '{p}'")),
            PackageManager::Emerge => Some(format!("sudo emerge --ask=n '{p}'")),
            PackageManager::Unknown => None,
        }
    }

    /// Remove verb (one table — B1).
    pub fn remove_verb(&self, package: &str) -> Option<String> {
        let p = package.replace("'", "'\\''");
        match self {
            PackageManager::Apt => Some(format!("sudo apt-get remove -y '{p}'")),
            PackageManager::Dnf => Some(format!("sudo dnf remove -y '{p}'")),
            PackageManager::Pacman => Some(format!("sudo pacman -R --noconfirm '{p}'")),
            PackageManager::Apk => Some(format!("sudo apk del '{p}'")),
            PackageManager::Zypper => Some(format!("sudo zypper remove -y '{p}'")),
            PackageManager::Xbps => Some(format!("sudo xbps-remove -y '{p}'")),
            PackageManager::Emerge => Some(format!("sudo emerge --unmerge '{p}'")),
            PackageManager::Unknown => None,
        }
    }

    pub fn install_cmd(&self, file: &Path) -> Option<Command> {
        match self {
            PackageManager::Apt => Some(apt_install_cmd(file)),
            PackageManager::Dnf => Some(dnf_install_cmd(file)),
            PackageManager::Pacman => Some(pacman_install_cmd(file)),
            PackageManager::Apk => Some(apk_install_cmd(file)),
            PackageManager::Zypper => Some(zypper_install_cmd(file)),
            PackageManager::Xbps => Some(xbps_install_cmd(file)),
            // No valid local-file verb (see above): None, not a guess.
            PackageManager::Emerge | PackageManager::Unknown => None,
        }
    }
    pub fn installable_file(&self) -> Option<&str> {
        match self {
            PackageManager::Apt => Some(".deb"),
            PackageManager::Dnf => Some(".rpm"),
            PackageManager::Pacman => Some(".pkg.tar.zst"),
            PackageManager::Apk => Some(".apk"),
            PackageManager::Zypper => Some(".rpm"),
            // Binary package formats for file-install; source-based (emerge)
            // and xbps have no direct local-file verb here.
            PackageManager::Xbps | PackageManager::Emerge | PackageManager::Unknown => None,
        }
    }
}

fn apt_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("apt-get");
    cmd.arg("install").arg(file);
    cmd
}

fn dnf_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("dnf");
    cmd.arg("install").arg(file);
    cmd
}

fn pacman_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("pacman");
    cmd.arg("-U").arg(file);
    cmd
}

fn apk_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("apk");
    cmd.arg("add").arg("--allow-untrusted").arg(file);
    cmd
}

fn zypper_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("zypper");
    cmd.arg("install").arg(file);
    cmd
}

fn xbps_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("xbps-install");
    cmd.arg(file);
    cmd
}

// No emerge file-install constructor: local-file install is not an emerge
// verb (source-based), so there is no valid `Command` to build. `install_cmd`
// returns `None` for `Emerge` (matching `installable_file() == None`) rather
// than a plausible-but-invalid command — a trap the reviewer caught.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_words_map_to_typed_managers() {
        assert_eq!(PackageManager::from_detected("apt\n"), PackageManager::Apt);
        assert_eq!(PackageManager::from_detected("yum"), PackageManager::Dnf);
        assert_eq!(
            PackageManager::from_detected("emerge"),
            PackageManager::Emerge
        );
        assert_eq!(PackageManager::from_detected("xbps"), PackageManager::Xbps);
        assert_eq!(
            PackageManager::from_detected("unknown"),
            PackageManager::Unknown
        );
        assert_eq!(PackageManager::from_detected(""), PackageManager::Unknown);
    }

    #[test]
    fn emerge_verbs_use_ask_n_not_y() {
        // B1: emerge has no `-y`; `--ask=n` is the non-interactive verb.
        let install = PackageManager::Emerge.install_verb("vim").unwrap();
        assert!(install.contains("--ask=n"), "{install}");
        assert!(!install.contains(" -y"), "{install}");
        let remove = PackageManager::Emerge.remove_verb("vim").unwrap();
        assert!(remove.contains("--unmerge"), "{remove}");
    }

    #[test]
    fn gentoo_and_void_map_to_real_managers() {
        // B1: the old table mapped both to Unknown.
        assert_eq!(
            DISTROS.get("gentoo").unwrap().package_manager,
            PackageManager::Emerge
        );
        assert_eq!(
            DISTROS.get("void").unwrap().package_manager,
            PackageManager::Xbps
        );
    }

    #[test]
    fn unknown_has_no_verbs() {
        assert!(PackageManager::Unknown.install_verb("x").is_none());
        assert!(PackageManager::Unknown.remove_verb("x").is_none());
    }
}

pub fn known_distro_by_image(url: &str) -> Option<KnownDistro> {
    DISTROS
        .values()
        .find(|distro| url.contains(&distro.name))
        .cloned()
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct KnownDistro {
    pub name: String,
    pub color: String,
    pub package_manager: PackageManager,
}

impl KnownDistro {
    pub fn new(name: &str, color: &str, package_manager: PackageManager) -> Self {
        Self {
            name: name.to_string(),
            color: color.to_string(),
            package_manager,
        }
    }
    pub fn icon_name(&self) -> String {
        format!("{}-symbolic", self.name)
    }
    pub fn default_icon_name() -> &'static str {
        "tux-symbolic"
    }
}
