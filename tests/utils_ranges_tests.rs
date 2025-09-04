//! Tests for range utilities in src/utils.rs
//! Framework: Rust built-in test harness (#[test])
use std::cmp::{min, max};

// Heuristic imports: adapt as needed based on actual crate structure.
// We try common paths: crate::utils, crate::range, or top-level re-exports.
#[allow(unused_imports)]
use crate::{
    utils::*,
};
#[allow(unused_imports)]
use crate::utils as _utils_mod;

// Fallback: try bringing Range, Loc into scope from likely modules.
// If your project organizes these differently, adjust these use lines.
#[allow(unused_imports)]
use crate::range::{Range, RangeVec};
#[allow(unused_imports)]
use crate::{Range as CrateRange, Loc as CrateLoc};

// To reduce ambiguity, attempt several import aliases; whichever compiles will be used.
#[allow(unused_imports)]
use crate::utils::{Range as URange, RangeVec as URangeVec, Loc as ULoc};

// Helper fn to construct a Range via Range::new, unwrapping for valid inputs.
fn r(from: i64, until: i64) -> Range {
    Range::new(from, until).expect("valid range")
}

#[test]
fn is_super_range_true_when_strictly_contains_left_edge() {
    let r1 = r(1, 10);
    let r2 = r(2, 10); // r1.from() < r2.from(), r2.until() <= r1.until()
    assert!(is_super_range(r1, r2));
}

#[test]
fn is_super_range_true_when_strictly_contains_right_edge() {
    let r1 = r(1, 10);
    let r2 = r(1, 9); // r1.from() <= r2.from(), r2.until() < r1.until()
    assert!(is_super_range(r1, r2));
}

#[test]
fn is_super_range_false_for_identical_ranges() {
    let r1 = r(5, 15);
    let r2 = r(5, 15);
    assert!(!is_super_range(r1, r2));
}

#[test]
fn is_super_range_false_for_partial_overlap() {
    let r1 = r(1, 5);
    let r2 = r(4, 10);
    assert!(!is_super_range(r1, r2));
}

#[test]
fn is_super_range_false_for_disjoint() {
    let r1 = r(1, 3);
    let r2 = r(5, 7);
    assert!(!is_super_range(r1, r2));
}

#[test]
fn common_range_none_for_disjoint() {
    let r1 = r(1, 3);
    let r2 = r(5, 7);
    assert!(common_range(r1, r2).is_none());
}

#[test]
fn common_range_some_for_overlap() {
    let r1 = r(1, 6);
    let r2 = r(4, 10);
    let c = common_range(r1, r2).expect("has intersection");
    assert_eq!(c.from(), 4);
    assert_eq!(c.until(), 6);
}

#[test]
fn common_range_edge_touching_is_none() {
    // Since inclusive ranges, touching at boundary (3 and 4) is disjoint
    let r1 = r(1, 3);
    let r2 = r(4, 8);
    assert!(common_range(r1, r2).is_none());
}

#[test]
fn merge_ranges_overlapping_merges() {
    let r1 = r(1, 6);
    let r2 = r(4, 10);
    let merged = merge_ranges(r1, r2).expect("should merge");
    assert_eq!((merged.from(), merged.until()), (1, 10));
}

#[test]
fn merge_ranges_adjacent_merges() {
    let r1 = r(1, 3);
    let r2 = r(4, 8);
    let merged = merge_ranges(r1, r2).expect("adjacent should merge");
    assert_eq!((merged.from(), merged.until()), (1, 8));
}

#[test]
fn merge_ranges_disjoint_none() {
    let r1 = r(1, 2);
    let r2 = r(4, 5);
    assert!(merge_ranges(r1, r2).is_none());
}

#[test]
fn eliminated_ranges_merges_chain_until_fixed_point() {
    // 1-3, 4-5 (adjacent chain), 10-12
    let v = vec![r(4,5), r(1,3), r(10,12)];
    let out = eliminated_ranges(v);
    // Expect [1,5] and [10,12]
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|x| x.from()==1 && x.until()==5));
    assert!(out.iter().any(|x| x.from()==10 && x.until()==12));
}

#[test]
fn eliminated_ranges_handles_full_overlap() {
    // overlapping ranges should collapse to containing one
    let v = vec![r(5,15), r(7,9), r(1,20)];
    let out = eliminated_ranges(v);
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].from(), out[0].until()), (1,20));
}

#[test]
fn exclude_ranges_removes_middle_segment_creating_two_pieces() {
    let src = vec![r(1,10)];
    let ex = vec![r(4,6)];
    let out = exclude_ranges(src, ex);
    // Expect [1,3] and [7,10]
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|x| x.from()==1 && x.until()==3));
    assert!(out.iter().any(|x| x.from()==7 && x.until()==10));
}

#[test]
fn exclude_ranges_left_trim() {
    let src = vec![r(1,10)];
    let ex = vec![r(1,3)];
    let out = exclude_ranges(src, ex);
    // Expect [4,10]
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].from(), out[0].until()), (4,10));
}

#[test]
fn exclude_ranges_right_trim() {
    let src = vec![r(1,10)];
    let ex = vec![r(8,10)];
    let out = exclude_ranges(src, ex);
    // Expect [1,7]
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].from(), out[0].until()), (1,7));
}

#[test]
fn exclude_ranges_adjacent_no_change() {
    // Excluding an adjacent disjoint range should have no effect
    let src = vec![r(1,3)];
    let ex = vec![r(4,6)];
    let out = exclude_ranges(src, ex);
    assert_eq!(out.len(), 1);
    assert_eq!((out[0].from(), out[0].until()), (1,3));
}

#[test]
fn exclude_ranges_multiple_sources_and_excludes() {
    let src = vec![r(1,5), r(10,20)];
    let ex = vec![r(3,12), r(15,17)];
    let out = exclude_ranges(src, ex);
    // Stepwise: [1,5] minus [3,12] => [1,2]
    //           [10,20] minus [3,12] => [13,20]
    //           [13,20] minus [15,17] => [13,14], [18,20]
    // Final expected: [1,2], [13,14], [18,20]
    assert_eq!(out.len(), 3);
    assert!(out.iter().any(|x| x.from()==1 && x.until()==2));
    assert!(out.iter().any(|x| x.from()==13 && x.until()==14));
    assert!(out.iter().any(|x| x.from()==18 && x.until()==20));
}