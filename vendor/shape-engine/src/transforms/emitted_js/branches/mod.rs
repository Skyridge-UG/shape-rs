//! Branch joins, exits and blocks.

pub(crate) mod blocks;
pub(crate) mod branch_handoffs;
pub(crate) mod branch_tails;
pub(crate) mod branch_value;
pub(crate) mod conditional_values;
pub(crate) mod empty_else;
pub(crate) mod empty_statements;
pub(crate) mod flatten_exits;
pub(crate) mod literal_branch_join;
pub(crate) mod return_joins;
