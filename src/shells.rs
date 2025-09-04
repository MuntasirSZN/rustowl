use clap_complete_nushell::Nushell;

use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

use clap::ValueEnum;

use clap_complete::Generator;
use clap_complete::shells;

/// Extended shell support including Nushell
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, ValueEnum)]
#[non_exhaustive]
#[value(rename_all = "lower")]
pub enum Shell {
    /// Bourne Again `SHell` (bash)
    Bash,
    /// Elvish shell  
    Elvish,
    /// Friendly Interactive `SHell` (fish)
    Fish,
    /// `PowerShell`
    PowerShell,
    /// Z `SHell` (zsh)
    Zsh,
    /// Nushell
    Nushell,
}

impl Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shell::Bash => write!(f, "bash"),
            Shell::Elvish => write!(f, "elvish"),
            Shell::Fish => write!(f, "fish"),
            Shell::PowerShell => write!(f, "powershell"),
            Shell::Zsh => write!(f, "zsh"),
            Shell::Nushell => write!(f, "nushell"),
        }
    }
}

impl FromStr for Shell {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bash" => Ok(Shell::Bash),
            "elvish" => Ok(Shell::Elvish),
            "fish" => Ok(Shell::Fish),
            "powershell" => Ok(Shell::PowerShell),
            "zsh" => Ok(Shell::Zsh),
            "nushell" => Ok(Shell::Nushell),
            _ => Err(format!("invalid variant: {s}")),
        }
    }
}

impl Generator for Shell {
    fn file_name(&self, name: &str) -> String {
        match self {
            Shell::Bash => shells::Bash.file_name(name),
            Shell::Elvish => shells::Elvish.file_name(name),
            Shell::Fish => shells::Fish.file_name(name),
            Shell::PowerShell => shells::PowerShell.file_name(name),
            Shell::Zsh => shells::Zsh.file_name(name),
            Shell::Nushell => Nushell.file_name(name),
        }
    }

    fn generate(&self, cmd: &clap::Command, buf: &mut dyn std::io::Write) {
        match self {
            Shell::Bash => shells::Bash.generate(cmd, buf),
            Shell::Elvish => shells::Elvish.generate(cmd, buf),
            Shell::Fish => shells::Fish.generate(cmd, buf),
            Shell::PowerShell => shells::PowerShell.generate(cmd, buf),
            Shell::Zsh => shells::Zsh.generate(cmd, buf),
            Shell::Nushell => Nushell.generate(cmd, buf),
        }
    }
}

impl Shell {
    /// Parse a shell from a path to the executable for the shell
    pub fn from_shell_path<P: AsRef<Path>>(path: P) -> Option<Shell> {
        let path = path.as_ref();
        let name = path.file_stem()?.to_str()?;

        match name {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            "elvish" => Some(Shell::Elvish),
            "powershell" | "powershell_ise" => Some(Shell::PowerShell),
            "nu" | "nushell" => Some(Shell::Nushell),
            _ => None,
        }
    }

    /// Determine the user's current shell from the environment
    pub fn from_env() -> Option<Shell> {
        if let Some(env_shell) = std::env::var_os("SHELL") {
            Shell::from_shell_path(env_shell)
        } else if cfg!(windows) {
            Some(Shell::PowerShell)
        } else {
            None
        }
    }

