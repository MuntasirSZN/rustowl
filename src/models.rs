//! Data models for RustOwl ownership and lifetime analysis.
//!
//! This module contains the core data structures used to represent
//! ownership information, lifetimes, and analysis results extracted
//! from Rust code via compiler integration.

use foldhash::quality::RandomState as FoldHasher;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// An IndexMap with FoldHasher for fast + high-quality hashing.
pub type FoldIndexMap<K, V> = IndexMap<K, V, FoldHasher>;

/// An IndexSet with FoldHasher for fast + high-quality hashing.
pub type FoldIndexSet<K> = IndexSet<K, FoldHasher>;

/// Represents a local variable within a function scope.
///
/// This structure uniquely identifies a local variable by combining
/// its local ID within the function and the function ID itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FnLocal {
    /// Local variable ID within the function
    pub id: u32,
    /// Function ID this local belongs to
    pub fn_id: u32,
}

impl FnLocal {
    /// Creates a new function-local variable identifier.
    ///
    /// # Arguments
    /// * `id` - The local variable ID within the function
    /// * `fn_id` - The function ID this local belongs to
    pub fn new(id: u32, fn_id: u32) -> Self {
        Self { id, fn_id }
    }
}

/// Represents a character position in source code.
///
/// This is a character-based position that handles Unicode correctly
/// and automatically filters out carriage return characters to match
/// compiler behavior.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(transparent)]
pub struct Loc(pub u32);

impl Loc {
    /// Creates a new location from source text and byte position.
    ///
    /// Converts a byte position to a character position, handling Unicode
    /// correctly and filtering out CR characters as the compiler does.
    ///
    /// # Arguments
    /// * `source` - The source code text
    /// * `byte_pos` - Byte position in the source
    /// * `offset` - Offset to subtract from byte position
    pub fn new(source: &str, byte_pos: u32, offset: u32) -> Self {
        let byte_pos = byte_pos.saturating_sub(offset);
        let byte_pos = byte_pos as usize;

        // Convert byte position to character position efficiently
        // Skip CR characters without allocating a new string
        let mut char_count = 0u32;
        let mut byte_count = 0usize;

        for ch in source.chars() {
            if byte_count >= byte_pos {
                break;
            }

            // Skip CR characters (compiler ignores them)
            if ch != '\r' {
                byte_count += ch.len_utf8();
                if byte_count <= byte_pos {
                    char_count += 1;
                }
            } else {
                byte_count += ch.len_utf8();
            }
        }

        Self(char_count)
    }
}

impl std::ops::Add<i32> for Loc {
    type Output = Loc;
    /// Adds a signed offset to this `Loc`, saturating to avoid underflow or overflow.
    ///
    /// For non-negative offsets, the location is increased with saturation at `u32::MAX`.
    /// For negative offsets, the absolute value is subtracted with saturation at `0`.
    ///
    /// # Examples
    ///
    /// ```
    /// let a = Loc(5);
    /// assert_eq!(a + 3, Loc(8));
    ///
    /// let b = Loc(0);
    /// assert_eq!(b + -10, Loc(0)); // saturates at zero, does not underflow
    ///
    /// let c = Loc(u32::MAX - 1);
    /// assert_eq!(c + 10, Loc(u32::MAX)); // saturates at u32::MAX, does not overflow
    /// ```
    fn add(self, rhs: i32) -> Self::Output {
        if rhs >= 0 {
            // Use saturating_add to prevent overflow
            Loc(self.0.saturating_add(rhs as u32))
        } else {
            // rhs is negative, so subtract the absolute value
            let abs_rhs = (-rhs) as u32;
            Loc(self.0.saturating_sub(abs_rhs))
        }
    }
}

