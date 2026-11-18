//! Expression tidying.

pub(crate) mod negated_comparisons;
pub(crate) mod numeric_coercions;
pub(crate) mod tidy_expressions;
// No stage runs it since the per-thread pipeline (it changed no thread).
pub(crate) mod bit_accumulators;
pub(crate) mod constant_slots;
pub(crate) mod discarded_values;
pub(crate) mod empty_starts;
pub(crate) mod let_runs;
#[allow(dead_code)]
pub(crate) mod method_calls;
pub(crate) mod null_flags;
pub(crate) mod nullish_syntax;
pub(crate) mod optional_chains;
pub(crate) mod path_temporaries;
pub(crate) mod prototype_methods;
pub(crate) mod trailing_returns;
