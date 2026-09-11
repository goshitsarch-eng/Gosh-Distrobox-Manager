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
        ("gentoo", "#daaada", PackageManager::Unknown),
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
        ("void", "#abff12", PackageManager::Unknown),
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum PackageManager {
    #[default]
    Unknown,
    Apt,
    Dnf,
    Pacman,
    Apk,
    Zypper,
}

impl PackageManager {
    pub fn install_cmd(&self, file: &Path) -> Option<Command> {
        match self {
            PackageManager::Apt => Some(apt_install_cmd(file)),
            PackageManager::Dnf => Some(dnf_install_cmd(file)),
            PackageManager::Pacman => Some(pacman_install_cmd(file)),
            PackageManager::Apk => Some(apk_install_cmd(file)),
            PackageManager::Zypper => Some(zypper_install_cmd(file)),
            PackageManager::Unknown => None,
        }
    }
    pub fn installable_file(&self) -> Option<&str> {
        match self {
            PackageManager::Apt => Some(".deb"),
            PackageManager::Dnf => Some(".rpm"),
            PackageManager::Pacman => Some(".pkg.tar.zst"),
            PackageManager::Apk => Some(".apk"),
            PackageManager::Zypper => Some(".rpm"),
            PackageManager::Unknown => None,
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
