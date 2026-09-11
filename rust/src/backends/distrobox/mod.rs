pub mod command;
// Module named after its parent (`backends::distrobox::distrobox`) because it holds
// the Distrobox type itself. Both names are load-bearing today (they carry history
// and every call site); the rename belongs to T1's workspace restructure, not to a
// lint commit. Scoped allow so the structural lint stays visible.
#[allow(clippy::module_inception)]
pub mod distrobox;

pub use distrobox::*;
