//! Tests for index_to_line_char and line_char_to_index in src/utils.rs
//! Framework: Rust built-in test harness (#[test])

#[allow(unused_imports)]
use crate::utils::{index_to_line_char, line_char_to_index};
#[allow(unused_imports)]
use crate::utils::Loc;

// Helper to create Loc
fn loc(i: u32) -> Loc { Loc(i) }

#[test]
fn index_to_line_char_simple() {
    let s = "abc";
    assert_eq!(index_to_line_char(s, loc(0)), (0,0));
    assert_eq!(index_to_line_char(s, loc(1)), (0,1));
    assert_eq!(index_to_line_char(s, loc(2)), (0,2));
    // position after last char returns end-of-string coordinates
    assert_eq!(index_to_line_char(s, loc(3)), (0,3));
}

#[test]
fn index_to_line_char_newlines() {
    let s = "ab\ncd\nef";
    // indexes (ignoring CR): a:0 b:1 \n:2 c:3 d:4 \n:5 e:6 f:7
    assert_eq!(index_to_line_char(s, loc(0)), (0,0)); // before 'a'
    assert_eq!(index_to_line_char(s, loc(2)), (0,2)); // before '\n'
    assert_eq!(index_to_line_char(s, loc(3)), (1,0)); // at start of line 1 before 'c'
    assert_eq!(index_to_line_char(s, loc(5)), (1,2)); // before second '\n'
    assert_eq!(index_to_line_char(s, loc(7)), (2,1)); // before 'f'
    assert_eq!(index_to_line_char(s, loc(8)), (2,2)); // end
}

#[test]
fn index_to_line_char_ignores_cr() {
    let s = "a\r\nb\r\nc";
    // chars considered for indexing: 'a','\n','b','\n','c'  => indexes 0..4
    assert_eq!(index_to_line_char(s, loc(0)), (0,0));
    assert_eq!(index_to_line_char(s, loc(1)), (0,1)); // before '\n'
    assert_eq!(index_to_line_char(s, loc(2)), (1,0)); // at line 1 col 0 before 'b'
    assert_eq!(index_to_line_char(s, loc(4)), (2,1)); // before 'c' end
}

#[test]
fn line_char_to_index_roundtrip_simple() {
    let s = "abc";
    for i in 0..=3 {
        let (line, col) = (0, i);
        let idx = line_char_to_index(s, line, col);
        assert_eq!(idx, i);
        assert_eq!(index_to_line_char(s, Loc(idx)), (line, col));
    }
}

#[test]
fn line_char_to_index_newlines() {
    let s = "ab\ncd\nef";
    // targets: (0,0)->0, (0,2)->2, (1,0)->3, (1,2)->5, (2,2)->8
    assert_eq!(line_char_to_index(s, 0, 0), 0);
    assert_eq!(line_char_to_index(s, 0, 2), 2);
    assert_eq!(line_char_to_index(s, 1, 0), 3);
    assert_eq!(line_char_to_index(s, 1, 2), 5);
    assert_eq!(line_char_to_index(s, 2, 2), 8);
}

#[test]
fn line_char_to_index_ignores_cr_and_roundtrips() {
    let s = "a\r\nb\r\nc"; // '\r' ignored
    // expected mapping counts only 'a','\n','b','\n','c'
    assert_eq!(line_char_to_index(s, 0, 0), 0);
    assert_eq!(line_char_to_index(s, 0, 1), 1); // before '\n'
    assert_eq!(line_char_to_index(s, 1, 0), 2);
    assert_eq!(line_char_to_index(s, 1, 1), 3);
    assert_eq!(line_char_to_index(s, 2, 1), 4);
    // roundtrip spot-check
    let idx = line_char_to_index(s, 1, 1);
    assert_eq!(super::index_to_line_char(s, Loc(idx)), (1,1));
}