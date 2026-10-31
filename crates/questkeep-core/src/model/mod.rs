//! Library data model. Every type here is plain serde data shared with the UI via ts-rs.

mod category;
mod completion;
mod game;
mod metadata;
mod query;
mod sorting;
mod start;
mod sync;
mod view;

pub use category::*;
pub use completion::*;
pub use game::*;
pub use metadata::*;
pub use query::*;
pub use sorting::{display_name_from_path, sorting_name};
pub use start::*;
pub use sync::SyncTimes;
pub use view::*;
