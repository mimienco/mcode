//! Default shell selection: Windows pwsh → powershell → cmd (probed on
//! PATH); POSIX $SHELL → /bin/zsh → /bin/bash → /bin/sh with the login flag.
//! A caller-provided `shell_override` always wins.

/// A resolved shell launch: program plus argv prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellLaunch {
    pub program: String,
    pub args: Vec<String>,
    /// POSIX login shell flag (-l) resolved at selection time.
    pub login: bool,
}

/// Whether an executable exists on PATH (or the absolute path is runnable).
fn probe(program: &str) -> bool {
    if program.contains(std::path::MAIN_SEPARATOR) || program.contains('/') {
        return std::fs::metadata(program).is_ok();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}

/// Windows shell preference order, probed so an unavailable PowerShell
/// implementation degrades to the next shell (the original fallback chain,
/// `buildWindowsPowerShellSpawnAttempts`).
#[cfg(windows)]
fn windows_candidates() -> [(&'static str, Vec<&'static str>); 3] {
    [
        ("pwsh.exe", vec!["-NoLogo"]),
        ("powershell.exe", vec!["-NoLogo"]),
        ("cmd.exe", vec![]),
    ]
}

/// Resolves the default shell. `shell_override` wins over every probe.
pub fn select_shell(shell_override: Option<&str>) -> ShellLaunch {
    if let Some(ovr) = shell_override {
        return ShellLaunch {
            program: ovr.to_string(),
            args: Vec::new(),
            login: false,
        };
    }
    #[cfg(windows)]
    {
        for (program, args) in windows_candidates() {
            if probe(program) {
                return ShellLaunch {
                    program: program.to_string(),
                    args: args.into_iter().map(String::from).collect(),
                    login: false,
                };
            }
        }
        // Probes cannot fail on a sane Windows box (cmd.exe is system32
        // material), but the contract demands a total function:
        ShellLaunch {
            program: "cmd.exe".to_string(),
            args: Vec::new(),
            login: false,
        }
    }
    #[cfg(not(windows))]
    {
        let candidates: [(&str, bool); 4] = [
            ("", false), // placeholder replaced below
            ("/bin/zsh", true),
            ("/bin/bash", true),
            ("/bin/sh", false),
        ];
        // $SHELL first (login shell), then the fixed ladder.
        if let Some(user_shell) = std::env::var("SHELL").ok().filter(|s| !s.is_empty()) {
            if std::path::Path::new(&user_shell).exists() {
                return ShellLaunch {
                    program: user_shell,
                    args: vec!["-l".to_string()],
                    login: true,
                };
            }
        }
        for (program, login) in candidates.into_iter().skip(1) {
            if std::path::Path::new(program).exists() {
                return ShellLaunch {
                    program: program.to_string(),
                    args: if login {
                        vec!["-l".to_string()]
                    } else {
                        Vec::new()
                    },
                    login,
                };
            }
        }
        ShellLaunch {
            program: "/bin/sh".to_string(),
            args: Vec::new(),
            login: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_always() {
        let launch = select_shell(Some("/custom/fish"));
        assert_eq!(launch.program, "/custom/fish");
        assert!(launch.args.is_empty());
        assert!(!launch.login);
    }

    #[cfg(windows)]
    #[test]
    fn windows_ladder_lands_on_a_probe() {
        let launch = select_shell(None);
        // On any sane Windows box one of the three exists; cmd.exe is last.
        assert!(["pwsh.exe", "powershell.exe", "cmd.exe"].contains(&launch.program.as_str()));
        assert!(!launch.login);
        if launch.program == "pwsh.exe" || launch.program == "powershell.exe" {
            assert_eq!(launch.args, vec!["-NoLogo"]);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_probe_order_prefers_pwsh_when_present() {
        // Probe the same list directly: the first existing candidate must be
        // the selection.
        let candidates = windows_candidates();
        let first_existing = candidates
            .iter()
            .find(|(program, _)| probe(program))
            .map(|(p, _)| p);
        if let Some(expected) = first_existing {
            assert_eq!(select_shell(None).program, *expected);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_ladder_prefers_user_shell() {
        // SAFETY: test-only env mutation.
        unsafe {
            std::env::set_var("SHELL", "/bin/bash");
        }
        let launch = select_shell(None);
        assert_eq!(launch.program, "/bin/bash");
        assert!(launch.login);
        unsafe {
            std::env::remove_var("SHELL");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_ladder_falls_through_to_bin_sh() {
        unsafe {
            std::env::remove_var("SHELL");
        }
        let launch = select_shell(None);
        assert!(["/bin/zsh", "/bin/bash", "/bin/sh"].contains(&launch.program.as_str()));
    }

    #[test]
    fn probe_rejects_missing_binary() {
        assert!(!probe("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn probe_accepts_cmd_on_windows() {
        #[cfg(windows)]
        assert!(probe("cmd.exe"));
    }
}
