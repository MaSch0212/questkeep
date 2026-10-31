//! Library data model. Every type here is plain serde data shared with the UI via ts-rs.

mod category;
mod combine;
mod completion;
mod edit;
mod game;
mod metadata;
mod query;
mod sorting;
mod start;
mod sync;
mod view;

pub use category::*;
pub use combine::CombinePlan;
pub use completion::*;
pub use edit::*;
pub use game::*;
pub use metadata::*;
pub use query::*;
pub use sorting::{
    DEFAULT_PREFIXES, display_name_from_path, set_ignored_prefixes, sorting_name, sorting_name_with,
};
pub use start::*;
pub use sync::SyncTimes;
pub use view::*;