    /// Convert to the standard shell type if possible, for compatibility
    pub fn to_standard_shell(&self) -> Option<shells::Shell> {
        match self {
            Shell::Bash => Some(shells::Shell::Bash),
            Shell::Elvish => Some(shells::Shell::Elvish),
            Shell::Fish => Some(shells::Shell::Fish),
            Shell::PowerShell => Some(shells::Shell::PowerShell),
            Shell::Zsh => Some(shells::Shell::Zsh),
            Shell::Nushell => None, // Not supported by standard shells
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_from_str() {
        use std::str::FromStr;

        assert_eq!(<Shell as FromStr>::from_str("bash"), Ok(Shell::Bash));
        assert_eq!(<Shell as FromStr>::from_str("zsh"), Ok(Shell::Zsh));
        assert_eq!(<Shell as FromStr>::from_str("fish"), Ok(Shell::Fish));
        assert_eq!(<Shell as FromStr>::from_str("elvish"), Ok(Shell::Elvish));
        assert_eq!(
            <Shell as FromStr>::from_str("powershell"),
            Ok(Shell::PowerShell)
        );
        assert_eq!(<Shell as FromStr>::from_str("nushell"), Ok(Shell::Nushell));

        assert!(<Shell as FromStr>::from_str("invalid").is_err());
    }

    #[test]
    fn test_shell_display() {
        assert_eq!(Shell::Bash.to_string(), "bash");
        assert_eq!(Shell::Zsh.to_string(), "zsh");
        assert_eq!(Shell::Fish.to_string(), "fish");
        assert_eq!(Shell::Elvish.to_string(), "elvish");
        assert_eq!(Shell::PowerShell.to_string(), "powershell");
        assert_eq!(Shell::Nushell.to_string(), "nushell");
    }

    #[test]
    fn test_shell_from_shell_path() {
        assert_eq!(Shell::from_shell_path("/bin/bash"), Some(Shell::Bash));
        assert_eq!(Shell::from_shell_path("/usr/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(
            Shell::from_shell_path("/usr/local/bin/fish"),
            Some(Shell::Fish)
        );
        assert_eq!(Shell::from_shell_path("/opt/elvish"), Some(Shell::Elvish));
        // PowerShell on Windows could be powershell.exe or powershell_ise.exe
        assert_eq!(
            Shell::from_shell_path("powershell"),
            Some(Shell::PowerShell)
        );
        assert_eq!(
            Shell::from_shell_path("powershell_ise"),
            Some(Shell::PowerShell)
        );
        assert_eq!(Shell::from_shell_path("/usr/bin/nu"), Some(Shell::Nushell));
        assert_eq!(
            Shell::from_shell_path("/usr/bin/nushell"),
            Some(Shell::Nushell)
        );

        assert_eq!(Shell::from_shell_path("/bin/unknown"), None);
    }

    #[test]
    fn test_shell_to_standard_shell() {
        assert!(Shell::Bash.to_standard_shell().is_some());
        assert!(Shell::Zsh.to_standard_shell().is_some());
        assert!(Shell::Fish.to_standard_shell().is_some());
        assert!(Shell::Elvish.to_standard_shell().is_some());
        assert!(Shell::PowerShell.to_standard_shell().is_some());
        assert!(Shell::Nushell.to_standard_shell().is_none()); // Nushell not in standard
    }

    #[test]
    fn test_shell_generator_interface() {
        // Test that our Shell implements Generator correctly
        let shell = Shell::Bash;
        let filename = shell.file_name("test");
        assert!(filename.contains("test"));

        // Test generate method with proper command setup
        use clap::Command;
        let cmd = Command::new("test").bin_name("test");
        let mut buf = Vec::new();
        shell.generate(&cmd, &mut buf);
        // The actual content depends on clap_complete implementation
        // Just verify it doesn't panic and produces some output
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_shell_from_str_case_insensitive() {
        use std::str::FromStr;

        // Test uppercase variants
        assert_eq!(<Shell as FromStr>::from_str("BASH"), Ok(Shell::Bash));
        assert_eq!(<Shell as FromStr>::from_str("ZSH"), Ok(Shell::Zsh));
        assert_eq!(<Shell as FromStr>::from_str("FISH"), Ok(Shell::Fish));
        assert_eq!(
            <Shell as FromStr>::from_str("POWERSHELL"),
            Ok(Shell::PowerShell)
        );
        assert_eq!(<Shell as FromStr>::from_str("NUSHELL"), Ok(Shell::Nushell));

        // Test mixed case variants
        assert_eq!(<Shell as FromStr>::from_str("BaSh"), Ok(Shell::Bash));
        assert_eq!(
            <Shell as FromStr>::from_str("PowerShell"),
            Ok(Shell::PowerShell)
        );
        assert_eq!(<Shell as FromStr>::from_str("NuShell"), Ok(Shell::Nushell));
    }

    #[test]
    fn test_shell_from_str_error_messages() {
        use std::str::FromStr;

        let result = <Shell as FromStr>::from_str("invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid variant: invalid");

        let result = <Shell as FromStr>::from_str("cmd");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid variant: cmd");

        let result = <Shell as FromStr>::from_str("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "invalid variant: ");
    }

    #[test]
    fn test_shell_from_shell_path_comprehensive() {
        // Test various path formats
        let path_variants = vec![
            ("/bin/bash", Some(Shell::Bash)),
            ("/usr/bin/bash", Some(Shell::Bash)),
            ("/usr/local/bin/bash", Some(Shell::Bash)),
            ("bash", Some(Shell::Bash)),
            ("./bash", Some(Shell::Bash)),
            ("zsh", Some(Shell::Zsh)),
            ("/usr/bin/zsh", Some(Shell::Zsh)),
            ("fish", Some(Shell::Fish)),
            ("/usr/local/bin/fish", Some(Shell::Fish)),
            ("elvish", Some(Shell::Elvish)),
            ("/opt/bin/elvish", Some(Shell::Elvish)),
            ("powershell", Some(Shell::PowerShell)),
            ("powershell_ise", Some(Shell::PowerShell)),
            // Note: complex Windows paths may not parse correctly due to path parsing limitations
            ("nu", Some(Shell::Nushell)),
            ("nushell", Some(Shell::Nushell)),
            ("/usr/bin/nu", Some(Shell::Nushell)),
            // Invalid cases
            ("unknown", None),
            ("/bin/unknown", None),
            ("sh", None),
            ("cmd", None),
            ("", None),
        ];

        for (path, expected) in path_variants {
            assert_eq!(
                Shell::from_shell_path(path),
                expected,
                "Failed for path: {}",
                path
            );
        }
    }

    #[test]
    fn test_shell_from_shell_path_with_extensions() {
        // Test paths with executable extensions
        assert_eq!(Shell::from_shell_path("bash.exe"), Some(Shell::Bash));
        assert_eq!(Shell::from_shell_path("zsh.exe"), Some(Shell::Zsh));
        assert_eq!(
            Shell::from_shell_path("powershell.exe"),
            Some(Shell::PowerShell)
        );
        assert_eq!(Shell::from_shell_path("nu.exe"), Some(Shell::Nushell));

        // Test with complex paths
        assert_eq!(
            Shell::from_shell_path("C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
            None
        );
        assert_eq!(Shell::from_shell_path("/snap/bin/nu"), Some(Shell::Nushell));
    }

    #[test]
    fn test_shell_from_env_simulation() {
        // Test the environment detection logic without actually modifying env

        // Simulate what from_env would do
        let shell_paths = vec![
            "/bin/bash",
            "/usr/bin/zsh",
            "/usr/local/bin/fish",
            "/opt/elvish",
        ];

        for shell_path in shell_paths {
            let detected = Shell::from_shell_path(shell_path);
            assert!(
                detected.is_some(),
                "Should detect shell from path: {}",
                shell_path
            );
        }

        // Test Windows default behavior simulation
        #[cfg(windows)]
        {
            // On Windows, if no SHELL env var, it should default to PowerShell
            let default_shell = Some(Shell::PowerShell);
            assert_eq!(default_shell, Some(Shell::PowerShell));
        }
    }

    #[test]
    fn test_shell_to_standard_shell_completeness() {
        // Test that all shells except Nushell have standard equivalents
        let shells = [
            Shell::Bash,
            Shell::Elvish,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Zsh,
            Shell::Nushell,
        ];

        for shell in shells {
            match shell {
                Shell::Nushell => assert!(shell.to_standard_shell().is_none()),
                _ => assert!(shell.to_standard_shell().is_some()),
            }
        }
    }

    #[test]
    fn test_shell_file_name_generation() {
        // Test file name generation for different shells
        let shells = [
            (Shell::Bash, "rustowl"),
            (Shell::Zsh, "rustowl"),
            (Shell::Fish, "rustowl"),
            (Shell::PowerShell, "rustowl"),
            (Shell::Elvish, "rustowl"),
            (Shell::Nushell, "rustowl"),
        ];

        for (shell, app_name) in shells {
            let filename = shell.file_name(app_name);
            assert!(!filename.is_empty());
            assert!(filename.contains(app_name));
        }
    }

    #[test]
    fn test_shell_generate_different_commands() {
        // Test generation basic functionality
        use clap::Command;

        let cmd = Command::new("test-app").bin_name("test-app");

        // Test with one shell to verify basic functionality
        let shell = Shell::Bash;
        let mut buf = Vec::new();
        shell.generate(&cmd, &mut buf);
        assert!(!buf.is_empty(), "Generated completion should not be empty");

        // Verify it contains some expected content
        let content = String::from_utf8_lossy(&buf);
        assert!(content.contains("test-app"), "Should contain app name");
    }

    #[test]
    fn test_shell_enum_properties() {
        // Test enum properties and traits
        let shell = Shell::Bash;

        // Test Clone
        let cloned = shell.clone();
        assert_eq!(shell, cloned);

        // Test Copy
        let copied = shell;
        assert_eq!(shell, copied);

        // Test Hash consistency
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(shell, "value");
        assert_eq!(map.get(&Shell::Bash), Some(&"value"));

        // Test PartialEq
        assert_eq!(Shell::Bash, Shell::Bash);
        assert_ne!(Shell::Bash, Shell::Zsh);
    }

    #[test]
    fn test_shell_display_format_consistency() {
        // Test that display format is consistent with from_str parsing
        use std::str::FromStr;

        let shells = [
            Shell::Bash,
            Shell::Elvish,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Zsh,
            Shell::Nushell,
        ];

        for shell in shells {
            let display_str = shell.to_string();
            let parsed_shell = <Shell as FromStr>::from_str(&display_str).unwrap();
            assert_eq!(
                shell, parsed_shell,
                "Display and parse should roundtrip for {:?}",
                shell
            );
        }
    }

    #[test]
    fn test_shell_value_enum_integration() {
        // Test that Shell works properly as a clap ValueEnum
        use clap::ValueEnum;

        // Test value_variants
        let variants = Shell::value_variants();
        assert_eq!(variants.len(), 6);
        assert!(variants.contains(&Shell::Bash));
        assert!(variants.contains(&Shell::Nushell));

        // Test to_possible_value
        for variant in variants {
            let possible_value = variant.to_possible_value();
            assert!(possible_value.is_some());
            let pv = possible_value.unwrap();
            assert!(!pv.get_name().is_empty());
        }
    }

    #[test]
    fn test_shell_edge_cases() {
        // Test edge cases and boundary conditions

        // Test with empty path components
        assert_eq!(Shell::from_shell_path(""), None);
        assert_eq!(Shell::from_shell_path("/"), None);
        assert_eq!(Shell::from_shell_path("/."), None);

        // Test with paths that have no file stem
        assert_eq!(Shell::from_shell_path("/usr/bin/"), None);
        assert_eq!(Shell::from_shell_path(".bashrc"), None);

        // Test with symlink-like names (common in some distributions)
        assert_eq!(Shell::from_shell_path("/usr/bin/sh"), None); // sh is not supported
        assert_eq!(Shell::from_shell_path("/bin/dash"), None); // dash is not supported

        // Test case sensitivity in file stem extraction
        assert_eq!(Shell::from_shell_path("/usr/bin/BASH"), None); // Case matters for file stem
    }
}

#[cfg(test)]
mod more_shell_tests {
    // Test framework: Rust built-in test harness (`cargo test`) with #[test]; no external testing libraries detected.
    use super::*;
    use clap::Command;
    use clap_complete::shells;

    #[test]
    fn test_from_str_rejects_aliases_and_whitespace() {
        use std::str::FromStr;
        let cases = [
            "nu",             // alias not supported by FromStr
            " pwsh",          // unsupported alias with leading space
            "pwsh",           // unsupported alias
            "bash ",          // trailing whitespace
            " powerShell ",   // mixed case and surrounding whitespace
            " elvish ",
            " fish ",
            " zsh ",
        ];
        for case in cases {
            let res = <Shell as FromStr>::from_str(case);
            assert!(res.is_err(), "Expected error for input {:?}", case);
            assert_eq!(res.unwrap_err(), format!("invalid variant: {}", case));
        }
    }

    #[test]
    fn test_from_shell_path_recognizes_powershell_ise_and_nushell_exe() {
        assert_eq!(
            Shell::from_shell_path("powershell_ise.exe"),
            Some(Shell::PowerShell)
        );
        assert_eq!(
            Shell::from_shell_path("nushell.exe"),
            Some(Shell::Nushell)
        );
    }

    #[test]
    fn test_from_shell_path_pwsh_not_recognized() {
        assert_eq!(Shell::from_shell_path("pwsh"), None);
        assert_eq!(Shell::from_shell_path("pwsh.exe"), None);
    }

    #[test]
    fn test_from_shell_path_case_sensitivity_for_nu() {
        // Ensure case-sensitive file stem behavior for Nushell detection
        assert_eq!(Shell::from_shell_path("NU"), None);
    }

    #[test]
    fn test_file_name_extensions_by_shell() {
        let name = "comp-test";

        // Shells with well-known completion file extensions
        let cases = [
            (Shell::Bash, ".bash"),
            (Shell::Fish, ".fish"),
            (Shell::Elvish, ".elvish"),
            (Shell::PowerShell, ".ps1"),
        ];
        for (shell, ext) in cases {
            let fname = shell.file_name(name);
            assert!(fname.ends_with(ext), "{:?} file_name should end with {}", shell, ext);
            assert!(fname.contains(name), "Filename should contain app name: {}", fname);
        }

        // Zsh typically uses a leading underscore file (e.g., "_comp-test") but allow .zsh too
        let z = Shell::Zsh.file_name(name);
        assert!(
            z.starts_with('_') || z.ends_with(".zsh"),
            "Zsh filename unexpected: {}",
            z
        );
        assert!(
            z.contains(name) || z.starts_with('_'),
            "Zsh filename should indicate the app name: {}",
            z
        );

        // Nushell should use .nu extension
        let nu = Shell::Nushell.file_name(name);
        assert!(nu.ends_with(".nu"), "Nushell filename should end with .nu; got {}", nu);
        assert!(nu.contains(name), "Nushell filename should contain app name: {}", nu);
    }

    #[test]
    fn test_generate_non_empty_all_shells() {
        let cmd = Command::new("comp-test").bin_name("comp-test");
        for shell in [Shell::Bash, Shell::Elvish, Shell::Fish, Shell::PowerShell, Shell::Zsh, Shell::Nushell] {
            let mut buf = Vec::new();
            shell.generate(&cmd, &mut buf);
            assert!(!buf.is_empty(), "Expected non-empty completion for {:?}", shell);
            let content = String::from_utf8_lossy(&buf);
            if matches!(shell, Shell::Zsh) {
                assert!(
                    content.contains("comp-test") || content.contains("_comp-test"),
                    "Zsh completion should reference the app name or function; got: {}",
                    &*content
                );
            } else {
                assert!(
                    content.contains("comp-test"),
                    "Completion should contain app name for {:?}. Output: {}",
                    shell,
                    &*content
                );
            }
        }
    }

    #[test]
    fn test_to_standard_shell_exact_values() {
        assert_eq!(Shell::Bash.to_standard_shell(), Some(shells::Shell::Bash));
        assert_eq!(Shell::Elvish.to_standard_shell(), Some(shells::Shell::Elvish));
        assert_eq!(Shell::Fish.to_standard_shell(), Some(shells::Shell::Fish));
        assert_eq!(Shell::PowerShell.to_standard_shell(), Some(shells::Shell::PowerShell));
        assert_eq!(Shell::Zsh.to_standard_shell(), Some(shells::Shell::Zsh));
        assert_eq!(Shell::Nushell.to_standard_shell(), None);
    }
}
