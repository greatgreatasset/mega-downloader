//! The "structure brain": parse MEGA links and (in Phase 1) reconstruct the
//! encrypted node tree via MEGA's `cs` API. Listing is free and unmetered, so
//! we can always know the correct hierarchy regardless of the byte source.

pub mod crypto;
pub mod folder;
pub mod link;

pub use folder::{fetch_tree, NodeKind, Tree, TreeNode};
pub use link::{parse, MegaLink};
