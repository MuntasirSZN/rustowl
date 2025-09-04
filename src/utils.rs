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
/// Takes a vector of ranges and repeatedly merges overlapping or adjacent
/// ranges until no more merges are possible, returning the minimal set
/// of non-overlapping ranges.
pub fn eliminated_ranges(ranges: Vec<Range>) -> Vec<Range> {
    let mut ranges = ranges;
    let mut i = 0;
    'outer: while i < ranges.len() {
        let mut j = 0;
        while j < ranges.len() {
            if i != j
                && let Some(merged) = merge_ranges(ranges[i], ranges[j])
            {
                ranges[i] = merged;
                ranges.remove(j);
                continue 'outer;
            }
            j += 1;
        }
        i += 1;
    }
    ranges
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
    let mut line = 0;
    let mut col = 0;
    let mut char_idx = 0u32;

    // Process characters directly without allocating a new string
    for c in s.chars() {
        if char_idx == idx.0 {
            return (line, col);
        }

        // Skip CR characters (compiler ignores them)
        if c != '\r' {
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            char_idx += 1;
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
    let mut col = 0;
    let mut char_idx = 0u32;

    // Process characters directly without allocating a new string
    for c in s.chars() {
        if line == 0 && col == char {
            return char_idx;
        }

        // Skip CR characters (compiler ignores them)
        if c != '\r' {
            if c == '\n' && line > 0 {
                line -= 1;
                col = 0;
            } else {
                col += 1;
            }
            char_idx += 1;
        }
    }
    char_idx
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
        assert!(result.iter().any(|r| r.from() == Loc(0) && r.until() == Loc(4)));
        assert!(result.iter().any(|r| r.from() == Loc(16) && r.until() == Loc(20)));
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
            fn visit_func(&mut self, _func: &Function) {
                self.func_count += 1;
            }

            fn visit_decl(&mut self, _decl: &MirDecl) {
                self.decl_count += 1;
            }

            fn visit_stmt(&mut self, _stmt: &MirStatement) {
                self.stmt_count += 1;
            }

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
            Range::new(Loc(3), Loc(8)).unwrap(),   // Overlaps with first
            Range::new(Loc(8), Loc(12)).unwrap(),  // Adjacent to second
            Range::new(Loc(15), Loc(20)).unwrap(), // Separate
            Range::new(Loc(18), Loc(25)).unwrap(), // Overlaps with fourth
        ];

        let eliminated = eliminated_ranges(ranges);
        
        // Should merge 0-12 and 15-25
        assert_eq!(eliminated.len(), 2);
        
        let has_first_merged = eliminated.iter().any(|r| r.from() == Loc(0) && r.until() == Loc(12));
        let has_second_merged = eliminated.iter().any(|r| r.from() == Loc(15) && r.until() == Loc(25));
        
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
