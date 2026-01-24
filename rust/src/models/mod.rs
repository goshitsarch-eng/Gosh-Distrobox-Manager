pub mod known_distros;
pub mod task;

pub use known_distros::{known_distro_by_image, KnownDistro};
pub use task::Task;
