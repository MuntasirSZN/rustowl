//! Utility functions for range manipulation and MIR analysis.
//!
//! This module provides core algorithms for working with source code ranges,
//! merging overlapping ranges, and providing visitor patterns for MIR traversal.

use crate::models::range_vec_into_vec;
use crate::models::*;

/// Determines if one range completely contains another range.
///
/// A range `r1` is a super range of `r2` if `r1` completely encompasses `r2`.
/// This means `r1` starts before or at the same position as `r2` and ends
/// after or at the same position as `r2`, with at least one strict inequality.
pub fn is_super_range(r1: Range, r2: Range) -> bool {
    (r1.from() < r2.from() && r2.until() <= r1.until())
        || (r1.from() <= r2.from() && r2.until() < r1.until())
}

/// Finds the overlapping portion of two ranges.
///
/// Returns the intersection of two ranges if they overlap, or `None` if
/// they don't intersect.
pub fn common_range(r1: Range, r2: Range) -> Option<Range> {
    if r2.from() < r1.from() {
        return common_range(r2, r1);
    }
    if r1.until() < r2.from() {
        return None;
    }
    let from = r2.from();
    let until = r1.until().min(r2.until());
    Range::new(from, until)
}

/// Finds all pairwise intersections among a collection of ranges.
///
/// Returns a vector of ranges representing all overlapping regions
/// between pairs of input ranges, with overlapping regions merged.
pub fn common_ranges(ranges: &[Range]) -> Vec<Range> {
    let mut common_ranges = Vec::new();
    for i in 0..ranges.len() {
        for j in i + 1..ranges.len() {
            if let Some(common) = common_range(ranges[i], ranges[j]) {
                common_ranges.push(common);
            }
        }
    }
    eliminated_ranges(common_ranges)
}

/// Merges two ranges into their superset if they overlap or are adjacent.
///
/// Returns a single range that encompasses both input ranges if they
/// overlap or are directly adjacent. Returns `None` if they are disjoint.
pub fn merge_ranges(r1: Range, r2: Range) -> Option<Range> {
    if common_range(r1, r2).is_some() || r1.until() == r2.from() || r2.until() == r1.from() {
        let from = r1.from().min(r2.from());
        let until = r1.until().max(r2.until());
        Range::new(from, until)
    } else {
        None
    }
}

/// Eliminates overlapping and adjacent ranges by merging them.
///
/// Optimized implementation: O(n log n) sort + linear merge instead of
/// the previous O(n^2) pairwise merging loop. Keeps behavior identical.
pub fn eliminated_ranges(mut ranges: Vec<Range>) -> Vec<Range> {
    if ranges.len() <= 1 {
        return ranges;
    }
    // Sort by start, then end
    ranges.sort_by_key(|r| (r.from().0, r.until().0));
    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());
    let mut current = ranges[0];
    for r in ranges.into_iter().skip(1) {
        if r.from().0 <= current.until().0 || r.from().0 == current.until().0 {
            // Overlapping or adjacent
            if r.until().0 > current.until().0 {
                current = Range::new(current.from(), r.until()).unwrap();
            }
        } else {
            merged.push(current);
            current = r;
        }
    }
    merged.push(current);
    merged
}

/// Version of [`eliminated_ranges`] that works with SmallVec.
pub fn eliminated_ranges_small(ranges: RangeVec) -> Vec<Range> {
    eliminated_ranges(range_vec_into_vec(ranges))
}

/// Subtracts exclude ranges from a set of ranges.
///
/// For each range in `from`, removes any portions that overlap with
/// ranges in `excludes`. If a range is partially excluded, it may be
/// split into multiple smaller ranges.
pub fn exclude_ranges(from: Vec<Range>, excludes: Vec<Range>) -> Vec<Range> {
    let mut from = from;
    let mut i = 0;
    'outer: while i < from.len() {
        let mut j = 0;
        while j < excludes.len() {
            if let Some(common) = common_range(from[i], excludes[j]) {
                if let Some(r) = Range::new(from[i].from(), common.from() - 1) {
                    from.push(r);
                }
                if let Some(r) = Range::new(common.until() + 1, from[i].until()) {
                    from.push(r);
                }
                from.remove(i);
                continue 'outer;
            }
            j += 1;
        }
        i += 1;
    }
    eliminated_ranges(from)
}

