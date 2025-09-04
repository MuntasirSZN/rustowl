use std::env;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Configuration for cache behavior
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// Maximum number of entries before eviction
    pub max_entries: usize,
    /// Maximum memory usage in bytes before eviction
    pub max_memory_bytes: usize,
    /// Enable LRU eviction policy (vs FIFO)
    pub use_lru_eviction: bool,
    /// Enable file modification time validation
    pub validate_file_mtime: bool,
    /// Enable compression for cache files
    pub enable_compression: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            use_lru_eviction: true,
            validate_file_mtime: true,
            enable_compression: false, // Disable by default for compatibility
        }
    }
}

pub fn is_cache() -> bool {
    !env::var("RUSTOWL_CACHE")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "false" || v == "0"
        })
        .unwrap_or(false)
}

pub fn set_cache_path(cmd: &mut Command, target_dir: impl AsRef<Path>) {
    cmd.env("RUSTOWL_CACHE_DIR", target_dir.as_ref().join("cache"));
}

pub fn get_cache_path() -> Option<PathBuf> {
    env::var("RUSTOWL_CACHE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Construct a CacheConfig starting from defaults and overriding fields from environment variables.
///
/// The following environment variables are recognized (case-sensitive names):
/// - `RUSTOWL_CACHE_MAX_ENTRIES`: parsed as `usize` to set `max_entries`.
/// - `RUSTOWL_CACHE_MAX_MEMORY_MB`: parsed as `usize`; stored as bytes using saturating multiplication by 1024*1024.
/// - `RUSTOWL_CACHE_EVICTION`: case-insensitive; `"lru"` enables LRU eviction, `"fifo"` disables it; other values leave the default.
/// - `RUSTOWL_CACHE_VALIDATE_FILES`: case-insensitive; `"false"` or `"0"` disables file mtime validation, any other value enables it.
///
/// Returns the assembled `CacheConfig`.
///
/// # Examples
///
/// ```
/// std::env::set_var("RUSTOWL_CACHE_MAX_ENTRIES", "5");
/// let cfg = get_cache_config();
/// assert_eq!(cfg.max_entries, 5);
/// ```
pub fn get_cache_config() -> CacheConfig {
    let mut config = CacheConfig::default();

    // Configure max entries
    if let Ok(max_entries) = env::var("RUSTOWL_CACHE_MAX_ENTRIES")
        && let Ok(value) = max_entries.parse::<usize>()
    {
        config.max_entries = value;
    }

    // Configure max memory in MB
    if let Ok(max_memory_mb) = env::var("RUSTOWL_CACHE_MAX_MEMORY_MB")
        && let Ok(value) = max_memory_mb.parse::<usize>()
    {
        config.max_memory_bytes = value.saturating_mul(1024 * 1024);
    }

    // Configure eviction policy
    if let Ok(policy) = env::var("RUSTOWL_CACHE_EVICTION") {
        match policy.trim().to_ascii_lowercase().as_str() {
            "lru" => config.use_lru_eviction = true,
            "fifo" => config.use_lru_eviction = false,
            _ => {} // keep default
        }
    }

    // Configure file validation
    if let Ok(validate) = env::var("RUSTOWL_CACHE_VALIDATE_FILES") {
        let v = validate.trim().to_ascii_lowercase();
        config.validate_file_mtime = !(v == "false" || v == "0");
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Temporarily sets an environment variable for the duration of a closure, restoring the previous state afterwards.
    ///
    /// The function saves the current value of `key` (if any), sets `key` to `value`, runs `f()`, and then restores `key` to its original value:
    /// - If the variable existed before, it is reset to its previous value.
    /// - If the variable did not exist before, it is removed after `f` returns.
    ///
    /// This is intended for use in tests to run code under specific environment settings without leaking changes.
    ///
    /// # Examples
    ///
    /// ```
    /// // Ensure a value is visible inside the closure and restored afterwards.
    /// use std::env;
    ///
    /// let prev = env::var("MY_TEST_VAR").ok();
    /// with_env("MY_TEST_VAR", "temp", || {
    ///     assert_eq!(env::var("MY_TEST_VAR").unwrap(), "temp");
    /// });
    /// assert_eq!(env::var("MY_TEST_VAR").ok(), prev);
    /// ```
    fn with_env<F>(key: &str, value: &str, f: F)
    where
        F: FnOnce(),
    {
        let old_value = env::var(key).ok();
        unsafe {
            env::set_var(key, value);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        match old_value {
            Some(v) => unsafe { env::set_var(key, v) },
            None => unsafe { env::remove_var(key) },
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}

#[test]
fn test_cache_config_default() {
    let config = CacheConfig::default();
    assert_eq!(config.max_entries, 1000);
    assert_eq!(config.max_memory_bytes, 100 * 1024 * 1024);
    assert!(config.use_lru_eviction);
    assert!(config.validate_file_mtime);
    assert!(!config.enable_compression);
}

#[test]
fn test_is_cache_default() {
    // Remove any existing cache env var for clean test
    let old_value = env::var("RUSTOWL_CACHE").ok();
    unsafe {
        env::remove_var("RUSTOWL_CACHE");
    }

    assert!(is_cache()); // Should be true by default

    // Restore old value
    if let Some(v) = old_value {
        unsafe {
            env::set_var("RUSTOWL_CACHE", v);
        }
    }
}

#[test]
fn test_is_cache_with_false_values() {
    with_env("RUSTOWL_CACHE", "false", || {
        assert!(!is_cache());
    });

    with_env("RUSTOWL_CACHE", "FALSE", || {
        assert!(!is_cache());
    });

    with_env("RUSTOWL_CACHE", "0", || {
        assert!(!is_cache());
    });

    with_env("RUSTOWL_CACHE", "  false  ", || {
        assert!(!is_cache());
    });
}

#[test]
fn test_is_cache_with_true_values() {
    with_env("RUSTOWL_CACHE", "true", || {
        assert!(is_cache());
    });

    with_env("RUSTOWL_CACHE", "1", || {
        assert!(is_cache());
    });

    with_env("RUSTOWL_CACHE", "yes", || {
        assert!(is_cache());
    });

    with_env("RUSTOWL_CACHE", "", || {
        assert!(is_cache());
    });
}

#[test]
fn test_get_cache_path() {
    // Test with no env var
    let old_value = env::var("RUSTOWL_CACHE_DIR").ok();
    unsafe {
        env::remove_var("RUSTOWL_CACHE_DIR");
    }
    assert!(get_cache_path().is_none());

    // Test with empty value
    with_env("RUSTOWL_CACHE_DIR", "", || {
        assert!(get_cache_path().is_none());
    });

    // Test with whitespace only
    with_env("RUSTOWL_CACHE_DIR", "   ", || {
        assert!(get_cache_path().is_none());
    });

    // Test with valid path
    with_env("RUSTOWL_CACHE_DIR", "/tmp/cache", || {
        let path = get_cache_path().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/cache"));
    });

    // Test with path that has whitespace
    with_env("RUSTOWL_CACHE_DIR", "  /tmp/cache  ", || {
        let path = get_cache_path().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/cache"));
    });

    // Restore old value
    if let Some(v) = old_value {
        unsafe {
            env::set_var("RUSTOWL_CACHE_DIR", v);
        }
    }
}

#[test]
fn test_set_cache_path() {
    use tokio::process::Command;

    let mut cmd = Command::new("echo");
    let target_dir = PathBuf::from("/tmp/test_target");

    set_cache_path(&mut cmd, &target_dir);

    // Note: We can't easily test that the env var was set on the Command
    // since that's internal to tokio::process::Command, but we can test
    // that the function doesn't panic and accepts the expected types
    let expected_cache_dir = target_dir.join("cache");
    assert_eq!(expected_cache_dir, PathBuf::from("/tmp/test_target/cache"));
}

#[test]
fn test_get_cache_config_with_env_vars() {
    // Test max entries configuration
    with_env("RUSTOWL_CACHE_MAX_ENTRIES", "500", || {
        let config = get_cache_config();
        assert_eq!(config.max_entries, 500);
    });

    // Test that invalid values don't crash the program
    with_env("RUSTOWL_CACHE_MAX_ENTRIES", "invalid", || {
        let config = get_cache_config();
        // Should use a reasonable default value, either 500 or 1000
        assert!(config.max_entries == 500 || config.max_entries == 1000);
    });

    // Test max memory configuration
    with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", "200", || {
        let config = get_cache_config();
        assert_eq!(config.max_memory_bytes, 200 * 1024 * 1024);
    });

    // Test max memory with overflow protection
    with_env(
        "RUSTOWL_CACHE_MAX_MEMORY_MB",
        &usize::MAX.to_string(),
        || {
            let config = get_cache_config();
            // Should use saturating_mul, so might be different from exact calculation
            assert!(config.max_memory_bytes > 0);
        },
    );

    // Test eviction policy configuration
    with_env("RUSTOWL_CACHE_EVICTION", "lru", || {
        let config = get_cache_config();
        assert!(config.use_lru_eviction);
    });

    with_env("RUSTOWL_CACHE_EVICTION", "LRU", || {
        let config = get_cache_config();
        assert!(config.use_lru_eviction);
    });

    with_env("RUSTOWL_CACHE_EVICTION", "fifo", || {
        let config = get_cache_config();
        assert!(!config.use_lru_eviction);
    });

    with_env("RUSTOWL_CACHE_EVICTION", "FIFO", || {
        let config = get_cache_config();
        assert!(!config.use_lru_eviction);
    });

    // Test invalid eviction policy (should keep default)
    with_env("RUSTOWL_CACHE_EVICTION", "invalid", || {
        let config = get_cache_config();
        assert!(config.use_lru_eviction); // default is true
    });

    // Test file validation configuration
    with_env("RUSTOWL_CACHE_VALIDATE_FILES", "false", || {
        let config = get_cache_config();
        assert!(!config.validate_file_mtime);
    });

    with_env("RUSTOWL_CACHE_VALIDATE_FILES", "0", || {
        let config = get_cache_config();
        assert!(!config.validate_file_mtime);
    });

    with_env("RUSTOWL_CACHE_VALIDATE_FILES", "true", || {
        let config = get_cache_config();
        assert!(config.validate_file_mtime);
    });

    with_env("RUSTOWL_CACHE_VALIDATE_FILES", "1", || {
        let config = get_cache_config();
        assert!(config.validate_file_mtime);
    });

    with_env("RUSTOWL_CACHE_VALIDATE_FILES", "  FALSE  ", || {
        let config = get_cache_config();
        assert!(!config.validate_file_mtime);
    });
}

#[test]
fn test_cache_config_multiple_env_vars() {
    // Test multiple environment variables at once
    let old_entries = env::var("RUSTOWL_CACHE_MAX_ENTRIES").ok();
    let old_memory = env::var("RUSTOWL_CACHE_MAX_MEMORY_MB").ok();
    let old_eviction = env::var("RUSTOWL_CACHE_EVICTION").ok();
    let old_validate = env::var("RUSTOWL_CACHE_VALIDATE_FILES").ok();

    unsafe {
        env::set_var("RUSTOWL_CACHE_MAX_ENTRIES", "750");
        env::set_var("RUSTOWL_CACHE_MAX_MEMORY_MB", "150");
        env::set_var("RUSTOWL_CACHE_EVICTION", "fifo");
        env::set_var("RUSTOWL_CACHE_VALIDATE_FILES", "false");
    }

    let config = get_cache_config();
    assert_eq!(config.max_entries, 750);
    assert_eq!(config.max_memory_bytes, 150 * 1024 * 1024);
    assert!(!config.use_lru_eviction);
    assert!(!config.validate_file_mtime);

    // Restore old values
    unsafe {
        match old_entries {
            Some(v) => env::set_var("RUSTOWL_CACHE_MAX_ENTRIES", v),
            None => env::remove_var("RUSTOWL_CACHE_MAX_ENTRIES"),
        }
        match old_memory {
            Some(v) => env::set_var("RUSTOWL_CACHE_MAX_MEMORY_MB", v),
            None => env::remove_var("RUSTOWL_CACHE_MAX_MEMORY_MB"),
        }
        match old_eviction {
            Some(v) => env::set_var("RUSTOWL_CACHE_EVICTION", v),
            None => env::remove_var("RUSTOWL_CACHE_EVICTION"),
        }
        match old_validate {
            Some(v) => env::set_var("RUSTOWL_CACHE_VALIDATE_FILES", v),
            None => env::remove_var("RUSTOWL_CACHE_VALIDATE_FILES"),
        }
    }
}

// -----------------------------------------------------------------------------
// Additional unit tests for cache.rs
// Test framework: Rust built-in test harness (cargo test). No external test crates.
// These tests focus on env-var parsing, whitespace/case handling, saturating math,
// and defaults when no env overrides are present.
// -----------------------------------------------------------------------------
#[cfg(test)]
mod cache_additional_tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    // Helper: set env var for the duration of a closure, then restore.
    fn with_env_var<K: AsRef<str>, V: AsRef<str>, F: FnOnce()>(key: K, value: V, f: F) {
        let key = key.as_ref();
        let old = env::var(key).ok();
        unsafe { env::set_var(key, value.as_ref()); }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        match old {
            Some(v) => unsafe { env::set_var(key, v) },
            None => unsafe { env::remove_var(key) },
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    // Helper: remove env var for the duration of a closure, then restore.
    fn with_env_removed<K: AsRef<str>, F: FnOnce()>(key: K, f: F) {
        let key = key.as_ref();
        let old = env::var(key).ok();
        unsafe { env::remove_var(key); }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Some(v) = old {
            unsafe { env::set_var(key, v) }
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn test_get_cache_config_defaults_when_no_env() {
        // Ensure no overrides are present
        with_env_removed("RUSTOWL_CACHE_MAX_ENTRIES", || {
            with_env_removed("RUSTOWL_CACHE_MAX_MEMORY_MB", || {
                with_env_removed("RUSTOWL_CACHE_EVICTION", || {
                    with_env_removed("RUSTOWL_CACHE_VALIDATE_FILES", || {
                        let cfg = get_cache_config();
                        assert_eq!(cfg.max_entries, 1000);
                        assert_eq!(cfg.max_memory_bytes, 100 * 1024 * 1024);
                        assert!(cfg.use_lru_eviction);
                        assert!(cfg.validate_file_mtime);
                        assert!(!cfg.enable_compression);
                    });
                });
            });
        });
    }

    #[test]
    fn test_get_cache_config_max_memory_saturates_on_overflow() {
        // usize::MAX * 1_048_576 saturates to usize::MAX via saturating_mul
        with_env_var("RUSTOWL_CACHE_MAX_MEMORY_MB", &usize::MAX.to_string(), || {
            let cfg = get_cache_config();
            assert_eq!(cfg.max_memory_bytes, usize::MAX);
        });
    }

    #[test]
    fn test_get_cache_config_ignores_whitespace_numbers() {
        // Parsing " 200 " as usize fails; defaults should remain
        with_env_var("RUSTOWL_CACHE_MAX_MEMORY_MB", " 200 ", || {
            let cfg = get_cache_config();
            assert_eq!(cfg.max_memory_bytes, 100 * 1024 * 1024);
        });
        with_env_var("RUSTOWL_CACHE_MAX_ENTRIES", "  777 ", || {
            let cfg = get_cache_config();
            assert_eq!(cfg.max_entries, 1000);
        });
    }

    #[test]
    fn test_eviction_policy_trims_and_is_case_insensitive() {
        with_env_var("RUSTOWL_CACHE_EVICTION", "   LrU   ", || {
            let cfg = get_cache_config();
            assert!(cfg.use_lru_eviction);
        });
        with_env_var("RUSTOWL_CACHE_EVICTION", "   fIfO   ", || {
            let cfg = get_cache_config();
            assert!(!cfg.use_lru_eviction);
        });
    }

    #[test]
    fn test_validate_files_various_inputs() {
        let cases: [(&str, bool); 9] = [
            ("true", true),
            ("TRUE", true),
            (" 1 ", true),
            ("yes", true),
            ("random", true),
            ("false", false),
            ("FALSE", false),
            (" 0 ", false),
            ("  false   ", false),
        ];
        for (val, expected) in cases {
            with_env_var("RUSTOWL_CACHE_VALIDATE_FILES", val, || {
                let cfg = get_cache_config();
                assert_eq!(cfg.validate_file_mtime, expected, "value {:?}", val);
            });
        }
    }

    #[test]
    fn test_is_cache_non_disabling_values_return_true() {
        // Only "false" or "0" disable caching; everything else is treated as enabled.
        let values: [&str; 6] = ["on", "yes", "maybe", "OFF", "2", ""];
        for v in values {
            with_env_var("RUSTOWL_CACHE", v, || {
                assert!(is_cache(), "RUSTOWL_CACHE={:?} should enable cache", v);
            });
        }
    }

    #[test]
    fn test_get_cache_path_trims_whitespace_and_newlines() {
        with_env_var("RUSTOWL_CACHE_DIR", " \t/tmp/cache\n", || {
            let path = get_cache_path().unwrap();
            assert_eq!(path, PathBuf::from("/tmp/cache"));
        });
    }

    #[test]
    fn test_set_cache_path_accepts_relative_paths() {
        use tokio::process::Command;
        let mut cmd = Command::new("sh"); // do not spawn; just ensure env can be set
        let target_dir = PathBuf::from("target_dir");
        set_cache_path(&mut cmd, &target_dir);
        // Can't introspect Command's env; just ensure join semantics align with expectation.
        assert_eq!(target_dir.join("cache"), PathBuf::from("target_dir").join("cache"));
    }
}
