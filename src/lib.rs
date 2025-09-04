//! # RustOwl Library
//!
//! RustOwl is a Language Server Protocol (LSP) implementation for visualizing
//! ownership and lifetimes in Rust code. This library provides the core
//! functionality for analyzing Rust programs and extracting ownership information.
//!
//! ## Core Components
//!
//! - **LSP Backend**: Language server implementation for IDE integration
//! - **Analysis Engine**: Rust compiler integration for ownership analysis  
//! - **Caching System**: Intelligent caching for improved performance
//! - **Error Handling**: Comprehensive error reporting with context
//! - **Toolchain Management**: Automatic setup and management of analysis tools
//!
//! ## Usage
//!
//! This library is primarily used by the RustOwl binary for LSP server functionality,
//! but can also be used directly for programmatic analysis of Rust code.

/// Core caching functionality for analysis results
pub mod cache;
/// Command-line interface definitions
pub mod cli;
/// Comprehensive error handling with context
pub mod error;
/// Language Server Protocol implementation
pub mod lsp;
/// Data models for analysis results
pub mod models;
/// Shell completion utilities
pub mod shells;
/// Rust toolchain management
pub mod toolchain;
/// General utility functions
pub mod utils;

pub use lsp::backend::Backend;

// Miri-specific memory safety tests
#[cfg(test)]
mod miri_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure() {
        // Test that all modules are accessible and key types can be imported
        use crate::cache::CacheConfig;
        use crate::models::{FnLocal, Loc, Range};
        use crate::shells::Shell;
        use crate::error::RustOwlError;
        
        // Test basic construction of key types
        let _config = CacheConfig::default();
        let _fn_local = FnLocal::new(1, 2);
        let _loc = Loc(10);
        let _range = Range::new(Loc(0), Loc(5));
        let _shell = Shell::Bash;
        
        // Test error types
        let _error = RustOwlError::Cache("test error".to_string());
        
        // Verify Backend type is available
        let _backend_type = std::any::type_name::<Backend>();
    }

    #[test]
    fn test_public_api() {
        // Test that the public API exports work correctly
        
        // Backend should be available from root
        let backend_name = std::any::type_name::<Backend>();
        assert!(backend_name.contains("Backend"));
        
        // Test that modules contain expected items
        use crate::models::*;
        use crate::utils::*;
        
        // Test utils functions
        let range1 = Range::new(Loc(0), Loc(10)).unwrap();
        let range2 = Range::new(Loc(5), Loc(15)).unwrap();
        
        assert!(common_range(range1, range2).is_some());
        
        // Test models
        let mut variables = MirVariables::new();
        let var = MirVariable::User {
            index: 1,
            live: range1,
            dead: range2,
        };
        variables.push(var);
        
        let vec = variables.to_vec();
        assert_eq!(vec.len(), 1);
    }

    #[test]
    fn test_type_compatibility() {
        // Test that types work together as expected in the public API
        use crate::models::*;
        use crate::utils::*;
        
        // Create a function with basic blocks
        let mut function = Function::new(42);
        
        // Add a basic block
        let mut bb = MirBasicBlock::new();
        bb.statements.push(MirStatement::Other {
            range: Range::new(Loc(0), Loc(5)).unwrap(),
        });
        function.basic_blocks.push(bb);
        
        // Test visitor pattern
        struct CountingVisitor {
            count: usize,
        }
        
        impl MirVisitor for CountingVisitor {
            fn visit_func(&mut self, _func: &Function) {
                self.count += 1;
            }
            
            fn visit_stmt(&mut self, _stmt: &MirStatement) {
                self.count += 1;
            }
        }
        
        let mut visitor = CountingVisitor { count: 0 };
        mir_visit(&function, &mut visitor);
        
        assert_eq!(visitor.count, 2); // 1 function + 1 statement
    }
}
