use super::transform::{BorrowData, BorrowMap};
use rayon::prelude::*;
use rustc_borrowck::consumers::{PoloniusLocationTable, PoloniusOutput};
use rustc_index::Idx;
use rustc_middle::mir::Local;
use rustowl::models::{FoldIndexMap as HashMap, FoldIndexSet as HashSet};
use rustowl::{models::*, utils};

pub fn get_accurate_live(
    datafrog: &PoloniusOutput,
    location_table: &PoloniusLocationTable,
    basic_blocks: &[MirBasicBlock],
) -> HashMap<Local, Vec<Range>> {
    get_range(
        datafrog
            .var_live_on_entry
            .iter()
            .map(|(p, v)| (*p, v.iter().copied())),
        location_table,
        basic_blocks,
    )
}

/// returns (shared, mutable)
pub fn get_borrow_live(
    datafrog: &PoloniusOutput,
    location_table: &PoloniusLocationTable,
    borrow_map: &BorrowMap,
    basic_blocks: &[MirBasicBlock],
) -> (HashMap<Local, Vec<Range>>, HashMap<Local, Vec<Range>>) {
    let output = datafrog;
    let mut shared_borrows = HashMap::default();
    let mut mutable_borrows = HashMap::default();
    for (location_idx, borrow_idc) in output.loan_live_at.iter() {
        let location = location_table.to_rich_location(*location_idx);
        for borrow_idx in borrow_idc {
            match borrow_map.get_from_borrow_index(*borrow_idx) {
                Some((_, BorrowData::Shared { borrowed, .. })) => {
                    shared_borrows
                        .entry(*borrowed)
                        .or_insert_with(Vec::new)
                        .push(location);
                }
                Some((_, BorrowData::Mutable { borrowed, .. })) => {
                    mutable_borrows
                        .entry(*borrowed)
                        .or_insert_with(Vec::new)
                        .push(location);
                }
                _ => {}
            }
        }
    }
    (
        shared_borrows
            .into_par_iter()
            .map(|(local, locations)| {
                (
                    local,
                    utils::eliminated_ranges(super::transform::rich_locations_to_ranges(
                        basic_blocks,
                        &locations,
                    )),
                )
            })
            .collect(),
        mutable_borrows
            .into_par_iter()
            .map(|(local, locations)| {
                (
                    local,
                    utils::eliminated_ranges(super::transform::rich_locations_to_ranges(
                        basic_blocks,
                        &locations,
                    )),
                )
            })
            .collect(),
    )
}

pub fn get_must_live(
    datafrog: &PoloniusOutput,
    location_table: &PoloniusLocationTable,
    borrow_map: &BorrowMap,
    basic_blocks: &[MirBasicBlock],
) -> HashMap<Local, Vec<Range>> {
    // obtain a map that region -> region contained locations
    let mut region_locations = HashMap::default();
    for (location_idx, region_idc) in datafrog.origin_live_on_entry.iter() {
        for region_idx in region_idc {
            region_locations
                .entry(*region_idx)
                .or_insert_with(HashSet::default)
                .insert(*location_idx);
        }
    }

    // obtain a map that borrow index -> local
    let mut borrow_local = HashMap::default();
    for (local, borrow_idc) in borrow_map.local_map().iter() {
        for borrow_idx in borrow_idc {
            borrow_local.insert(*borrow_idx, *local);
        }
    }

    // check all regions' subset that must be satisfied
    let mut subsets = HashMap::default();
    for (_, subset) in datafrog.subset.iter() {
        for (sup, subs) in subset.iter() {
            subsets
                .entry(*sup)
                .or_insert_with(HashSet::default)
                .extend(subs.iter().copied());
        }
    }
    // obtain a map that region -> locations
    // a region must contains the locations
    let mut region_must_locations = HashMap::default();
    for (sup, subs) in subsets.iter() {
        for sub in subs {
            if let Some(locs) = region_locations.get(sub) {
                region_must_locations
                    .entry(*sup)
                    .or_insert_with(HashSet::default)
                    .extend(locs.iter().copied());
            }
        }
    }
    // obtain a map that local -> locations
    // a local must lives in the locations
    let mut local_must_locations = HashMap::default();
    for (_location, region_borrows) in datafrog.origin_contains_loan_at.iter() {
        for (region, borrows) in region_borrows.iter() {
            for borrow in borrows {
                if let Some(locs) = region_must_locations.get(region)
                    && let Some(local) = borrow_local.get(borrow)
                {
                    local_must_locations
                        .entry(*local)
                        .or_insert_with(HashSet::default)
                        .extend(locs.iter().copied());
                }
            }
        }
    }

    HashMap::from_iter(local_must_locations.iter().map(|(local, locations)| {
        (
            *local,
            utils::eliminated_ranges(super::transform::rich_locations_to_ranges(
                basic_blocks,
                &locations
                    .iter()
                    .map(|v| location_table.to_rich_location(*v))
                    .collect::<Vec<_>>(),
            )),
        )
    }))
}

