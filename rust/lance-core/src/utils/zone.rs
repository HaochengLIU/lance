// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright The Lance Authors

//! Zone-related utilities for Lance data structures

use crate::Result;
use arrow_array::ArrayRef;

/// Zone bound within a fragment
///
/// This structure represents the boundary of a zone, which is a contiguous
/// range of rows within a fragment. Zones are used for scalar indexing and
/// column statistics.
///
/// # Example
///
/// Suppose we have two fragments, each with 4 rows:
/// - Fragment 0: start = 0, length = 4  // covers rows 0, 1, 2, 3
/// - Fragment 1: start = 0, length = 4  // covers rows 0, 1, 2, 3
///
/// After deleting rows 0 and 1 from fragment 0, and rows 1 and 2 from fragment 1:
/// - Fragment 0: start = 2, length = 2  // covers rows 2, 3
/// - Fragment 1: start = 0, length = 4  // covers rows 0, 3 (with gaps)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneBound {
    /// Fragment ID containing this zone
    pub fragment_id: u64,
    /// Start row offset within the fragment (local offset)
    ///
    /// To get the actual first row address, use `(fragment_id << 32) | start`.
    pub start: u64,
    /// Span of row offsets between the first and last row in the zone
    ///
    /// Calculated as (last_row_offset - first_row_offset + 1). This is not
    /// the count of physical rows, since deletions may create gaps within
    /// the span.
    pub length: usize,
}

/// Trait for processing data in zones and computing zone-level statistics.
///
/// This trait provides a common interface for zone-based processing used in
/// both scalar indexing (ZoneMap) and file-level column statistics.
///
/// Implementors accumulate statistics as chunks of data are processed, then
/// emit final statistics when a zone is complete.
pub trait ZoneProcessor {
    /// The type of statistics produced for each zone
    type ZoneStatistics;

    /// Process a slice of values that belongs to the current zone.
    ///
    /// This method is called repeatedly with chunks of data. Implementations
    /// should accumulate statistics incrementally.
    fn process_chunk(&mut self, values: &ArrayRef) -> Result<()>;

    /// Emit statistics when the zone is full or the fragment changes.
    ///
    /// The provided `bound` describes the row range covered by this zone.
    /// After calling this method, the processor should be ready to start
    /// accumulating statistics for the next zone (via `reset()`).
    fn finish_zone(&mut self, bound: ZoneBound) -> Result<Self::ZoneStatistics>;

    /// Reset state so the processor can handle the next zone.
    ///
    /// This is called after `finish_zone()` to prepare for processing
    /// the next zone's data.
    fn reset(&mut self) -> Result<()>;
}