impl std::ops::Sub<i32> for Loc {
    type Output = Loc;
    /// Subtracts a signed offset from this `Loc`, using saturating arithmetic.
    ///
    /// For non-negative `rhs` the function subtracts `rhs` (saturating at 0 to prevent underflow).
    /// If `rhs` is negative the absolute value is added (saturating on overflow).
    ///
    /// # Examples
    ///
    /// ```
    /// # use crate::Loc;
    /// let a = Loc(10);
    /// assert_eq!(a.sub(3), Loc(7));   // normal subtraction
    /// assert_eq!(a.sub(-2), Loc(12)); // negative rhs -> addition
    /// let zero = Loc(0);
    /// assert_eq!(zero.sub(1), Loc(0)); // saturates at 0, no underflow
    /// let max = Loc(u32::MAX);
    /// assert_eq!(max.sub(-1), Loc(u32::MAX)); // saturating add prevents overflow
    /// ```
    fn sub(self, rhs: i32) -> Self::Output {
        if rhs >= 0 {
            Loc(self.0.saturating_sub(rhs as u32))
        } else {
            // rhs is negative, so we're actually adding the absolute value
            let abs_rhs = (-rhs) as u32;
            Loc(self.0.saturating_add(abs_rhs))
        }
    }
}

impl From<u32> for Loc {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Loc> for u32 {
    fn from(value: Loc) -> Self {
        value.0
    }
}

/// Represents a character range in source code.
///
/// A range is defined by a starting and ending location, where the
/// ending location is exclusive (half-open interval).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Range {
    from: Loc,
    until: Loc,
}

impl Range {
    /// Creates a new range if the end position is after the start position.
    ///
    /// # Arguments
    /// * `from` - Starting location (inclusive)
    /// * `until` - Ending location (exclusive)
    ///
    /// # Returns
    /// `Some(Range)` if valid, `None` if `until <= from`
    pub fn new(from: Loc, until: Loc) -> Option<Self> {
        if until.0 <= from.0 {
            None
        } else {
            Some(Self { from, until })
        }
    }

    /// Returns the starting location of the range.
    pub fn from(&self) -> Loc {
        self.from
    }

    /// Returns the ending location of the range.
    pub fn until(&self) -> Loc {
        self.until
    }

    /// Returns the size of the range in characters.
    pub fn size(&self) -> u32 {
        self.until.0 - self.from.0
    }
}

/// Represents a MIR (Mid-level IR) variable with lifetime information.
///
/// MIR variables can be either user-defined variables or compiler-generated
/// temporaries, each with their own live and dead ranges.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MirVariable {
    /// A user-defined variable
    User {
        /// Variable index within the function
        index: u32,
        /// Range where the variable is live
        live: Range,
        /// Range where the variable is dead/dropped
        dead: Range,
    },
    /// A compiler-generated temporary or other variable
    Other {
        index: u32,
        live: Range,
        dead: Range,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(transparent)]
pub struct MirVariables(IndexMap<u32, MirVariable>);

impl Default for MirVariables {
    fn default() -> Self {
        Self::new()
    }
}

impl MirVariables {
    pub fn new() -> Self {
        Self(IndexMap::with_capacity(8))
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(IndexMap::with_capacity(capacity))
    }

    pub fn push(&mut self, var: MirVariable) {
        let index = match &var {
            MirVariable::User { index, .. } | MirVariable::Other { index, .. } => *index,
        };
        self.0.entry(index).or_insert(var);
    }

