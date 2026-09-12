//! Deterministic limits for parsing preprocessed C source.

use measure::{Measure, Measurement};
use span::{Node, Span};

/// Hard recursion ceiling for the bounded parser worker, including debug builds.
pub(crate) const MAX_RULE_DEPTH: usize = 512;

/// Maximum resource use for one parser invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    /// Input bytes, before parsing or allocating parser state.
    pub max_input_bytes: usize,
    /// Recursive entries, loop iterations, input/constructed bytes, and structural visits.
    pub max_work: u64,
    /// Rule/loop steps without advancing the furthest examined source byte.
    pub max_backtracking_steps: u64,
    /// Simultaneously active recursive parsing calls, including precedence parsing.
    /// Values above 512 are rejected before starting the worker.
    pub max_rule_depth: usize,
    /// Maximum logical AST depth (including node/container wrappers and arena links).
    /// Values above 1024 are rejected before starting the worker.
    pub max_ast_depth: usize,
    /// Bytes retained in the parser's token buffer, including spare capacity.
    /// This excludes the AST and is not an allocator RSS measurement.
    pub max_cache_bytes: u64,
    /// Live construction-metric entries plus AST arena records.
    /// Completed external declarations release their child metrics.
    pub max_metadata_entries: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024 * 1024,
            max_work: 2_000_000_000,
            max_backtracking_steps: 1_000_000,
            max_rule_depth: MAX_RULE_DEPTH,
            max_ast_depth: 1024,
            max_cache_bytes: 256 * 1024 * 1024,
            max_metadata_entries: 500_000,
        }
    }
}

/// The resource that stopped parsing. Limits are independent of elapsed time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    InputBytes,
    Work,
    BacktrackingSteps,
    RuleDepth,
    AstDepth,
    CacheBytes,
    MetadataEntries,
    /// The operating system could not create the bounded parser worker.
    WorkerThread,
}

/// A parser resource diagnostic at a byte offset in the preprocessed source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimit {
    pub kind: ResourceKind,
    pub offset: usize,
    pub limit: u64,
    pub observed: u64,
}

impl ::std::fmt::Display for ResourceLimit {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        if self.kind == ResourceKind::WorkerThread {
            return f.write_str("unable to create parser worker thread");
        }
        write!(
            f,
            "parser {:?} limit exceeded ({} > {})",
            self.kind, self.observed, self.limit
        )
    }
}

/// Resource counters for a parser invocation, including work before failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseStatistics {
    pub work: u64,
    /// Maximum rule/loop steps without advancing the furthest examined source byte.
    pub maximum_backtracking_steps: u64,
    pub rule_entries: u64,
    pub maximum_rule_depth: usize,
    pub maximum_ast_depth: usize,
    pub structural_visits: u64,
    pub cache_bytes: u64,
    /// AST bytes cloned by parsing; zero for the handwritten parser.
    pub cloned_bytes: u64,
    pub maximum_metadata_entries: usize,
}

pub(crate) struct Budget {
    pub(crate) limits: ParseLimits,
    pub(crate) statistics: ParseStatistics,
    pub(crate) failure: Option<ResourceLimit>,
    depth: usize,
    furthest_offset: usize,
    backtracking_steps: u64,
    #[cfg(test)]
    verify_uncached: bool,
    nodes: ::rustc_hash::FxHashMap<(u8, usize, usize), Measurement>,
    external_nodes: ::rustc_hash::FxHashMap<(usize, usize), Measurement>,
    arena_nodes: usize,
}

impl Budget {
    pub(crate) fn new(limits: ParseLimits) -> Self {
        Self {
            limits,
            statistics: ParseStatistics::default(),
            failure: None,
            depth: 0,
            furthest_offset: 0,
            backtracking_steps: 0,
            #[cfg(test)]
            verify_uncached: false,
            nodes: ::rustc_hash::FxHashMap::default(),
            external_nodes: ::rustc_hash::FxHashMap::default(),
            arena_nodes: 0,
        }
    }

