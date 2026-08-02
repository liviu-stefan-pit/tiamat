//! Windows executable / wrapper argv normalization shared by hosted spawn and probes.

use std::path::Path;

/// Normalize argv so `.cmd`/`.bat` run via `cmd.exe` and `.ps1` via PowerShell.
/// Non-wrapper executables are returned unchanged.
#[cfg(windows)]
pub fn normalize_windows_argv(argv: &[String]) -> Vec<String> {
    if argv.is_empty() {
        return Vec::new();
    }
    let program = &argv[0];
    let args = &argv[1..];
    let lower = program.to_ascii_lowercase();
    if lower.ends_with(".cmd") || lower.ends_with(".bat") {
        let mut out = vec!["cmd.exe".to_string(), "/d".into(), "/s".into(), "/c".into()];
        // Build a single `/c` command string so CreateProcessW and CommandExt agree.
        let mut cmdline = quote_windows_arg(program);
        for arg in args {
            cmdline.push(' ');
            cmdline.push_str(&quote_windows_arg(arg));
        }
        out.push(cmdline);
        return out;
    }
    if lower.ends_with(".ps1") {
        let powershell =
            Path::new(&std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe");
        let mut out = vec![
            powershell.display().to_string(),
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            program.clone(),
        ];
        out.extend(args.iter().cloned());
        return out;
    }
    argv.to_vec()
}

#[cfg(not(windows))]
pub fn normalize_windows_argv(argv: &[String]) -> Vec<String> {
    argv.to_vec()
}

/// Quote a single Windows command-line argument (CreateProcessW / cmd.exe rules).
pub fn quote_windows_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".into();
    }
    let needs = arg.chars().any(|c| c == ' ' || c == '\t' || c == '"');
    if !needs {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0u32;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..(backslashes * 2 + 1) {
                    out.push('\\');
                }
                out.push('"');
                backslashes = 0;
            }
            _ => {
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_handles_spaces() {
        assert_eq!(quote_windows_arg("a b"), "\"a b\"");
        assert_eq!(quote_windows_arg("plain"), "plain");
    }

    #[cfg(windows)]
    #[test]
    fn cmd_wrappers_route_through_cmd_exe() {
        let normalized =
            normalize_windows_argv(&[r"C:\tools\agent.cmd".into(), "--version".into()]);
        assert_eq!(normalized[0].to_ascii_lowercase(), "cmd.exe");
        assert!(normalized.iter().any(|a| a == "/c"));
        assert!(normalized
            .last()
            .is_some_and(|c| c.to_ascii_lowercase().contains("agent.cmd")));
    }

    #[cfg(windows)]
    #[test]
    fn ps1_wrappers_route_through_powershell() {
        let normalized = normalize_windows_argv(&[r"C:\tools\agent.ps1".into(), "--help".into()]);
        assert!(normalized[0].to_ascii_lowercase().contains("powershell"));
        assert!(normalized.iter().any(|a| a == "-File"));
    }
}