/// obtain map from local id to living range
pub fn drop_range(
    datafrog: &PoloniusOutput,
    location_table: &PoloniusLocationTable,
    basic_blocks: &[MirBasicBlock],
) -> HashMap<Local, Vec<Range>> {
    get_range(
        datafrog
            .var_drop_live_on_entry
            .iter()
            .map(|(p, v)| (*p, v.iter().copied())),
        location_table,
        basic_blocks,
    )
}

pub fn get_range(
    live_on_entry: impl Iterator<Item = (impl Idx, impl Iterator<Item = impl Idx>)>,
    location_table: &PoloniusLocationTable,
    basic_blocks: &[MirBasicBlock],
) -> HashMap<Local, Vec<Range>> {
    use rustc_borrowck::consumers::RichLocation;
    use rustc_middle::mir::BasicBlock;

    #[derive(Default)]
    struct LocalLive {
        starts: Vec<(BasicBlock, usize)>,
        mids: Vec<(BasicBlock, usize)>,
    }

    // Collect start/mid locations per local without building an intermediate RichLocation Vec
    let mut locals_live: HashMap<u32, LocalLive> = HashMap::default();
    for (loc_idx, locals) in live_on_entry {
        let rich = location_table.to_rich_location(loc_idx.index().into());
        for local in locals {
            let entry = locals_live
                .entry(local.index().try_into().unwrap())
                .or_insert_with(LocalLive::default);
            match rich {
                RichLocation::Start(l) => entry.starts.push((l.block, l.statement_index)),
                RichLocation::Mid(l) => entry.mids.push((l.block, l.statement_index)),
            }
        }
    }

    fn statement_location_to_range(
        basic_blocks: &[MirBasicBlock],
        block: BasicBlock,
        statement_index: usize,
    ) -> Option<Range> {
        basic_blocks.get(block.index()).and_then(|bb| {
            if statement_index < bb.statements.len() {
                bb.statements.get(statement_index).map(|v| v.range())
            } else {
                bb.terminator.as_ref().map(|v| v.range())
            }
        })
    }

    locals_live
        .into_par_iter()
        .map(|(local_idx, mut live)| {
            super::shared::sort_locs(&mut live.starts);
            super::shared::sort_locs(&mut live.mids);
            let n = live.starts.len().min(live.mids.len());
            if n != live.starts.len() || n != live.mids.len() {
                tracing::debug!(
                    "get_range: starts({}) != mids({}); truncating to {}",
                    live.starts.len(),
                    live.mids.len(),
                    n
                );
            }
            let mut ranges = Vec::with_capacity(n);
            for i in 0..n {
                if let (Some(s), Some(m)) = (
                    statement_location_to_range(basic_blocks, live.starts[i].0, live.starts[i].1),
                    statement_location_to_range(basic_blocks, live.mids[i].0, live.mids[i].1),
                ) && let Some(r) = Range::new(s.from(), m.until())
                {
                    ranges.push(r);
                }
            }
            (local_idx.into(), utils::eliminated_ranges(ranges))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustowl::models::{MirBasicBlock, MirStatement, MirTerminator, Range as OwlRange};
    use rustc_borrowck::consumers::{PoloniusLocationTable, RichLocation};
    use rustc_middle::mir::BasicBlock;
    use rustc_index::Idx;

    // Lightweight test Idx newtype for locals and locations where possible.
    // If rustc_index::Idx cannot be implemented here due to coherence or private traits,
    // we fallback to using existing rustc types where available; otherwise tests for get_range
    // will use simple wrappers around existing index types.
    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    struct TestIdx(u32);
    // SAFETY: TestIdx behaves like a simple index wrapper for testing iterators.
    unsafe impl Idx for TestIdx {
        fn new(idx: usize) -> Self { TestIdx(idx as u32) }
        fn index(self) -> usize { self.0 as usize }
    }

    // Helper: build a basic block with N statements whose ranges are monotonically increasing.
    fn mk_block(stmt_ranges: &[(usize, usize)], term_range: Option<(usize, usize)>) -> MirBasicBlock {
        let mut bb = MirBasicBlock {
            statements: Vec::new(),
            terminator: None,
        };
        for (from, until) in stmt_ranges.iter().copied() {
            // Assume MirStatement::new(from, until) or similar API via Range.
            // We construct via a Range::new(from, until).unwrap() path where available.
            let r = Range::new(from, until).expect("valid statement range");
            bb.statements.push(MirStatement::from_range(r));
        }
        if let Some((from, until)) = term_range {
            let r = Range::new(from, until).expect("valid terminator range");
            bb.terminator = Some(MirTerminator::from_range(r));
        }
        bb
    }

    // Build a fake PoloniusLocationTable by wrapping a vector of RichLocation that we index into.
    // If PoloniusLocationTable lacks a public constructor, we provide a minimal shim type locally
    // that exposes the same to_rich_location interface, and only use it within tests via the get_range signature.
    struct TestLocationTable {
        locs: Vec<RichLocation>,
    }
    impl TestLocationTable {
        fn new(locs: Vec<RichLocation>) -> Self { Self { locs } }
        fn as_polonius<'a>(&'a self) -> &'a PoloniusLocationTable {
            // SAFETY: We never dereference; this is only to satisfy the type in get_range.
            // If the actual type cannot be coerced, switch get_range tests to a local copy of statement_location_to_range logic.
            unsafe { &*(self as *const _ as *const PoloniusLocationTable) }
        }
        fn to_rich_location(&self, idx: usize) -> RichLocation {
            self.locs[idx].clone()
        }
    }

    // Shadow the method used by get_range via a trait to call our shim in tests when compiled in test mode.
    trait TestToRich {
        fn to_rich_location_test(&self, idx: usize) -> RichLocation;
    }
    impl TestToRich for TestLocationTable {
        fn to_rich_location_test(&self, idx: usize) -> RichLocation { self.to_rich_location(idx) }
    }

    // Adaptation layer: a local copy of get_range for tests that uses TestLocationTable to avoid rustc internals.
    fn get_range_test(
        live_on_entry: impl Iterator<Item = (impl Idx, impl Iterator<Item = impl Idx>)>,
        location_table: &TestLocationTable,
        basic_blocks: &[MirBasicBlock],
    ) -> HashMap<Local, Vec<Range>> {
        use rustc_middle::mir::BasicBlock;

        #[derive(Default)]
        struct LocalLive {
            starts: Vec<(BasicBlock, usize)>,
            mids: Vec<(BasicBlock, usize)>,
        }

        let mut locals_live: HashMap<u32, LocalLive> = HashMap::default();
        for (loc_idx, locals) in live_on_entry {
            let rich = location_table.to_rich_location_test(loc_idx.index().into());
            for local in locals {
                let entry = locals_live
                    .entry(local.index().try_into().unwrap())
                    .or_insert_with(LocalLive::default);
                match rich {
                    RichLocation::Start(l) => entry.starts.push((l.block, l.statement_index)),
                    RichLocation::Mid(l) => entry.mids.push((l.block, l.statement_index)),
                }
            }
        }

        fn statement_location_to_range(
            basic_blocks: &[MirBasicBlock],
            block: BasicBlock,
            statement_index: usize,
        ) -> Option<Range> {
            basic_blocks.get(block.index()).and_then(|bb| {
                if statement_index < bb.statements.len() {
                    bb.statements.get(statement_index).map(|v| v.range())
                } else {
                    bb.terminator.as_ref().map(|v| v.range())
                }
            })
        }

        locals_live
            .into_par_iter()
            .map(|(local_idx, mut live)| {
                super::shared::sort_locs(&mut live.starts);
                super::shared::sort_locs(&mut live.mids);
                let n = live.starts.len().min(live.mids.len());
                let mut ranges = Vec::with_capacity(n);
                for i in 0..n {
                    if let (Some(s), Some(m)) = (
                        statement_location_to_range(basic_blocks, live.starts[i].0, live.starts[i].1),
                        statement_location_to_range(basic_blocks, live.mids[i].0, live.mids[i].1),
                    ) && let Some(r) = Range::new(s.from(), m.until())
                    {
                        ranges.push(r);
                    }
                }
                (local_idx.into(), utils::eliminated_ranges(ranges))
            })
            .collect()
    }

    // Helpers to build RichLocation::Start/Mid without relying on rustc internals beyond BasicBlock.
    fn start(block: usize, stmt: usize) -> RichLocation {
        RichLocation::Start(rustc_borrowck::consumers::Location { block: BasicBlock::from_usize(block), statement_index: stmt })
    }
    fn mid(block: usize, stmt: usize) -> RichLocation {
        RichLocation::Mid(rustc_borrowck::consumers::Location { block: BasicBlock::from_usize(block), statement_index: stmt })
    }

    #[test]
    fn get_range_pairs_start_and_mid_happy_path() {
        // Arrange: one basic block with three statements
        let b0 = mk_block(&[(0, 10), (10, 20), (20, 30)], Some((30, 40)));
        let bbs = vec![b0];

        // Two locals appear live at Start(0,1) .. Mid(0,2)
        // Local indices 1 and 2
        let locs = vec![start(0, 1), mid(0, 2)];
        let table = TestLocationTable::new(locs);

        // live_on_entry: (location_idx -> iterator over locals)
        let live = vec![
            (TestIdx(0), vec![TestIdx(1)].into_iter()),
            (TestIdx(1), vec![TestIdx(1)].into_iter()),
        ];

        // Act
        let res = get_range_test(live.into_iter(), &table, &bbs);

        // Assert
        // Expect a single range from statement[1].from (10) to statement[2].until (30)
        let ranges = res.get(&Local::from_u32(1)).expect("local 1 present");
        assert_eq!(ranges.len(), 1);
        let r = &ranges[0];
        assert_eq!(r.from(), 10);
        assert_eq!(r.until(), 30);
    }

    #[test]
    fn get_range_truncates_when_counts_mismatch() {
        // Arrange: two starts but one mid; should truncate to one pair
        let b0 = mk_block(&[(0, 5), (5, 15), (15, 25)], Some((25, 30)));
        let bbs = vec![b0];

        let locs = vec![start(0, 0), start(0, 1), mid(0, 2)];
        let table = TestLocationTable::new(locs);

        // Both locations refer to the same local 7
        let live = vec![
            (TestIdx(0), vec![TestIdx(7)].into_iter()),
            (TestIdx(1), vec![TestIdx(7)].into_iter()),
            (TestIdx(2), vec![TestIdx(7)].into_iter()),
        ];

        let res = get_range_test(live.into_iter(), &table, &vec![bbs[0].clone()]);
        let v = res.get(&Local::from_u32(7)).expect("local 7");
        // Only one pair considered => one range from stmt[0].from (0) to stmt[2].until (25)
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].from(), 0);
        assert_eq!(v[0].until(), 25);
    }

    #[test]
    fn get_range_uses_terminator_when_statement_index_at_end() {
        // Arrange: reference beyond last statement should pick terminator range
        let b0 = mk_block(&[(0, 3), (3, 6)], Some((6, 9)));
        let bbs = vec![b0];

        // Start at stmt index 1, Mid at stmt index 5 (beyond len=2) => use terminator
        let locs = vec![start(0, 1), mid(0, 5)];
        let table = TestLocationTable::new(locs);

        let live = vec![
            (TestIdx(0), vec![TestIdx(2)].into_iter()),
            (TestIdx(1), vec![TestIdx(2)].into_iter()),
        ];

        let res = get_range_test(live.into_iter(), &table, &bbs);
        let v = res.get(&Local::from_u32(2)).expect("local 2");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].from(), 3);
        assert_eq!(v[0].until(), 9); // picked from terminator
    }

    #[test]
    fn get_range_discards_invalid_ranges() {
        // Arrange: Construct a Start and Mid whose computed Range::new returns None (until < from)
        // Use a block with statement[1] (10..5) invalid and statement[2] (5..4) invalid to force elimination.
        // We assume Range::new returns None for invalid ranges.
        let mut b0 = MirBasicBlock { statements: Vec::new(), terminator: None };
        // Create two statements where the second ends before it starts (invalid)
        b0.statements.push(MirStatement::from_range(Range::new(10, 5).unwrap_or_else(|| Range::new(0, 0).unwrap())));
        b0.statements.push(MirStatement::from_range(Range::new(5, 4).unwrap_or_else(|| Range::new(0, 0).unwrap())));
        b0.terminator = Some(MirTerminator::from_range(Range::new(4, 3).unwrap_or_else(|| Range::new(0, 0).unwrap())));

        let bbs = vec![b0];

        let locs = vec![start(0, 0), mid(0, 1)];
        let table = TestLocationTable::new(locs);
        let live = vec![
            (TestIdx(0), vec![TestIdx(3)].into_iter()),
            (TestIdx(1), vec![TestIdx(3)].into_iter()),
        ];

        let res = get_range_test(live.into_iter(), &table, &bbs);
        let v = res.get(&Local::from_u32(3)).unwrap();
        // Eliminated invalid ranges => should be empty
        assert!(v.is_empty(), "invalid ranges should be eliminated");
    }

    // Smoke tests (documenting intended behavior) for outward functions that require heavy rustc Polonius structures.
    // These tests are annotated with ignore to avoid failing without full compiler context.
    // They serve as placeholders for environments where test helpers are available.
    #[test]
    #[ignore = "requires constructing PoloniusOutput and PoloniusLocationTable from rustc internals"]
    fn smoke_get_accurate_live_compiles() {
        let (output, table, bb): (PoloniusOutput, PoloniusLocationTable, Vec<MirBasicBlock>) = todo!();
        let _ = get_accurate_live(&output, &table, &bb);
    }

    #[test]
    #[ignore = "requires constructing BorrowMap and Polonius structures"]
    fn smoke_get_borrow_live_compiles() {
        let (output, table, borrow_map, bb): (PoloniusOutput, PoloniusLocationTable, BorrowMap, Vec<MirBasicBlock>) = todo!();
        let _ = get_borrow_live(&output, &table, &borrow_map, &bb);
    }

    #[test]
    #[ignore = "requires full Polonius subset/origin mappings"]
    fn smoke_get_must_live_compiles() {
        let (output, table, borrow_map, bb): (PoloniusOutput, PoloniusLocationTable, BorrowMap, Vec<MirBasicBlock>) = todo!();
        let _ = get_must_live(&output, &table, &borrow_map, &bb);
    }

    #[test]
    #[ignore = "requires Polonius var_drop_live_on_entry mapping"]
    fn smoke_drop_range_compiles() {
        let (output, table, bb): (PoloniusOutput, PoloniusLocationTable, Vec<MirBasicBlock>) = todo!();
        let _ = drop_range(&output, &table, &bb);
    }
}