    pub(crate) fn check(
        &mut self,
        kind: ResourceKind,
        offset: usize,
        observed: u64,
        limit: u64,
    ) -> bool {
        if self.failure.is_some() {
            return false;
        }
        if observed > limit {
            self.failure = Some(ResourceLimit {
                kind,
                offset,
                observed,
                limit,
            });
            return false;
        }
        true
    }

    pub(crate) fn work(&mut self, offset: usize, amount: u64) -> bool {
        if self.failure.is_some() {
            return false;
        }
        let total = self.statistics.work.checked_add(amount);
        self.statistics.work = total.unwrap_or(u64::MAX);
        if total.is_none() {
            self.failure = Some(ResourceLimit {
                kind: ResourceKind::Work,
                offset,
                observed: u64::MAX,
                limit: self.limits.max_work,
            });
            return false;
        }
        self.check(
            ResourceKind::Work,
            offset,
            self.statistics.work,
            self.limits.max_work,
        )
    }

    pub(crate) fn step(&mut self, offset: usize) -> bool {
        if !self.work(offset, 1) {
            return false;
        }
        if offset > self.furthest_offset {
            self.furthest_offset = offset;
            self.backtracking_steps = 0;
        }
        self.backtracking_steps = self.backtracking_steps.saturating_add(1);
        self.statistics.maximum_backtracking_steps = self
            .statistics
            .maximum_backtracking_steps
            .max(self.backtracking_steps);
        self.check(
            ResourceKind::BacktrackingSteps,
            offset,
            self.backtracking_steps,
            self.limits.max_backtracking_steps,
        )
    }

    pub(crate) fn enter(&mut self, offset: usize) -> bool {
        if !self.step(offset)
            || !self.check(
                ResourceKind::RuleDepth,
                offset,
                (self.depth + 1) as u64,
                self.limits.max_rule_depth as u64,
            )
        {
            return false;
        }
        self.depth += 1;
        self.statistics.rule_entries += 1;
        self.statistics.maximum_rule_depth = self.statistics.maximum_rule_depth.max(self.depth);
        true
    }

    pub(crate) fn leave(&mut self, offset: usize, end: Option<usize>) {
        self.depth -= 1;
        if let Some(end) = end {
            self.work(offset, end.saturating_sub(offset) as u64);
        }
    }

