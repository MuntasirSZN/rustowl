//! Additional coverage for src/models.rs focusing on edge cases and behaviors emphasized in the PR diff.
//!
//! Test framework: Rust's built-in test harness (#[test]) with standard assertions.
//! Notes:
//! - We specifically exercise Loc byte->char conversion with Unicode and CR filtering,
//!   saturating arithmetic (+/- i32) including i32::MIN behavior under debug/release,
//!   Range validation, MirVariables dedup/order, Crate and Workspace merging dedup semantics,
//!   MirStatement/MirRval range forwarding, and RangeVec conversions beyond inline capacity.

use std::collections::{HashMap, HashSet};

// In integration tests we import the library crate by its package name (Cargo.toml: name = "rustowl").
use rustowl::models::*;

#[test]
fn loc_new_handles_offset_saturation_and_empty_source() {
    // When offset > byte_pos, saturating_sub should yield 0 byte position.
    let loc = Loc::new("", 3, 10);
    assert_eq!(u32::from(loc), 0);

    // Non-empty source but offset still forces saturation to 0
    let loc2 = Loc::new("abc", 2, 5);
    assert_eq!(u32::from(loc2), 0);
}

#[test]
fn loc_new_multibyte_boundaries_and_cr_filtering() {
    // "a🦀b" bytes: 'a'(1), '🦀'(4), 'b'(1) → chars (excluding CR): a, 🦀, b
    let s = "a🦀b";
    assert_eq!(u32::from(Loc::new(s, 0, 0)), 0); // start
    assert_eq!(u32::from(Loc::new(s, 1, 0)), 1); // after 'a'
    for pos in 2..=4 {
        // mid-emoji should not advance char_count past emoji
        assert_eq!(u32::from(Loc::new(s, pos, 0)), 1, "byte_pos {} within emoji", pos);
    }
    assert_eq!(u32::from(Loc::new(s, 5, 0)), 2); // end of emoji
    assert_eq!(u32::from(Loc::new(s, 6, 0)), 3); // full length

    // CR filtering: '\r' should not count as a character
    let s_cr = "a\rb";
    assert_eq!(u32::from(Loc::new(s_cr, 2, 0)), 1); // 'a'(1) + '\r'(1 byte) -> still 1 char
    let s_no_cr = "a\nb";
    assert_eq!(u32::from(Loc::new(s_no_cr, 2, 0)), 2); // '\n' counts as a character
}

#[test]
fn loc_add_sub_additional_extremes() {
    let zero = Loc(0);
    // Subtract positive from zero saturates to zero
    assert_eq!(u32::from(zero - 1), 0);
    // Add negative to zero saturates to zero
    assert_eq!(u32::from(zero + (-1)), 0);

    // Large positive add saturates at u32::MAX
    let near_max = Loc(u32::MAX - 1);
    assert_eq!(u32::from(near_max + 10), u32::MAX);

    // Subtract negative equals addition
    let base = Loc(2);
    assert_eq!(u32::from(base - (-3)), 5);
}

// Document current behavior for i32::MIN handling in Add/Sub implementations.
// In debug builds, negating i32::MIN overflows and should panic; in release it wraps.
#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn loc_add_i32_min_panics_in_debug() {
    let _ = Loc(5) + i32::MIN;
}