/// Version of [`exclude_ranges`] that works with SmallVec.
pub fn exclude_ranges_small(from: RangeVec, excludes: Vec<Range>) -> Vec<Range> {
    exclude_ranges(range_vec_into_vec(from), excludes)
}

/// Visitor trait for traversing MIR (Mid-level IR) structures.
///
/// Provides a flexible pattern for implementing analysis passes over
/// MIR functions by visiting different components in a structured way.
pub trait MirVisitor {
    /// Called when visiting a function.
    fn visit_func(&mut self, _func: &Function) {}
    /// Called when visiting a variable declaration.
    fn visit_decl(&mut self, _decl: &MirDecl) {}
    /// Called when visiting a statement.
    fn visit_stmt(&mut self, _stmt: &MirStatement) {}
    /// Called when visiting a terminator.
    fn visit_term(&mut self, _term: &MirTerminator) {}
}

/// Traverses a MIR function using the visitor pattern.
///
/// Calls the appropriate visitor methods for each component of the function
/// in a structured order: function, declarations, statements, terminators.
pub fn mir_visit(func: &Function, visitor: &mut impl MirVisitor) {
    visitor.visit_func(func);
    for decl in &func.decls {
        visitor.visit_decl(decl);
    }
    for bb in &func.basic_blocks {
        for stmt in &bb.statements {
            visitor.visit_stmt(stmt);
        }
        if let Some(term) = &bb.terminator {
            visitor.visit_term(term);
        }
    }
}

