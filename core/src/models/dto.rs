//! Data-transfer types that used to live at the top of `api.rs`.
//!
//! Moved here by S3 (architecture.md §1.3) so the types outlive the FRB shim
//! layer that S7 deletes: the app crate depends on these names, and S7 removes
//! `api.rs` whole. `api.rs` re-exports them again through `pub use crate::models::*`
//! because the re-export list is what flutter_rust_bridge scans.
//!
//! This module is where B3's `ParseIssue` lands (architecture.md §6.4).

pub use crate::backends::ContainerInfo;
pub use crate::backends::ContainerStats;
pub use crate::backends::CreateArgName;
pub use crate::backends::CreateArgs;
pub use crate::backends::PackageInfo;
pub use crate::backends::SnapshotInfo;
pub use crate::backends::Status;
pub use crate::backends::Volume;
pub use crate::backends::VolumeMode;
pub use crate::backends::desktop_file::DesktopEntry;
pub use crate::models::known_distros::{KnownDistro, PackageManager};

/// Represents an application that can be exported from a container
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub desktop_file_path: String,
    pub is_exported: bool,
}

/// Represents a binary that has been exported from a container
#[derive(Debug, Clone)]
pub struct ExportedBinary {
    pub name: String,
    pub source_path: String,
    pub exported_path: String,
}