#[cfg(not(debug_assertions))]
#[test]
fn loc_add_i32_min_release_behaves_saturating() {
    let r = Loc(5) + i32::MIN;
    // In release, -i32::MIN wraps to i32::MIN (0x80000000) as u32, saturating_sub yields 0.
    assert_eq!(u32::from(r), 0);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn loc_sub_i32_min_panics_in_debug() {
    let _ = Loc(5) - i32::MIN;
}

#[cfg(not(debug_assertions))]
#[test]
fn loc_sub_i32_min_release_behaves_saturating() {
    let r = Loc(5) - i32::MIN;
    // In release, this becomes saturating_add(2147483648)
    assert_eq!(u32::from(r), 2147483653);
}

#[test]
fn loc_from_conversions_roundtrip() {
    let l: Loc = 123u32.into();
    let v: u32 = l.into();
    assert_eq!(v, 123);
}

#[test]
fn range_minimal_valid_and_invalid_cases() {
    // Minimal valid
    let r = Range::new(Loc(0), Loc(1)).expect("valid minimal range");
    assert_eq!(r.size(), 1);
    assert_eq!(u32::from(r.from()), 0);
    assert_eq!(u32::from(r.until()), 1);

    // Invalid: until <= from
    assert!(Range::new(Loc(1), Loc(1)).is_none());
    assert!(Range::new(Loc(2), Loc(1)).is_none());
}

#[test]
fn mir_variables_insertion_order_and_dedup() {
    let mut vars = MirVariables::new();

    let v2 = MirVariable::User {
        index: 2,
        live: Range::new(Loc(0), Loc(1)).unwrap(),
        dead: Range::new(Loc(1), Loc(2)).unwrap(),
    };
    let v1 = MirVariable::Other {
        index: 1,
        live: Range::new(Loc(2), Loc(3)).unwrap(),
        dead: Range::new(Loc(3), Loc(4)).unwrap(),
    };
    let v1_dup = MirVariable::User {
        index: 1,
        live: Range::new(Loc(10), Loc(11)).unwrap(),
        dead: Range::new(Loc(11), Loc(12)).unwrap(),
    };

    // Push in order: 2, 1, 1(dup)
    vars.push(v2.clone());
    vars.push(v1.clone());
    vars.push(v1_dup);

    let out = vars.clone().to_vec();
    // IndexMap preserves insertion order of first-seen keys.
    assert_eq!(out.len(), 2);
    match (&out[0], &out[1]) {
        (MirVariable::User { index: 2, .. }, MirVariable::Other { index: 1, .. }) => {}
        _ => panic!("Unexpected order or variants: {:?}", out),
    }
}

#[test]
fn crate_merge_dedup_within_other_and_across_existing() {
    let mut c1 = Crate(FoldIndexMap::default());
    let mut f1 = File::new();
    f1.items.push(Function::new(1));
    c1.0.insert("x.rs".to_string(), f1);

    let mut c2 = Crate(FoldIndexMap::default());
    let mut f2 = File::new();
    // Duplicate '1' appears twice, plus a new '2'
    f2.items.push(Function::new(1));
    f2.items.push(Function::new(1));
    f2.items.push(Function::new(2));
    c2.0.insert("x.rs".to_string(), f2);

    c1.merge(c2);

    let merged = &c1.0["x.rs"];
    let mut ids: Vec<u32> = merged.items.iter().map(|f| f.fn_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2], "should deduplicate duplicates within other and across crates");
}

#[test]
fn workspace_merge_invokes_crate_merge_for_colliding_crates() {
    let mut w1 = Workspace(FoldIndexMap::default());
    let mut w2 = Workspace(FoldIndexMap::default());

    let mut crate_left = Crate(FoldIndexMap::default());
    let mut file_left = File::new();
    file_left.items.push(Function::new(10));
    crate_left.0.insert("f.rs".into(), file_left);
    w1.0.insert("same".into(), crate_left);

    let mut crate_right = Crate(FoldIndexMap::default());
    let mut file_right = File::new();
    file_right.items.push(Function::new(10)); // dup
    file_right.items.push(Function::new(20)); // new
    crate_right.0.insert("f.rs".into(), file_right);
    w2.0.insert("same".into(), crate_right);

    w1.merge(w2);
    let merged = &w1.0["same"].0["f.rs"];
    let mut ids: Vec<u32> = merged.items.iter().map(|f| f.fn_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![10, 20]);
}

#[test]
fn mir_statement_assign_with_rval_variants_preserves_range() {
    let r = Range::new(Loc(100), Loc(200)).unwrap();
    let local = FnLocal::new(3, 7);

    let s_move = MirStatement::Assign {
        target_local: local,
        range: r,
        rval: Some(MirRval::Move { target_local: local, range: r }),
    };
    assert_eq!(s_move.range(), r);

    let borrow_outlive = Range::new(Loc(150), Loc(160)).unwrap();
    let s_borrow = MirStatement::Assign {
        target_local: local,
        range: r,
        rval: Some(MirRval::Borrow {
            target_local: local,
            range: r,
            mutable: true,
            outlive: Some(borrow_outlive),
        }),
    };
    assert_eq!(s_borrow.range(), r);
}

#[test]
fn fn_local_equality_and_hash_in_set() {
    let a = FnLocal::new(5, 9);
    let b = FnLocal::new(5, 9);
    let c = FnLocal::new(5, 8);

    // HashSet should deduplicate equal keys
    let mut set = HashSet::new();
    set.insert(a);
    set.insert(b);
    set.insert(c);
    assert_eq!(set.len(), 2);
    assert!(set.contains(&FnLocal::new(5, 9)));
    assert!(set.contains(&FnLocal::new(5, 8)));

    // Also verify HashMap key replacement behavior is stable for equal keys
    let mut map = HashMap::new();
    map.insert(a, "first");
    map.insert(b, "second"); // same key should overwrite
    assert_eq!(map.get(&FnLocal::new(5, 9)), Some(&"second"));
}

#[test]
fn range_vec_roundtrip_large() {
    // Larger than smallvec inline capacity to exercise heap path
    let mut v = Vec::new();
    for i in 0..32u32 {
        v.push(Range::new(Loc(i), Loc(i + 1)).unwrap());
    }
    let rv = range_vec_from_vec(v.clone());
    let back = range_vec_into_vec(rv);
    assert_eq!(v, back);
}