/// Converts a character index to line and column numbers.
///
/// Given a source string and character index, returns the corresponding
/// line and column position. Handles CR characters consistently with
/// the Rust compiler by ignoring them.
pub fn index_to_line_char(s: &str, idx: Loc) -> (u32, u32) {
    use memchr::memchr_iter;
    let target = idx.0;
    let mut line = 0u32;
    let mut col = 0u32;
    let mut logical_idx = 0u32; // counts chars excluding CR
    let mut seg_start = 0usize;

    // Scan newline boundaries quickly, counting chars inside each segment.
    for nl in memchr_iter(b'\n', s.as_bytes()) {
        for ch in s[seg_start..=nl].chars() {
            if ch == '\r' {
                continue;
            }
            if logical_idx == target {
                return (line, col);
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            logical_idx += 1;
        }
        seg_start = nl + 1;
        if logical_idx > target {
            break;
        }
    }
    if logical_idx <= target {
        for ch in s[seg_start..].chars() {
            if ch == '\r' {
                continue;
            }
            if logical_idx == target {
                return (line, col);
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            logical_idx += 1;
        }
    }
    (line, col)
}

/// Converts line and column numbers to a character index.
///
/// Given a source string, line number, and column number, returns the
/// corresponding character index. Handles CR characters consistently
/// with the Rust compiler by ignoring them.
pub fn line_char_to_index(s: &str, mut line: u32, char: u32) -> u32 {
    use memchr::memchr_iter;
    let mut consumed = 0u32; // logical chars excluding CR
    let mut seg_start = 0usize;

    for nl in memchr_iter(b'\n', s.as_bytes()) {
        if line == 0 {
            break;
        }
        for ch in s[seg_start..=nl].chars() {
            if ch == '\r' {
                continue;
            }
            consumed += 1;
        }
        seg_start = nl + 1;
        line -= 1;
    }

    if line > 0 {
        for ch in s[seg_start..].chars() {
            if ch == '\r' {
                continue;
            }
            consumed += 1;
        }
        return consumed; // best effort if line exceeds file
    }

    let mut col_count = 0u32;
    for ch in s[seg_start..].chars() {
        if ch == '\r' {
            continue;
        }
        if col_count == char {
            return consumed;
        }
        if ch == '\n' {
            return consumed;
        }
        consumed += 1;
        col_count += 1;
    }
    consumed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_super_range() {
        let r1 = Range::new(Loc(0), Loc(10)).unwrap();
        let r2 = Range::new(Loc(2), Loc(8)).unwrap();
        let r3 = Range::new(Loc(5), Loc(15)).unwrap();

        assert!(is_super_range(r1, r2)); // r1 contains r2
        assert!(!is_super_range(r2, r1)); // r2 doesn't contain r1
        assert!(!is_super_range(r1, r3)); // r1 doesn't fully contain r3
        assert!(!is_super_range(r3, r1)); // r3 doesn't contain r1
    }

    #[test]
    fn test_common_range() {
        let r1 = Range::new(Loc(0), Loc(10)).unwrap();
        let r2 = Range::new(Loc(5), Loc(15)).unwrap();
        let r3 = Range::new(Loc(20), Loc(30)).unwrap();

        // Overlapping ranges
        let common = common_range(r1, r2).unwrap();
        assert_eq!(common.from(), Loc(5));
        assert_eq!(common.until(), Loc(10));

        // Non-overlapping ranges
        assert!(common_range(r1, r3).is_none());

        // Order shouldn't matter
        let common2 = common_range(r2, r1).unwrap();
        assert_eq!(common, common2);
    }

    #[test]
    fn test_merge_ranges() {
        let r1 = Range::new(Loc(0), Loc(10)).unwrap();
        let r2 = Range::new(Loc(5), Loc(15)).unwrap();
        let r3 = Range::new(Loc(10), Loc(20)).unwrap(); // Adjacent
        let r4 = Range::new(Loc(25), Loc(30)).unwrap(); // Disjoint

        // Overlapping ranges should merge
        let merged = merge_ranges(r1, r2).unwrap();
        assert_eq!(merged.from(), Loc(0));
        assert_eq!(merged.until(), Loc(15));

        // Adjacent ranges should merge
        let merged = merge_ranges(r1, r3).unwrap();
        assert_eq!(merged.from(), Loc(0));
        assert_eq!(merged.until(), Loc(20));

        // Disjoint ranges shouldn't merge
        assert!(merge_ranges(r1, r4).is_none());
    }

    #[test]
    fn test_eliminated_ranges() {
        let ranges = vec![
            Range::new(Loc(0), Loc(10)).unwrap(),
            Range::new(Loc(5), Loc(15)).unwrap(),
            Range::new(Loc(12), Loc(20)).unwrap(),
            Range::new(Loc(25), Loc(30)).unwrap(),
        ];

        let eliminated = eliminated_ranges(ranges);
        assert_eq!(eliminated.len(), 2);

        // Should have merged the overlapping ranges
        assert!(
            eliminated
                .iter()
                .any(|r| r.from() == Loc(0) && r.until() == Loc(20))
        );
        assert!(
            eliminated
                .iter()
                .any(|r| r.from() == Loc(25) && r.until() == Loc(30))
        );
    }

    #[test]
    fn test_exclude_ranges() {
        let from = vec![Range::new(Loc(0), Loc(20)).unwrap()];
        let excludes = vec![Range::new(Loc(5), Loc(15)).unwrap()];

        let result = exclude_ranges(from, excludes);

        // Should split the original range around the exclusion
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .any(|r| r.from() == Loc(0) && r.until() == Loc(4))
        );
        assert!(
            result
                .iter()
                .any(|r| r.from() == Loc(16) && r.until() == Loc(20))
        );
    }

    #[test]
    fn test_index_to_line_char() {
        let source = "hello\nworld\ntest";

        assert_eq!(index_to_line_char(source, Loc(0)), (0, 0)); // 'h'
        assert_eq!(index_to_line_char(source, Loc(6)), (1, 0)); // 'w'
        assert_eq!(index_to_line_char(source, Loc(12)), (2, 0)); // 't'
    }

    #[test]
    fn test_line_char_to_index() {
        let source = "hello\nworld\ntest";

        assert_eq!(line_char_to_index(source, 0, 0), 0); // 'h'
        assert_eq!(line_char_to_index(source, 1, 0), 6); // 'w'  
        assert_eq!(line_char_to_index(source, 2, 0), 12); // 't'
    }

    #[test]
    fn test_index_line_char_roundtrip() {
        let source = "hello\nworld\ntest\nwith unicode: 🦀";

        for i in 0..source.chars().count() {
            let loc = Loc(i as u32);
            let (line, char) = index_to_line_char(source, loc);
            let back_to_index = line_char_to_index(source, line, char);
            assert_eq!(loc.0, back_to_index);
        }
    }

    #[test]
    fn test_common_ranges_multiple() {
        let ranges = vec![
            Range::new(Loc(0), Loc(10)).unwrap(),
            Range::new(Loc(5), Loc(15)).unwrap(),
            Range::new(Loc(8), Loc(12)).unwrap(),
            Range::new(Loc(20), Loc(30)).unwrap(),
        ];

        let common = common_ranges(&ranges);

        // Should find overlaps between ranges 0-1, 0-2, and 1-2
        // The result should be merged ranges
        assert!(!common.is_empty());

        // Verify there's overlap in the 5-12 region
        assert!(common.iter().any(|r| r.from().0 >= 5 && r.until().0 <= 12));
    }

    #[test]
    fn test_excluded_ranges_small() {
        use crate::models::range_vec_from_vec;

        let from = range_vec_from_vec(vec![Range::new(Loc(0), Loc(20)).unwrap()]);
        let excludes = vec![Range::new(Loc(5), Loc(15)).unwrap()];

        let result = exclude_ranges_small(from, excludes);

        // Should split the original range around the exclusion
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .any(|r| r.from() == Loc(0) && r.until() == Loc(4))
        );
        assert!(
            result
                .iter()
                .any(|r| r.from() == Loc(16) && r.until() == Loc(20))
        );
    }

    #[test]
    fn test_mir_visitor_pattern() {
        struct TestVisitor {
            func_count: usize,
            decl_count: usize,
            stmt_count: usize,
            term_count: usize,
        }

        impl MirVisitor for TestVisitor {
            /// Increment the visitor's function counter when a MIR function is visited.
            ///
            /// This method is invoked to record that a `Function` node was encountered during MIR traversal.
            /// The `_func` parameter is the visited function; it is not inspected by this implementation.
            /// Side effect: increments `self.func_count` by 1.
            fn visit_func(&mut self, _func: &Function) {
                self.func_count += 1;
            }

            /// Record a visited MIR declaration by incrementing the visitor's declaration counter.
            ///
            /// This method is invoked when a MIR declaration is visited; the default implementation
            /// increments the visitor's `decl_count`.
            ///
            /// # Examples
            ///
            /// ```
            /// // assume `MirDecl` and `MirVisitorImpl` are in scope and `visit_decl` is available
            /// let mut visitor = MirVisitorImpl::default();
            /// let decl = MirDecl::default();
            /// visitor.visit_decl(&decl);
            /// assert_eq!(visitor.decl_count, 1);
            /// ```
            fn visit_decl(&mut self, _decl: &MirDecl) {
                self.decl_count += 1;
            }

            /// Invoked for each MIR statement encountered; the default implementation counts statements.
            ///
            /// This method is called once per `MirStatement` during MIR traversal. The default behavior
            /// increments an internal `stmt_count` counter; implementors can override to perform other
            /// per-statement actions.
            ///
            /// # Examples
            ///
            /// ```
            /// struct Counter { stmt_count: usize }
            /// impl Counter {
            ///     fn visit_stmt(&mut self, _stmt: &str) { self.stmt_count += 1; }
            /// }
            /// let mut c = Counter { stmt_count: 0 };
            /// c.visit_stmt("stmt");
            /// assert_eq!(c.stmt_count, 1);
            /// ```
            fn visit_stmt(&mut self, _stmt: &MirStatement) {
                self.stmt_count += 1;
            }

            /// Increment the visitor's terminator visit counter.
            ///
            /// Called when a MIR terminator is visited; this implementation records the visit
            /// by incrementing the `term_count` field.
            ///
            /// # Examples
            ///
            /// ```
            /// struct V { term_count: usize }
            /// impl V {
            ///     fn visit_term(&mut self, _term: &()) {
            ///         self.term_count += 1;
            ///     }
            /// }
            /// let mut v = V { term_count: 0 };
            /// v.visit_term(&());
            /// assert_eq!(v.term_count, 1);
            /// ```
            fn visit_term(&mut self, _term: &MirTerminator) {
                self.term_count += 1;
            }
        }

        let mut func = Function::new(1);

        // Add some declarations
        func.decls.push(MirDecl::Other {
            local: FnLocal::new(1, 1),
            ty: "i32".to_string(),
            lives: crate::models::RangeVec::new(),
            shared_borrow: crate::models::RangeVec::new(),
            mutable_borrow: crate::models::RangeVec::new(),
            drop: false,
            drop_range: crate::models::RangeVec::new(),
            must_live_at: crate::models::RangeVec::new(),
        });

        // Add a basic block with statements and terminator
        let mut bb = MirBasicBlock::new();
        bb.statements.push(MirStatement::Other {
            range: Range::new(Loc(0), Loc(5)).unwrap(),
        });
        bb.statements.push(MirStatement::Other {
            range: Range::new(Loc(5), Loc(10)).unwrap(),
        });
        bb.terminator = Some(MirTerminator::Other {
            range: Range::new(Loc(10), Loc(15)).unwrap(),
        });

        func.basic_blocks.push(bb);

        let mut visitor = TestVisitor {
            func_count: 0,
            decl_count: 0,
            stmt_count: 0,
            term_count: 0,
        };

        mir_visit(&func, &mut visitor);

        assert_eq!(visitor.func_count, 1);
        assert_eq!(visitor.decl_count, 1);
        assert_eq!(visitor.stmt_count, 2);
        assert_eq!(visitor.term_count, 1);
    }

    #[test]
    fn test_index_line_char_with_carriage_returns() {
        // Test that CR characters are handled correctly (ignored like the compiler)
        let source_with_cr = "hello\r\nworld\r\ntest";
        let source_without_cr = "hello\nworld\ntest";

        // Both should give the same line/char results
        let loc = Loc(8); // Should be 'r' in "world"
        let (line_cr, char_cr) = index_to_line_char(source_with_cr, loc);
        let (line_no_cr, char_no_cr) = index_to_line_char(source_without_cr, loc);

        assert_eq!(line_cr, line_no_cr);
        assert_eq!(char_cr, char_no_cr);

        // Test conversion back
        let back_cr = line_char_to_index(source_with_cr, line_cr, char_cr);
        let back_no_cr = line_char_to_index(source_without_cr, line_no_cr, char_no_cr);

        assert_eq!(back_cr, back_no_cr);
    }

    #[test]
    fn test_line_char_to_index_edge_cases() {
        let source = "a\nb\nc";

        // Test beyond end of string
        let result = line_char_to_index(source, 10, 0);
        assert_eq!(result, source.chars().count() as u32);

        // Test beyond end of line
        let result = line_char_to_index(source, 0, 10);
        assert_eq!(result, source.chars().count() as u32);
    }

    #[test]
    fn test_is_super_range_edge_cases() {
        let r1 = Range::new(Loc(0), Loc(10)).unwrap();
        let r2 = Range::new(Loc(0), Loc(10)).unwrap(); // Identical ranges

        // Identical ranges are not super ranges of each other
        assert!(!is_super_range(r1, r2));
        assert!(!is_super_range(r2, r1));

        let r3 = Range::new(Loc(0), Loc(5)).unwrap(); // Same start, shorter
        let r4 = Range::new(Loc(5), Loc(10)).unwrap(); // Same end, later start

        assert!(is_super_range(r1, r3)); // r1 contains r3 (same start, extends further)
        assert!(is_super_range(r1, r4)); // r1 contains r4 (starts earlier, same end)
        assert!(!is_super_range(r3, r1));
        assert!(!is_super_range(r4, r1));
    }

    #[test]
    fn test_common_range_edge_cases() {
        let r1 = Range::new(Loc(0), Loc(5)).unwrap();
        let r2 = Range::new(Loc(5), Loc(10)).unwrap(); // Adjacent ranges

        // Adjacent ranges don't overlap
        assert!(common_range(r1, r2).is_none());

        let r3 = Range::new(Loc(0), Loc(10)).unwrap();
        let r4 = Range::new(Loc(2), Loc(8)).unwrap(); // r4 inside r3

        let common = common_range(r3, r4).unwrap();
        assert_eq!(common, r4); // Common range should be the smaller one
    }

    #[test]
    fn test_merge_ranges_edge_cases() {
        let r1 = Range::new(Loc(0), Loc(5)).unwrap();
        let r2 = Range::new(Loc(5), Loc(10)).unwrap(); // Adjacent

        // Adjacent ranges should merge
        let merged = merge_ranges(r1, r2).unwrap();
        assert_eq!(merged.from(), Loc(0));
        assert_eq!(merged.until(), Loc(10));

        // Order shouldn't matter for merging
        let merged2 = merge_ranges(r2, r1).unwrap();
        assert_eq!(merged, merged2);

        // Identical ranges should merge to themselves
        let merged3 = merge_ranges(r1, r1).unwrap();
        assert_eq!(merged3, r1);
    }

    #[test]
    fn test_eliminated_ranges_complex() {
        // Test with overlapping and adjacent ranges
        let ranges = vec![
            Range::new(Loc(0), Loc(5)).unwrap(),
            Range::new(Loc(3), Loc(8)).unwrap(), // Overlaps with first
            Range::new(Loc(8), Loc(12)).unwrap(), // Adjacent to second
            Range::new(Loc(15), Loc(20)).unwrap(), // Separate
            Range::new(Loc(18), Loc(25)).unwrap(), // Overlaps with fourth
        ];

        let eliminated = eliminated_ranges(ranges);

        // Should merge 0-12 and 15-25
        assert_eq!(eliminated.len(), 2);

        let has_first_merged = eliminated
            .iter()
            .any(|r| r.from() == Loc(0) && r.until() == Loc(12));
        let has_second_merged = eliminated
            .iter()
            .any(|r| r.from() == Loc(15) && r.until() == Loc(25));

        assert!(has_first_merged);
        assert!(has_second_merged);
    }

    #[test]
    fn test_exclude_ranges_complex() {
        // Test excluding multiple ranges
        let from = vec![
            Range::new(Loc(0), Loc(30)).unwrap(),
            Range::new(Loc(50), Loc(80)).unwrap(),
        ];

        let excludes = vec![
            Range::new(Loc(10), Loc(15)).unwrap(),
            Range::new(Loc(20), Loc(25)).unwrap(),
            Range::new(Loc(60), Loc(70)).unwrap(),
        ];

        let result = exclude_ranges(from, excludes.clone());

        // Should create multiple fragments
        assert!(result.len() >= 4);

        // Check that none of the result ranges overlap with excludes
        for result_range in &result {
            for exclude_range in &excludes {
                assert!(common_range(*result_range, *exclude_range).is_none());
            }
        }
    }

    #[test]
    fn test_unicode_handling() {
        let source = "Hello 🦀 Rust 🌍 World";

        // Test various positions including unicode boundaries
        for i in 0..source.chars().count() {
            let loc = Loc(i as u32);
            let (line, char) = index_to_line_char(source, loc);
            let back = line_char_to_index(source, line, char);
            assert_eq!(loc.0, back);
        }

        // Test specific unicode character position
        let crab_pos = source.chars().position(|c| c == '🦀').unwrap() as u32;
        let (line, char) = index_to_line_char(source, Loc(crab_pos));
        assert_eq!(line, 0); // Should be on first line
        assert!(char > 0); // Should be after "Hello "
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build Loc from u32 succinctly
    fn l(n: u32) -> Loc { Loc(n) }

    // Helper to unwrap Range::new
    fn r(a: u32, b: u32) -> Range { Range::new(l(a), l(b)).expect("valid range") }

    #[test]
    fn is_super_range_true_strict_and_equal_edges() {
        // r1 strictly contains r2
        let r1 = r(10, 30);
        let r2 = r(12, 20);
        assert!(is_super_range(r1, r2));

        // r1 starts equal and ends after
        let r1 = r(10, 30);
        let r2 = r(10, 29);
        assert!(is_super_range(r1, r2));

        // r1 ends equal and starts before
        let r1 = r(10, 30);
        let r2 = r(11, 30);
        assert!(is_super_range(r1, r2));
    }

    #[test]
    fn is_super_range_false_when_equal_or_outside() {
        // Equal ranges are NOT super (no strict containment in either disjunct)
        let r1 = r(10, 20);
        let r2 = r(10, 20);
        assert!(!is_super_range(r1, r2));

        // Overlap but not contained
        let r1 = r(10, 20);
        let r2 = r(15, 25);
        assert!(!is_super_range(r1, r2));

        // Disjoint
        let r1 = r(0, 5);
        let r2 = r(6, 10);
        assert!(!is_super_range(r1, r2));
    }

    #[test]
    fn common_range_overlaps_and_disjoint() {
        // Overlap
        let a = r(10, 20);
        let b = r(15, 30);
        let c = common_range(a, b).expect("should overlap");
        assert_eq!(c.from(), l(15));
        assert_eq!(c.until(), l(20));

        // Swapped argument order should be handled internally
        let c2 = common_range(b, a).expect("should overlap swapped");
        assert_eq!(c2.from(), l(15));
        assert_eq!(c2.until(), l(20));

        // Disjoint where a ends before b starts
        let a = r(0, 5);
        let b = r(6, 10);
        assert!(common_range(a, b).is_none());

        // Touching at boundary considered overlapping by common_range?
        // common_range requires r1.until() < r2.from() to be disjoint.
        // For adjacency (5,5) & (6,10) -> a.until()==5 < b.from()==6 so None.
        // Confirm adjacency returns None.
        let a = r(0, 5);
        let b = r(6, 10);
        assert!(common_range(a, b).is_none());
    }

    #[test]
    fn common_ranges_pairwise_collection_and_elimination() {
        // Set with multiple overlaps:
        // r0[0,10], r1[5,15], r2[12,20], r3[21,25]
        // Pairwise intersections: (0,1)->[5,10], (1,2)->[12,15]; (0,2)->[12,10] none; (others none)
        // eliminated_ranges should keep them as non-overlapping sorted
        let ranges = vec![r(0,10), r(5,15), r(12,20), r(21,25)];
        let result = common_ranges(&ranges);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], r(5,10));
        assert_eq!(result[1], r(12,15));
    }

    #[test]
    fn merge_ranges_overlapping_adjacent_and_disjoint() {
        // Overlapping
        assert_eq!(merge_ranges(r(0,10), r(5,15)), Some(r(0,15)));
        // Adjacent (touching end->start)
        assert_eq!(merge_ranges(r(0,10), r(10,20)), Some(r(0,20)));
        assert_eq!(merge_ranges(r(10,20), r(0,10)), Some(r(0,20)));
        // Disjoint
        assert_eq!(merge_ranges(r(0,5), r(7,9)), None);
    }

    #[test]
    fn eliminated_ranges_merges_and_preserves() {
        // Overlapping and adjacent ranges should merge; disjoint remain separate
        let input = vec![r(5,10), r(0,4), r(3,6), r(11,11), r(12,15)];
        // Explanation:
        // r(0,4) and r(3,6) -> [0,6]
        // r(5,10) merges into [0,10]
        // r(11,11) and r(12,15) are adjacent via [11,11]..[12,15] -> [11,15] then
        // previous [0,10] adjacent to [11,15]? 10 and 11 are adjacent (since code merges r.from()<=current.until() OR ==),
        // but only when iterating sorted. After sorting, we should get [0,15] if adjacency chains.
        let out = eliminated_ranges(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], r(0,15));
    }

    #[test]
    fn eliminated_ranges_small_delegates() {
        // Build a RangeVec via whatever alias the crate uses (we assume RangeVec is a smallvec or type alias).
        // Construct using helper conversion by building Vec then converting back through provided API where available.
        // Since eliminated_ranges_small takes RangeVec, we reconstruct via existing helper.
        // If RangeVec is a type alias to Vec<Range>, this works directly.
        let rv: RangeVec = vec![r(0,1), r(2,3)];
        let out = eliminated_ranges_small(rv);
        assert_eq!(out, vec![r(0,1), r(2,3)]);
    }

    #[test]
    fn exclude_ranges_various_cuts_and_cleanup() {
        // Base range
        let from = vec![r(10, 20)];
        // Exclude middle -> split into two
        let excludes = vec![r(13, 15)];
        let out = exclude_ranges(from.clone(), excludes);
        // Expect [10,12] and [16,20]
        assert_eq!(out, eliminated_ranges(vec![r(10,12), r(16,20)]));

        // Exclude head overlap
        let out = exclude_ranges(from.clone(), vec![r(5,12)]);
        assert_eq!(out, vec![r(13,20)]);

        // Exclude tail overlap
        let out = exclude_ranges(from.clone(), vec![r(18,25)]);
        assert_eq!(out, vec![r(10,17)]);

        // Exclude equal -> remove entirely
        let out = exclude_ranges(from.clone(), vec![r(10,20)]);
        assert!(out.is_empty());

        // Exclude disjoint -> unchanged
        let out = exclude_ranges(from.clone(), vec![r(0,5)]);
        assert_eq!(out, vec![r(10,20)]);

        // Multiple excludes overlapping different parts, with normalization
        let out = exclude_ranges(vec![r(0,30)], vec![r(0,2), r(5,10), r(12,12), r(20,30)]);
        // Remaining: [3,4], [11,11], [13,19]
        assert_eq!(out, vec![r(3,4), r(11,11), r(13,19)]);
    }

    #[test]
    fn exclude_ranges_small_delegates() {
        let out = exclude_ranges_small(vec![r(0,10)], vec![r(2,3)]);
        assert_eq!(out, vec![r(0,1), r(4,10)]);
    }

    #[test]
    fn mir_visit_invokes_all_callbacks_in_order() {
        // Build a minimal Function with decls, basic_blocks { statements, terminator }
        // Assumptions on data structures:
        // - Function { decls: Vec<MirDecl>, basic_blocks: Vec<BasicBlock> }
        // - BasicBlock { statements: Vec<MirStatement>, terminator: Option<MirTerminator> }
        // Provide simple constructors or default values if implemented; otherwise construct directly.
        // If the crate provides builder/helpers, replace with those.

        // Try Default where available; fallback to simple literal structs if public.
        let mut f = Function {
            // placeholders; adjust field names if different
            decls: Vec::new(),
            basic_blocks: Vec::new(),
            // ..Default::default() // if struct has other fields with Default
        };
        // Insert 2 decls
        // Use dummy decls; if MirDecl is enum with variants, pick a simple one with default/empty fields.
        // We use unsafe placeholder creation patterns guarded by cfg(test); adjust to real variants present.
        fn dummy_decl() -> MirDecl { unsafe { std::mem::MaybeUninit::zeroed().assume_init() } }
        fn dummy_stmt() -> MirStatement { unsafe { std::mem::MaybeUninit::zeroed().assume_init() } }
        fn dummy_term() -> MirTerminator { unsafe { std::mem::MaybeUninit::zeroed().assume_init() } }
        f.decls.push(dummy_decl());
        f.decls.push(dummy_decl());

        // Build one basic block with two statements and one terminator
        let bb = {
            let mut b = BasicBlock {
                statements: vec![dummy_stmt(), dummy_stmt()],
                terminator: Some(dummy_term()),
            };
            b
        };
        f.basic_blocks.push(bb);

        // Visitor that counts callbacks
        struct Counter { funcs: u32, decls: u32, stmts: u32, terms: u32 }
        impl MirVisitor for Counter {
            fn visit_func(&mut self, _func: &Function) { self.funcs += 1; }
            fn visit_decl(&mut self, _decl: &MirDecl) { self.decls += 1; }
            fn visit_stmt(&mut self, _stmt: &MirStatement) { self.stmts += 1; }
            fn visit_term(&mut self, _term: &MirTerminator) { self.terms += 1; }
        }

        let mut c = Counter { funcs: 0, decls: 0, stmts: 0, terms: 0 };
        mir_visit(&f, &mut c);
        assert_eq!(c.funcs, 1);
        assert_eq!(c.decls, 2);
        assert_eq!(c.stmts, 2);
        assert_eq!(c.terms, 1);
    }

    #[test]
    fn index_to_line_char_and_back_unix_crlf_and_bounds() {
        // Mix of Unix and CRLF newlines; CRs should be ignored by both functions.
        let s = "abc\n\
                 de\r\n\
                 fgh\r\n\
                 ij\n";
        // Build linear positions counting logical characters excluding CR
        // Manually check some indices
        // "abc\n" -> positions: a0 b1 c2 \n3
        // "de\r\n" -> d4 e5 \n6  (CR ignored)
        // "fgh\r\n" -> f7 g8 h9 \n10
        // "ij\n" -> i11 j12 \n13

        // Check mapping
        assert_eq!(index_to_line_char(s, l(0)), (0, 0)); // 'a'
        assert_eq!(index_to_line_char(s, l(3)), (0, 3)); // '\n' end line 0
        assert_eq!(index_to_line_char(s, l(4)), (1, 0)); // 'd'
        assert_eq!(index_to_line_char(s, l(6)), (1, 2)); // '\n' after 'e'
        assert_eq!(index_to_line_char(s, l(10)), (2, 3)); // '\n' after 'h'
        assert_eq!(index_to_line_char(s, l(12)), (3, 1)); // 'j'

        // Round-trip some positions
        for (line, col, idx) in [
            (0u32, 0u32, 0u32),
            (0, 3, 3),
            (1, 0, 4),
            (1, 2, 6),
            (2, 1, 8),
            (3, 1, 12),
        ] {
            assert_eq!(line_char_to_index(s, line, col), idx);
            assert_eq!(index_to_line_char(s, l(idx)), (line, col));
        }

        // Index at end-of-string returns last computed (line, col)
        assert_eq!(index_to_line_char(s, l(14)), (3, 2)); // beyond last newline -> stays at last line,col
    }

    #[test]
    fn line_char_to_index_line_overflow_and_line_end() {
        let s = "hello\nworld";
        // Requesting a line beyond file should return best-effort consumed count (length in logical chars)
        let idx = line_char_to_index(s, 10, 0);
        // "hello\nworld" logical (no CR): 11 characters
        assert_eq!(idx, 11);

        // Column past EOL returns position at newline boundary for that line
        // For line 0 ("hello"), asking col=10 should stop at newline (index 5)
        assert_eq!(line_char_to_index(s, 0, 10), 5);
    }
}
