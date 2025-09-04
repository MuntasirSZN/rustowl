//! Tests for eliminated_ranges_small and exclude_ranges_small if RangeVec is available.
//! Framework: Rust built-in test harness (#[test])

#[allow(unused_imports)]
use crate::utils::{eliminated_ranges_small, exclude_ranges_small};
#[allow(unused_imports)]
use crate::range::{Range, RangeVec};

fn r(from: i64, until: i64) -> Range {
    Range::new(from, until).expect("valid range")
}

#[test]
fn eliminated_ranges_small_merges() {
    let rv: RangeVec = RangeVec::from_vec(vec![r(1,3), r(4,5), r(10,12)]);
    let out = eliminated_ranges_small(rv);
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|x| x.from()==1 && x.until()==5));
    assert!(out.iter().any(|x| x.from()==10 && x.until()==12));
}

#[test]
fn exclude_ranges_small_basic() {
    let from: RangeVec = RangeVec::from_vec(vec![r(1,10)]);
    let out = exclude_ranges_small(from, vec![r(3,7)]);
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|x| x.from()==1 && x.until()==2));
    assert!(out.iter().any(|x| x.from()==8 && x.until()==10));
}