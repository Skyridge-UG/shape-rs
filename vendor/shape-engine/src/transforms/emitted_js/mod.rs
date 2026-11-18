//! Oxc cleanup of JavaScript emitted by [`crate::emit_javascript`].
//!
//! The standalone declaration pass removes only unread declarations with
//! pure, non-throwing initializers. Linked-program passes also simplify SSA,
//! carries, property writes, and repeated closed VM helpers. Each pass has its
//! own proof boundary; see [`README.md`](README.md) and the individual modules.
//! The standalone pass skips dynamic scope and unsupported declaration forms.
//!
//! ## Coordinates and provenance
//!
//! The output is produced by exact, non-overlapping source edits. A wholly
//! removed declaration is replaced by an empty statement (`;`) so adjacent
//! tokens cannot be joined across the deletion; a partially removed declaration
//! keeps its keyword, semicolon, and surviving declarators. [`RemovedRange`]
//! records original input byte offsets. Bytes after the first removal move by
//! the total deleted length, so old offsets cannot be used as output offsets.
//!
//! An [`EmittedStatementMap`] is index-based. A whole declaration in the
//! primary emitted function body becomes an empty statement, so later output
//! statement indexes stay stable even though the removed entry has no useful
//! provenance. The standalone report marks the original mapping
//! [`StatementMapStatus::Invalidated`]. [`cleanup_emitted_javascript`]
//! regenerates that map (and `node_map`) exactly and reports
//! [`StatementMapStatus::Regenerated`]. Entries that pointed at a removed
//! statement are dropped; surviving entries keep their parsed output indexes.
//!
//! The module also exports an independent property-write pass,
//! [`simplify_property_writes`], which rewrites the emitter's exact
//! `Object.defineProperty(receiver, key, {writable:true, configurable:true,
//! enumerable:true, value})` data-descriptor shape to `receiver[key] = value`
//! only when the native authority, receiver, key, descriptor, and evaluation
//! order are all proven. See the [`simplify_property_writes`] documentation and
//! `docs/transforms/emitted-property-writes.md`. Its edits are expression-local, so
//! [`CleanedEmittedPropertyWrites`] preserves the caller's statement/node
//! maps instead of regenerating them.

// The passes, by family. Each module is also imported here under its own
// name, so `super::name` paths (and the re-exports below) reach it.
mod branches;
mod carries;
mod expressions;
mod facts;
mod helpers;
mod literals;
mod loops;
mod names;
mod pipeline;
mod program;
mod ssa;
mod storage;
mod support;

use branches::{
    blocks, branch_handoffs, branch_tails, branch_value, conditional_values, empty_else,
    empty_statements, flatten_exits, literal_branch_join, return_joins,
};
use carries::{
    carry_coalesce, carry_join, carry_lifetimes, carry_self_assign, dead_carry, equivalent_carry,
    join_channels, stable_channels,
};
use expressions::{
    bit_accumulators, constant_slots, discarded_values, empty_starts, let_runs, method_calls,
    negated_comparisons, null_flags, nullish_syntax, numeric_coercions, optional_chains,
    path_temporaries, prototype_methods, tidy_expressions, trailing_returns,
};
use facts::{branch_facts, cell_facts, cell_liveness};
use helpers::{
    expand_vm_sub, inline_small_helpers, inline_tuple_helpers, inline_vm_sub, runtime_helpers,
    tuple_scalarize, tuple_unpack, vm_sub_dedupe, vm_sub_parameters, vm_sub_simplify,
};
use literals::{
    adjacent_literals, array_fills, array_format, array_initializers, dense_arrays,
    fold_literal_fills, inline_single_use_literals, move_store_declarations, nest_literals,
    point_literals, property_writes,
};
use loops::{
    copied_counters, index_counters, loop_counter_copies, loop_counters, loop_labels, loop_tests,
    native_for_in, structured_loops,
};
use names::{name_loop_and_catch_bindings, positional_names, role_names, short_generated_names};
use pipeline::{stages, thread};
use program::{
    arrows, classes, dotted_properties, thread_parameters, unwrap_host_globals, vm_fingerprint,
    vm_values,
};
use ssa::{
    adjacent_ssa, alias_ssa, argument_snapshots, constant_values, dead_ssa, forward_values,
    literal_ssa, local_values, merge_derived, slot_forward, snapshot_copies, snapshot_ssa,
};
use storage::{
    dead_key, dead_locals, declaration_lifetimes, fresh_values, key_declarations, key_webs,
    last_use_storage, local_vars, private_array_reads, redundant_initializers, unread_declarations,
    unused_keys,
};
use support::{index, primitives, purity};

pub use array_format::compact_arrays;
pub(crate) use short_generated_names::shorten_generated_names;
pub use stages::{clean_function, finish_program};
pub use thread::cleanup_emitted_thread;
pub use thread::{
    cleanup_thread, cleanup_thread_with_options, ThreadCleanup, ThreadCleanupOptions,
};
pub(crate) use vm_sub_dedupe::share_vm_sub_helpers;
pub(crate) use vm_sub_parameters::name_vm_sub_parameters;
pub(crate) use vm_sub_simplify::simplify_vm_sub_bodies;

