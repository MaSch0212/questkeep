//! Library data model. Every type here is plain serde data shared with the UI via ts-rs.

mod category;
mod game;
mod query;
mod sorting;
mod start;
mod sync;
mod view;

pub use category::*;
pub use game::*;
pub use query::*;
pub use sorting::{display_name_from_path, sorting_name};
pub use start::*;
pub use sync::SyncTimes;
pub use view::*;
