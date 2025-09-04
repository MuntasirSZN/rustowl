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

use std::io::IsTerminal;

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

use tracing_subscriber::{EnvFilter, filter::LevelFilter, fmt, prelude::*};

/// Initializes the logging system with colors and a default log level.
///
/// If a global subscriber is already set (e.g. by another binary), this
/// silently returns without re-initializing.
pub fn initialize_logging(level: LevelFilter) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal());

    // Ignore error if already initialized
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init();
}

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
        use crate::error::RustOwlError;
        use crate::models::{FnLocal, Loc, Range};
        use crate::shells::Shell;

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
            /// Increment the visitor's internal count when a function node is visited.
            ///
            /// This method is invoked for each function encountered during MIR traversal.
            /// It does not inspect the function; it only records that a function visit occurred.
            ///
            /// # Examples
            ///
            /// ```no_run
            /// let mut visitor = CountingVisitor { count: 0 };
            /// let func = /* obtain a `Function` reference from the MIR being visited */ unimplemented!();
            /// visitor.visit_func(&func);
            /// assert_eq!(visitor.count, 1);
            /// ```
            fn visit_func(&mut self, _func: &Function) {
                self.count += 1;
            }

            /// Increment the visitor's statement counter by one.
            ///
            /// This is called for each `MirStatement` visited; it tracks how many statements
            /// the visitor has seen by incrementing `self.count`.
            ///
            /// # Examples
            ///
            /// ```
            /// use crate::models::{MirStatement, Range, Loc};
            ///
            /// let mut visitor = CountingVisitor { count: 0 };
            /// let stmt = MirStatement::Other { range: Range::new(Loc(0), Loc(1)).unwrap() };
            /// visitor.visit_stmt(&stmt);
            /// assert_eq!(visitor.count, 1);
            /// ```
            fn visit_stmt(&mut self, _stmt: &MirStatement) {
                self.count += 1;
            }
        }

        let mut visitor = CountingVisitor { count: 0 };
        mir_visit(&function, &mut visitor);

        assert_eq!(visitor.count, 2); // 1 function + 1 statement
    }
}

// Additional unit tests appended to focus on logging initialization and edge cases in public APIs.
#[cfg(test)]
mod logging_and_api_tests {
    // Testing framework note:
    // Using Rust's built-in test framework (cargo test). No external test libs introduced.
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Once;
    use tracing::{debug, error, info, trace, warn};
    use tracing_subscriber::filter::LevelFilter;

    // Ensure we only initialize the global subscriber once in this module to avoid cross-test flakiness.
    static INIT: Once = Once::new();

    fn init_logger_for_tests() {
        INIT.call_once(|| {
            // Avoid environment override during baseline init
            std::env::remove_var("RUST_LOG");
            initialize_logging(LevelFilter::TRACE);
        });
    }

    #[test]
    fn initialize_logging_is_idempotent_and_does_not_panic() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            initialize_logging(LevelFilter::INFO);
            initialize_logging(LevelFilter::DEBUG);
            initialize_logging(LevelFilter::TRACE);
        }));
        assert!(result.is_ok(), "initialize_logging should never panic even when called multiple times");
    }

    #[test]
    fn initialize_logging_gracefully_handles_invalid_rust_log_env() {
        // Even with an invalid RUST_LOG, the function should fall back to provided level and not panic.
        let result = catch_unwind(AssertUnwindSafe(|| {
            std::env::set_var("RUST_LOG", "!!!!invalid-filter!!!!");
            initialize_logging(LevelFilter::WARN);
            // Clean up to avoid leaking state into other tests
            std::env::remove_var("RUST_LOG");
        }));
        assert!(result.is_ok(), "initialize_logging should not panic with invalid RUST_LOG");
    }

    #[test]
    fn logging_macros_across_levels_do_not_panic_after_init() {
        init_logger_for_tests();
        let res = catch_unwind(AssertUnwindSafe(|| {
            trace!("trace message");
            debug!("debug message");
            info!("info message");
            warn!("warn message");
            error!("error message");
        }));
        assert!(res.is_ok(), "emitting logs at various levels should not panic");
    }

    // Additional API coverage focusing on pure/closed-form functions and edge cases.

    #[test]
    fn common_range_returns_none_for_disjoint_inputs() {
        use crate::models::{Loc, Range};
        use crate::utils::common_range;

        let r1 = Range::new(Loc(0), Loc(5)).expect("valid range");
        let r2 = Range::new(Loc(6), Loc(10)).expect("valid range");
        assert!(common_range(r1, r2).is_none(), "disjoint ranges should have no common intersection");
    }

    #[test]
    fn range_new_rejects_inverted_bounds() {
        use crate::models::{Loc, Range};
        // When start > end, construction should fail.
        let res = Range::new(Loc(10), Loc(0));
        assert!(res.is_err(), "Range::new should return Err for start > end");
    }

    #[test]
    fn mir_visitor_counts_multiple_statements() {
        use crate::models::*;
        use crate::utils::mir_visit;

        let mut function = Function::new(7);

        // Basic block 1 with one statement
        let mut bb1 = MirBasicBlock::new();
        bb1.statements.push(MirStatement::Other {
            range: Range::new(Loc(0), Loc(2)).expect("valid range"),
        });
        function.basic_blocks.push(bb1);

        // Basic block 2 with one statement
        let mut bb2 = MirBasicBlock::new();
        bb2.statements.push(MirStatement::Other {
            range: Range::new(Loc(3), Loc(4)).expect("valid range"),
        });
        function.basic_blocks.push(bb2);

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

        // Expect 1 function + 2 statements = 3 visits
        assert_eq!(visitor.count, 3, "visitor should count one function and two statements");
    }

    #[test]
    fn backend_type_is_publicly_exposed_and_named_sensibly() {
        // Validate the public re-export remains intact
        let ty = std::any::type_name::<Backend>();
        assert!(ty.contains("Backend"), "public type name should contain 'Backend'");
    }
}