    pub fn to_vec(self) -> Vec<MirVariable> {
        self.0.into_values().collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct File {
    pub items: SmallVec<[Function; 4]>, // Most files have few functions
}

impl Default for File {
    fn default() -> Self {
        Self::new()
    }
}

impl File {
    pub fn new() -> Self {
        Self {
            items: SmallVec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: SmallVec::with_capacity(capacity),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(transparent)]
pub struct Workspace(pub FoldIndexMap<String, Crate>);

impl Workspace {
    pub fn merge(&mut self, other: Self) {
        let Workspace(crates) = other;
        for (name, krate) in crates {
            if let Some(insert) = self.0.get_mut(&name) {
                insert.merge(krate);
            } else {
                self.0.insert(name, krate);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(transparent)]
pub struct Crate(pub FoldIndexMap<String, File>);

impl Crate {
    pub fn merge(&mut self, other: Self) {
        let Crate(files) = other;
        for (file, mut mir) in files {
            match self.0.get_mut(&file) {
                Some(existing) => {
                    // Pre-allocate capacity for better performance
                    let new_size = existing.items.len() + mir.items.len();
                    if existing.items.capacity() < new_size {
                        existing
                            .items
                            .reserve_exact(new_size - existing.items.capacity());
                    }

                    let mut seen_ids = FoldIndexSet::with_capacity_and_hasher(
                        existing.items.len(),
                        FoldHasher::default(),
                    );
                    seen_ids.extend(existing.items.iter().map(|i| i.fn_id));

                    mir.items.retain(|item| seen_ids.insert(item.fn_id));
                    existing.items.append(&mut mir.items);
                }
                None => {
                    self.0.insert(file, mir);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MirRval {
    Move {
        target_local: FnLocal,
        range: Range,
    },
    Borrow {
        target_local: FnLocal,
        range: Range,
        mutable: bool,
        outlive: Option<Range>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MirStatement {
    StorageLive {
        target_local: FnLocal,
        range: Range,
    },
    StorageDead {
        target_local: FnLocal,
        range: Range,
    },
    Assign {
        target_local: FnLocal,
        range: Range,
        rval: Option<MirRval>,
    },
    Other {
        range: Range,
    },
}
impl MirStatement {
    pub fn range(&self) -> Range {
        match self {
            Self::StorageLive { range, .. } => *range,
            Self::StorageDead { range, .. } => *range,
            Self::Assign { range, .. } => *range,
            Self::Other { range } => *range,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MirTerminator {
    Drop {
        local: FnLocal,
        range: Range,
    },
    Call {
        destination_local: FnLocal,
        fn_span: Range,
    },
    Other {
        range: Range,
    },
}
impl MirTerminator {
    pub fn range(&self) -> Range {
        match self {
            Self::Drop { range, .. } => *range,
            Self::Call { fn_span, .. } => *fn_span,
            Self::Other { range } => *range,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MirBasicBlock {
    pub statements: StatementVec,
    pub terminator: Option<MirTerminator>,
}

impl Default for MirBasicBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl MirBasicBlock {
    pub fn new() -> Self {
        Self {
            statements: StatementVec::new(),
            terminator: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            statements: StatementVec::with_capacity(capacity),
            terminator: None,
        }
    }
}

// Type aliases for commonly small collections
pub type RangeVec = SmallVec<[Range; 4]>; // Most variables have few ranges
pub type StatementVec = SmallVec<[MirStatement; 8]>; // Most basic blocks have few statements
pub type DeclVec = SmallVec<[MirDecl; 16]>; // Most functions have moderate number of declarations

// Helper functions for conversions since we can't impl traits on type aliases
pub fn range_vec_into_vec(ranges: RangeVec) -> Vec<Range> {
    ranges.into_vec()
}

pub fn range_vec_from_vec(vec: Vec<Range>) -> RangeVec {
    RangeVec::from_vec(vec)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MirDecl {
    User {
        local: FnLocal,
        name: smol_str::SmolStr,
        span: Range,
        ty: smol_str::SmolStr,
        lives: RangeVec,
        shared_borrow: RangeVec,
        mutable_borrow: RangeVec,
        drop: bool,
        drop_range: RangeVec,
        must_live_at: RangeVec,
    },
    Other {
        local: FnLocal,
        ty: smol_str::SmolStr,
        lives: RangeVec,
        shared_borrow: RangeVec,
        mutable_borrow: RangeVec,
        drop: bool,
        drop_range: RangeVec,
        must_live_at: RangeVec,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Function {
    pub fn_id: u32,
    pub basic_blocks: SmallVec<[MirBasicBlock; 8]>, // Most functions have few basic blocks
    pub decls: DeclVec,
}

impl Function {
    pub fn new(fn_id: u32) -> Self {
        Self {
            fn_id,
            basic_blocks: SmallVec::new(),
            decls: DeclVec::new(),
        }
    }

    /// Creates a `Function` with preallocated capacity for basic blocks and declarations.
    ///
    /// `fn_id` is the function identifier. `bb_capacity` is the initial capacity reserved
    /// for the function's basic block list. `decl_capacity` is the initial capacity reserved
    /// for the function's declarations.
    ///
    /// # Examples
    ///
    /// ```
    /// let f = Function::with_capacity(42, 8, 16);
    /// assert_eq!(f.fn_id, 42);
    /// assert!(f.basic_blocks.capacity() >= 8);
    /// assert!(f.decls.capacity() >= 16);
    /// ```
    pub fn with_capacity(fn_id: u32, bb_capacity: usize, decl_capacity: usize) -> Self {
        Self {
            fn_id,
            basic_blocks: SmallVec::with_capacity(bb_capacity),
            decls: DeclVec::with_capacity(decl_capacity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loc_creation_with_unicode() {
        let source = "hello 🦀 world\r\ngoodbye 🌍 world";
        // Test character position conversion
        let _loc = Loc::new(source, 8, 0); // Should point to space before 🦀
        
        // Verify that CR characters are filtered out
        let source_with_cr = "hello\r\n world";
        let loc_with_cr = Loc::new(source_with_cr, 8, 0);
        let loc_without_cr = Loc::new("hello\n world", 7, 0);
        assert_eq!(loc_with_cr.0, loc_without_cr.0);
    }

    #[test]
    fn test_loc_arithmetic_edge_cases() {
        let loc = Loc(10);
        
        // Test overflow protection
        let loc_max = Loc(u32::MAX - 5);
        let result = loc_max + 10;
        assert!(result.0 >= loc_max.0); // Should not wrap around
        
        // Test underflow protection with large subtraction
        let result_sub = loc - 20;
        assert_eq!(result_sub.0, 0); // Should saturate to 0
        
        // Test addition of negative that would underflow
        let result_neg = loc + (-15);
        assert_eq!(result_neg.0, 0); // Should saturate to 0
    }

    #[test]
    fn test_range_validation_comprehensive() {
        // Test edge cases for range creation
        let zero_size = Range::new(Loc(5), Loc(5));
        assert!(zero_size.is_none());
        
        let backwards = Range::new(Loc(10), Loc(5));
        assert!(backwards.is_none());
        
        let valid = Range::new(Loc(5), Loc(10)).unwrap();
        assert_eq!(valid.size(), 5);
        assert_eq!(valid.from().0, 5);
        assert_eq!(valid.until().0, 10);
        
        // Test with maximum values
        let max_range = Range::new(Loc(0), Loc(u32::MAX)).unwrap();
        assert_eq!(max_range.size(), u32::MAX);
    }

    #[test]
    fn test_mir_variable_enum_operations() {
        let user_var = MirVariable::User {
            index: 42,
            live: Range::new(Loc(0), Loc(10)).unwrap(),
            dead: Range::new(Loc(10), Loc(20)).unwrap(),
        };
        
        let other_var = MirVariable::Other {
            index: 24,
            live: Range::new(Loc(5), Loc(15)).unwrap(),
            dead: Range::new(Loc(15), Loc(25)).unwrap(),
        };
        
        // Test pattern matching
        match user_var {
            MirVariable::User { index, .. } => assert_eq!(index, 42),
            _ => panic!("Should be User variant"),
        }
        
        match other_var {
            MirVariable::Other { index, .. } => assert_eq!(index, 24),
            _ => panic!("Should be Other variant"),
        }
        
        // Test equality
        let user_var2 = MirVariable::User {
            index: 42,
            live: Range::new(Loc(0), Loc(10)).unwrap(),
            dead: Range::new(Loc(10), Loc(20)).unwrap(),
        };
        assert_eq!(user_var, user_var2);
        assert_ne!(user_var, other_var);
    }

    #[test]
    fn test_mir_variables_collection_advanced() {
        let mut vars = MirVariables::with_capacity(10);
        assert!(vars.0.capacity() >= 10);
        
        // Test adding duplicates
        let var1 = MirVariable::User {
            index: 1,
            live: Range::new(Loc(0), Loc(10)).unwrap(),
            dead: Range::new(Loc(10), Loc(20)).unwrap(),
        };
        
        let var1_duplicate = MirVariable::User {
            index: 1, // Same index
            live: Range::new(Loc(5), Loc(15)).unwrap(), // Different ranges
            dead: Range::new(Loc(15), Loc(25)).unwrap(),
        };
        
        vars.push(var1);
        vars.push(var1_duplicate); // Should not add due to same index
        
        let result = vars.to_vec();
        assert_eq!(result.len(), 1);
        
        // Verify the first one was kept (or_insert behavior)
        if let MirVariable::User { live, .. } = result[0] {
            assert_eq!(live.from().0, 0);
        }
    }

    #[test]
    fn test_file_operations() {
        let mut file = File::with_capacity(5);
        assert!(file.items.capacity() >= 5);
        
        // Test adding functions
        file.items.push(Function::new(1));
        file.items.push(Function::new(2));
        
        assert_eq!(file.items.len(), 2);
        assert_eq!(file.items[0].fn_id, 1);
        assert_eq!(file.items[1].fn_id, 2);
        
        // Test cloning
        let file_clone = file.clone();
        assert_eq!(file.items.len(), file_clone.items.len());
    }

    #[test]
    fn test_workspace_merge_operations() {
        let mut workspace1 = Workspace(FoldIndexMap::default());
        let mut workspace2 = Workspace(FoldIndexMap::default());
        
        // Setup workspace1 with a crate
        let mut crate1 = Crate(FoldIndexMap::default());
        crate1.0.insert("lib.rs".to_string(), File::new());
        workspace1.0.insert("my_crate".to_string(), crate1);
        
        // Setup workspace2 with the same crate name but different file
        let mut crate2 = Crate(FoldIndexMap::default());
        crate2.0.insert("main.rs".to_string(), File::new());
        workspace2.0.insert("my_crate".to_string(), crate2);
        
        // Setup workspace2 with a different crate
        let crate3 = Crate(FoldIndexMap::default());
        workspace2.0.insert("other_crate".to_string(), crate3);
        
        workspace1.merge(workspace2);
        
        // Should have 2 crates total
        assert_eq!(workspace1.0.len(), 2);
        assert!(workspace1.0.contains_key("my_crate"));
        assert!(workspace1.0.contains_key("other_crate"));
        
        // my_crate should have both files after merge
        let merged_crate = &workspace1.0["my_crate"];
        assert_eq!(merged_crate.0.len(), 2);
        assert!(merged_crate.0.contains_key("lib.rs"));
        assert!(merged_crate.0.contains_key("main.rs"));
    }

    #[test]
    fn test_crate_merge_with_duplicate_functions() {
        let mut crate1 = Crate(FoldIndexMap::default());
        let mut crate2 = Crate(FoldIndexMap::default());
        
        // Create files with functions
        let mut file1 = File::new();
        file1.items.push(Function::new(1));
        file1.items.push(Function::new(2));
        
        let mut file2 = File::new();
        file2.items.push(Function::new(2)); // Duplicate fn_id
        file2.items.push(Function::new(3));
        
        crate1.0.insert("test.rs".to_string(), file1);
        crate2.0.insert("test.rs".to_string(), file2);
        
        crate1.merge(crate2);
        
        let merged_file = &crate1.0["test.rs"];
        // Should have 3 unique functions (1, 2, 3) with duplicate 2 filtered out
        assert_eq!(merged_file.items.len(), 3);
        
        // Check that function IDs are unique
        let mut ids: Vec<u32> = merged_file.items.iter().map(|f| f.fn_id).collect();
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_mir_statement_range_extraction() {
        let range = Range::new(Loc(10), Loc(20)).unwrap();
        let fn_local = FnLocal::new(1, 42);
        
        let storage_live = MirStatement::StorageLive {
            target_local: fn_local,
            range,
        };
        assert_eq!(storage_live.range(), range);
        
        let storage_dead = MirStatement::StorageDead {
            target_local: fn_local,
            range,
        };
        assert_eq!(storage_dead.range(), range);
        
        let assign = MirStatement::Assign {
            target_local: fn_local,
            range,
            rval: None,
        };
        assert_eq!(assign.range(), range);
        
        let other = MirStatement::Other { range };
        assert_eq!(other.range(), range);
    }

    /// Verifies that `MirTerminator::range()` returns the associated `Range` for every variant.
    ///
    /// This test constructs `Drop`, `Call`, and `Other` terminators and asserts that
    /// calling `.range()` yields the same `Range` value provided at construction.
    ///
    /// # Examples
    ///
    /// ```
    /// let range = Range::new(Loc(5), Loc(15)).unwrap();
    /// let fn_local = FnLocal::new(2, 24);
    ///
    /// let drop_term = MirTerminator::Drop { local: fn_local, range };
    /// assert_eq!(drop_term.range(), range);
    ///
    /// let call_term = MirTerminator::Call { destination_local: fn_local, fn_span: range };
    /// assert_eq!(call_term.range(), range);
    ///
    /// let other_term = MirTerminator::Other { range };
    /// assert_eq!(other_term.range(), range);
    /// ```
    #[test]
    fn test_mir_terminator_range_extraction() {
        let range = Range::new(Loc(5), Loc(15)).unwrap();
        let fn_local = FnLocal::new(2, 24);
        
        let drop_term = MirTerminator::Drop {
            local: fn_local,
            range,
        };
        assert_eq!(drop_term.range(), range);
        
        let call_term = MirTerminator::Call {
            destination_local: fn_local,
            fn_span: range,
        };
        assert_eq!(call_term.range(), range);
        
        let other_term = MirTerminator::Other { range };
        assert_eq!(other_term.range(), range);
    }

    #[test]
    fn test_mir_basic_block_operations() {
        let mut bb = MirBasicBlock::with_capacity(5);
        assert!(bb.statements.capacity() >= 5);
        
        // Add statements
        let range = Range::new(Loc(0), Loc(5)).unwrap();
        let fn_local = FnLocal::new(1, 1);
        
        bb.statements.push(MirStatement::StorageLive {
            target_local: fn_local,
            range,
        });
        
        bb.statements.push(MirStatement::Other { range });
        
        // Add terminator
        bb.terminator = Some(MirTerminator::Drop {
            local: fn_local,
            range,
        });
        
        assert_eq!(bb.statements.len(), 2);
        assert!(bb.terminator.is_some());
        
        // Test default creation
        let default_bb = MirBasicBlock::default();
        assert_eq!(default_bb.statements.len(), 0);
        assert!(default_bb.terminator.is_none());
    }

    #[test]
    fn test_function_with_capacity() {
        let func = Function::with_capacity(123, 10, 20);
        assert_eq!(func.fn_id, 123);
        assert!(func.basic_blocks.capacity() >= 10);
        assert!(func.decls.capacity() >= 20);
        
        // Test that new function starts empty
        assert_eq!(func.basic_blocks.len(), 0);
        assert_eq!(func.decls.len(), 0);
    }

    #[test]
    fn test_range_vec_conversions() {
        let ranges = vec![
            Range::new(Loc(0), Loc(5)).unwrap(),
            Range::new(Loc(10), Loc(15)).unwrap(),
        ];
        
        let range_vec = range_vec_from_vec(ranges.clone());
        let converted_back = range_vec_into_vec(range_vec);
        
        assert_eq!(ranges, converted_back);
    }

    #[test]
    fn test_fn_local_hash_consistency() {
        use std::collections::HashMap;
        
        let fn_local1 = FnLocal::new(1, 2);
        let fn_local2 = FnLocal::new(1, 2);
        let fn_local3 = FnLocal::new(2, 1);
        
        let mut map = HashMap::new();
        map.insert(fn_local1, "value1");
        map.insert(fn_local3, "value2");
        
        // Same values should hash to same key
        assert_eq!(map.get(&fn_local2), Some(&"value1"));
        assert_eq!(map.get(&fn_local3), Some(&"value2"));
        assert_eq!(map.len(), 2);
    }
}