    pub(crate) fn visit(&mut self, offset: usize, depth: usize) -> Result<(), &'static str> {
        self.statistics.structural_visits += 1;
        if !self.work(offset, 1)
            || !self.check(
                ResourceKind::AstDepth,
                offset,
                depth as u64,
                self.limits.max_ast_depth as u64,
            )
        {
            return Err("parser resource limit");
        }
        self.statistics.maximum_ast_depth = self.statistics.maximum_ast_depth.max(depth);
        Ok(())
    }

    /// Charge a live arena node alongside cached construction measurements.
    pub(crate) fn arena_node(&mut self, offset: usize) -> Result<(), &'static str> {
        let count = self
            .nodes
            .len()
            .saturating_add(self.external_nodes.len())
            .saturating_add(self.arena_nodes)
            .saturating_add(1);
        if !self.check(
            ResourceKind::MetadataEntries,
            offset,
            count as u64,
            self.limits.max_metadata_entries.min(u32::MAX as usize) as u64,
        ) {
            return Err("parser resource limit");
        }
        self.arena_nodes += 1;
        self.statistics.maximum_metadata_entries =
            self.statistics.maximum_metadata_entries.max(count);
        Ok(())
    }

    pub(crate) fn node<T: Measure>(
        &mut self,
        value: T,
        span: Span,
        arena: &::arena::Arena,
    ) -> Result<Node<T>, &'static str> {
        // Measure each new root; only already-constructed child nodes may reuse
        // structural measurements.
        if !self.work(span.start, ::std::mem::size_of::<Node<T>>() as u64) {
            return Err("parser resource limit");
        }
        self.visit(span.start, 1)?;
        let child = value.measure(self, span.start, 2, arena)?;
        let measurement = Measurement {
            bytes: child
                .bytes
                .saturating_add(::std::mem::size_of::<Node<T>>() as u64),
            depth: child.depth + 1,
        };
        #[cfg(test)]
        {
            // The upstream reference suite checks the cache invariant against an
            // exhaustive walk at every constructor, without charging production work.
            let mut reference = Self::new(ParseLimits {
                max_work: u64::MAX,
                max_ast_depth: 2048,
                ..self.limits
            });
            reference.verify_uncached = true;
            let actual = value
                .measure(&mut reference, span.start, 2, arena)
                .expect("constructed subtree has bounded depth");
            assert!(measurement.depth > actual.depth);
            assert!(measurement.bytes >= actual.bytes + ::std::mem::size_of::<Node<T>>() as u64);
        }
        self.save_node::<T>(span, measurement)?;
        Ok(Node::new(value, span))
    }

    /// Reuse a node's cached subtree measurement at its current AST depth.
    ///
    /// Cached depths include the node itself. A hit still checks the resulting
    /// total depth before reusing the subtree without walking its children.
    pub(crate) fn node_measurement<T: Measure>(
        &mut self,
        span: Span,
        depth: usize,
    ) -> Result<Option<Measurement>, &'static str> {
        #[cfg(test)]
        if self.verify_uncached {
            return Ok(None);
        }
        let measurement = if T::external() {
            self.external_nodes.get(&(span.start, span.end)).copied()
        } else {
            T::identity().and_then(|kind| self.nodes.get(&(kind, span.start, span.end)).copied())
        };
        if let Some(measurement) = measurement {
            let actual_depth = depth.saturating_add(measurement.depth).saturating_sub(1);
            if !self.check(
                ResourceKind::AstDepth,
                span.start,
                actual_depth as u64,
                self.limits.max_ast_depth as u64,
            ) {
                return Err("parser resource limit");
            }
            self.statistics.maximum_ast_depth = self.statistics.maximum_ast_depth.max(actual_depth);
        }
        Ok(measurement)
    }

    /// Retain the largest measurements recorded for a node kind and source span.
    ///
    /// External declarations keep separate entries and release cached child measurements.
    /// Types without a cache identity are checked for depth but are not retained.
    pub(crate) fn save_node<T: Measure>(
        &mut self,
        span: Span,
        value: Measurement,
    ) -> Result<(), &'static str> {
        if !self.check(
            ResourceKind::AstDepth,
            span.start,
            value.depth as u64,
            self.limits.max_ast_depth as u64,
        ) {
            return Err("parser resource limit");
        }
        #[cfg(test)]
        if self.verify_uncached {
            return Ok(());
        }
        let key = match T::identity() {
            Some(key) => key,
            None => return Ok(()),
        };
        if T::external() {
            if !self.work(span.start, self.nodes.capacity() as u64) {
                return Err("parser resource limit");
            }
            // Do not repeatedly scan an oversized allocation after a large function.
            if self.nodes.capacity() > 4 * self.nodes.len().max(32) {
                self.nodes = ::rustc_hash::FxHashMap::default();
            } else {
                self.nodes.clear();
            }
        }
        let is_new = if T::external() {
            !self.external_nodes.contains_key(&(span.start, span.end))
        } else {
            !self.nodes.contains_key(&(key, span.start, span.end))
        };
        let count = self
            .nodes
            .len()
            .saturating_add(self.external_nodes.len())
            .saturating_add(self.arena_nodes)
            .saturating_add(usize::from(is_new));
        if !self.check(
            ResourceKind::MetadataEntries,
            span.start,
            count as u64,
            self.limits.max_metadata_entries as u64,
        ) {
            return Err("parser resource limit");
        }
        self.statistics.maximum_metadata_entries =
            self.statistics.maximum_metadata_entries.max(count);
        let entry = if T::external() {
            self.external_nodes
                .entry((span.start, span.end))
                .or_default()
        } else {
            self.nodes.entry((key, span.start, span.end)).or_default()
        };
        entry.depth = entry.depth.max(value.depth);
        entry.bytes = entry.bytes.max(value.bytes);
        Ok(())
    }
}
