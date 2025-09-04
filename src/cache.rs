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

/// Get cache configuration from environment variables
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

    fn with_env<F>(key: &str, value: &str, f: F) 
    where
        F: FnOnce(),
    {
        let old_value = env::var(key).ok();
        unsafe { env::set_var(key, value); }
        f();
        match old_value {
            Some(v) => unsafe { env::set_var(key, v); },
            None => unsafe { env::remove_var(key); },
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
        unsafe { env::remove_var("RUSTOWL_CACHE"); }
        
        assert!(is_cache()); // Should be true by default
        
        // Restore old value
        if let Some(v) = old_value {
            unsafe { env::set_var("RUSTOWL_CACHE", v); }
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
        unsafe { env::remove_var("RUSTOWL_CACHE_DIR"); }
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
            unsafe { env::set_var("RUSTOWL_CACHE_DIR", v); }
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

        // Test invalid max entries (should use default)
        with_env("RUSTOWL_CACHE_MAX_ENTRIES", "invalid", || {
            let config = get_cache_config();
            assert_eq!(config.max_entries, 1000); // default
        });

        // Test max memory configuration
        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", "200", || {
            let config = get_cache_config();
            assert_eq!(config.max_memory_bytes, 200 * 1024 * 1024);
        });

        // Test max memory with overflow protection
        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", &usize::MAX.to_string(), || {
            let config = get_cache_config();
            // Should use saturating_mul, so might be different from exact calculation
            assert!(config.max_memory_bytes > 0);
        });

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
}

































        }
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    // Note: Using Rust's built-in test harness (#[test]) with no external libraries.
    // Helper to set and restore a single env var around a closure (matches existing pattern).
    fn with_env<F>(key: &str, value: &str, f: F)
    where
        F: FnOnce(),
    {
        let old_value = env::var(key).ok();
        unsafe { env::set_var(key, value); }
        f();
        match old_value {
            Some(v) => unsafe { env::set_var(key, v); },
            None => unsafe { env::remove_var(key); },
        }
    }

    #[test]
    fn test_is_cache_whitespace_only_value() {
        with_env("RUSTOWL_CACHE", "    ", || {
            assert!(is_cache(), "Whitespace-only should not disable cache");
        });
    }

    #[test]
    fn test_is_cache_mixed_case_false() {
        with_env("RUSTOWL_CACHE", "FaLsE", || {
            assert!(!is_cache(), "Mixed-case 'FaLsE' should disable cache");
        });
    }

    #[test]
    fn test_is_cache_zero_with_spaces_and_newline() {
        with_env("RUSTOWL_CACHE", " 0 \n", || {
            assert!(!is_cache(), "'0' with surrounding whitespace/newline should disable cache");
        });
    }

    #[test]
    fn test_is_cache_true_with_spaces() {
        with_env("RUSTOWL_CACHE", "   true   ", || {
            assert!(is_cache(), "'true' with surrounding whitespace should keep cache enabled");
        });
    }

    #[test]
    fn test_get_cache_path_relative_and_trim() {
        with_env("RUSTOWL_CACHE_DIR", "  relative/path  ", || {
            let path = get_cache_path().expect("path should be Some");
            assert_eq!(path, PathBuf::from("relative/path"));
        });
    }

    #[test]
    fn test_get_cache_config_defaults_when_no_env() {
        // Temporarily clear relevant env vars and assert defaults from get_cache_config.
        let keys = [
            "RUSTOWL_CACHE_MAX_ENTRIES",
            "RUSTOWL_CACHE_MAX_MEMORY_MB",
            "RUSTOWL_CACHE_EVICTION",
            "RUSTOWL_CACHE_VALIDATE_FILES",
        ];
        let saved: Vec<_> = keys.iter().map(|k| env::var(k).ok()).collect();

        unsafe {
            for k in keys.iter() {
                env::remove_var(k);
            }
        }

        let config = get_cache_config();
        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.max_memory_bytes, 100 * 1024 * 1024);
        assert!(config.use_lru_eviction);
        assert!(config.validate_file_mtime);
        assert!(!config.enable_compression);

        unsafe {
            for (i, k) in keys.iter().enumerate() {
                match &saved[i] {
                    Some(v) => env::set_var(k, v),
                    None => env::remove_var(k),
                }
            }
        }
    }

    #[test]
    fn test_get_cache_config_invalid_max_memory_keeps_default() {
        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", "abc", || {
            let config = get_cache_config();
            assert_eq!(config.max_memory_bytes, 100 * 1024 * 1024);
        });
    }

    #[test]
    fn test_get_cache_config_numeric_with_whitespace_ignored_for_parse() {
        // Numeric envs are not trimmed before parse; whitespace should fail parse and keep defaults.
        with_env("RUSTOWL_CACHE_MAX_ENTRIES", " 300 ", || {
            let config = get_cache_config();
            assert_eq!(config.max_entries, 1000, "whitespace should keep default");
        });

        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", "  123  ", || {
            let config = get_cache_config();
            assert_eq!(config.max_memory_bytes, 100 * 1024 * 1024, "whitespace should keep default");
        });
    }

    #[test]
    fn test_get_cache_config_numeric_zero_values() {
        with_env("RUSTOWL_CACHE_MAX_ENTRIES", "0", || {
            let config = get_cache_config();
            assert_eq!(config.max_entries, 0);
        });

        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", "0", || {
            let config = get_cache_config();
            assert_eq!(config.max_memory_bytes, 0);
        });
    }

    #[test]
    fn test_get_cache_config_oversized_values_saturate() {
        with_env("RUSTOWL_CACHE_MAX_MEMORY_MB", &usize::MAX.to_string(), || {
            let config = get_cache_config();
            assert_eq!(config.max_memory_bytes, usize::MAX, "saturating_mul should cap at usize::MAX");
        });

        with_env("RUSTOWL_CACHE_MAX_ENTRIES", &usize::MAX.to_string(), || {
            let config = get_cache_config();
            assert_eq!(config.max_entries, usize::MAX, "entries should accept very large usize values");
        });
    }

    #[test]
    fn test_get_cache_config_eviction_policy_with_whitespace_and_mixed_case() {
        with_env("RUSTOWL_CACHE_EVICTION", "  fIfO  ", || {
            let config = get_cache_config();
            assert!(!config.use_lru_eviction);
        });

        with_env("RUSTOWL_CACHE_EVICTION", "  lRu  ", || {
            let config = get_cache_config();
            assert!(config.use_lru_eviction);
        });
    }

    #[test]
    fn test_get_cache_config_validate_files_mixed_case_and_whitespace() {
        with_env("RUSTOWL_CACHE_VALIDATE_FILES", "  TrUe  ", || {
            let config = get_cache_config();
            assert!(config.validate_file_mtime);
        });

        with_env("RUSTOWL_CACHE_VALIDATE_FILES", " false\n", || {
            let config = get_cache_config();
            assert!(!config.validate_file_mtime);
        });
    }

    #[test]
    fn test_get_cache_config_enable_compression_ignored_env() {
        // Intentionally verify that compression remains at default (false) even if an env var exists.
        // This codifies current behavior to preserve compatibility as noted in the default impl comment.
        with_env("RUSTOWL_CACHE_ENABLE_COMPRESSION", "true", || {
            let config = get_cache_config();
            assert!(!config.enable_compression);
        });

        with_env("RUSTOWL_CACHE_ENABLE_COMPRESSION", "false", || {
            let config = get_cache_config();
            assert!(!config.enable_compression);
        });
    }
}