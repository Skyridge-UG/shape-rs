//! Helpers the passes share: edit spans, who owns a node, statement lists.

mod edits;
pub(crate) mod index;
mod lists;
mod owners;
pub(crate) mod primitives;
pub(crate) mod purity;
mod syntax;

pub(crate) use edits::{apply_checked, indent_at, indent_before, whole_line};
pub(crate) use lists::for_each_list;
pub(crate) use owners::{function_of, owner, unclosed};
pub(crate) use syntax::{is_loop, is_void_zero, unwrap};