pub(crate) use adjacent_literals::inline_adjacent_literals;
pub use adjacent_ssa::{inline_adjacent_ssa, AdjacentSsaResult};
pub(crate) use alias_ssa::inline_alias_ssa;
pub(crate) use array_fills::fold_array_fills;
pub(crate) use arrows::arrow_callbacks;
pub(crate) use bit_accumulators::fold_bit_accumulators;
pub(crate) use branch_tails::hoist_branch_tails;
pub(crate) use branch_value::{
    fold_branch_values, fold_multi_branch_values, fold_plain_branch_assignments,
};
pub(crate) use carry_coalesce::coalesce_carry_copies;
pub(crate) use carry_join::inline_single_use_carry_joins;
pub(crate) use carry_self_assign::remove_carry_self_assignments;
pub(crate) use classes::rebuild_classes;
pub(crate) use constant_slots::inline_constant_slots;
pub(crate) use copied_counters::step_copied_counters;
pub(crate) use dead_carry::remove_dead_carries;
pub(crate) use dead_key::remove_unread_key_stores;
pub(crate) use dead_ssa::{
    remove_dead_ssa, remove_discarded_argument_reads, remove_noop_local_reads,
};
pub(crate) use dotted_properties::dotted_properties;
pub(crate) use empty_else::remove_empty_else_blocks;
pub(crate) use empty_starts::tidy_empty_starts;
pub(crate) use equivalent_carry::merge_equivalent_carries;
pub(crate) use flatten_exits::flatten_exits;
pub use fold_literal_fills::{fold_literal_fills, LiteralFillResult};
pub(crate) use forward_values::{
    forward_values, forward_values_and_property_reads, forward_values_once,
};
pub use inline_single_use_literals::{inline_single_use_literals, SingleUseLiteralResult};
pub(crate) use inline_small_helpers::{
    hoist_helper_calls, hoist_remaining_helper_calls, inline_remaining_helpers,
    inline_small_helpers,
};
pub(crate) use inline_tuple_helpers::inline_tuple_helpers;
pub(crate) use inline_vm_sub::inline_vm_sub_wrappers;
pub use inline_vm_sub::{inline_remaining_vm_subs, InlinedVmSubs};
pub(crate) use key_declarations::place_key_declarations;
pub(crate) use key_webs::split_block_key_webs;
pub(crate) use let_runs::join_let_runs;
pub use literal_branch_join::{collapse_literal_branch_joins, LiteralBranchJoinResult};
pub use literal_ssa::{inline_literal_ssa, LiteralSsaResult};
pub(crate) use local_vars::clean_local_vars;
pub(crate) use loop_counter_copies::merge_loop_counter_copies;
pub(crate) use loop_counters::lift_loop_counters;
pub(crate) use loop_labels::remove_needless_loop_labels;
pub(crate) use loop_tests::split_loop_tests;
pub(crate) use merge_derived::{merge_derived_values, split_single_reads};
pub(crate) use move_store_declarations::move_store_declarations;
pub use name_loop_and_catch_bindings::{name_loop_and_catch_bindings, LoopCatchNamesResult};
pub(crate) use native_for_in::native_for_in_loops;
pub(crate) use negated_comparisons::flip_negated_comparisons;
pub(crate) use nest_literals::nest_literals;
pub(crate) use optional_chains::optional_chains;
pub(crate) use path_temporaries::inline_path_temporaries;
pub(crate) use positional_names::name_bindings_by_position;
pub(crate) use prototype_methods::name_prototype_methods;
pub use redundant_initializers::{remove_redundant_initializers, RedundantInitializerResult};
pub(crate) use role_names::name_generated_roles;
pub use role_names::{role_of, Role};
pub(crate) use structured_loops::{reconstruct_loops, unwrap_single_pass_loops};
pub(crate) use thread_parameters::thread_parameters;
pub(crate) use tidy_expressions::{resugar_expressions, tidy_expressions, tidy_final_expressions};
pub(crate) use trailing_returns::drop_trailing_returns;
pub(crate) use tuple_scalarize::scalarize_literal_tuples;
pub(crate) use tuple_unpack::unpack_fresh_helper_tuples;
pub(crate) use unused_keys::remove_unused_keys;
pub use unwrap_host_globals::{unwrap_host_globals, HostGlobalsResult};
pub(crate) use vm_values::replace_vm_values;

pub use array_initializers::{
    simplify_array_constructions, simplify_cleaned_emitted_property_writes,
    ArrayConstructionCleanupError, ArrayConstructionCleanupOutcome, ArrayConstructionSkip,
    ArrayConstructionSkipReason, CleanedEmittedArrayConstructions,
};
pub use property_writes::{
    simplify_cleaned_emitted_javascript, simplify_emitted_property_writes,
    simplify_program_property_writes, simplify_property_writes,
    simplify_readable_program_property_writes, CleanedEmittedPropertyWrites,
    PropertyWriteCleanupError, PropertyWriteCleanupOutcome, PropertyWriteSkip,
    PropertyWriteSkipReason, MAX_PRESEED_ARRAY_ELEMENTS, MAX_PRESEED_OBJECT_KEYS,
};

pub use unread_declarations::{
    cleanup, cleanup_emitted_javascript, cleanup_emitted_javascript_with_options,
    cleanup_with_options, CleanedEmittedJavaScript, EmittedCleanupError, EmittedCleanupOptions,
    EmittedCleanupReport, EmittedCleanupResult, RemovedRange, StatementMapStatus,
};
