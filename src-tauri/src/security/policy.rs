//! Command policy (§10.3) with optional managed-root scoping.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPolicyDecision {
    Allow,
    Deny { reason: String },
}

/// Evaluate command policy and optionally require working directory under approved roots.
pub fn evaluate_command_policy_in_roots(
    command: &[String],
    working_directory: &Path,
    approved_roots: Option<&[String]>,
) -> CommandPolicyDecision {
    let base = evaluate_command_policy(command, working_directory);
    if matches!(base, CommandPolicyDecision::Deny { .. }) {
        return base;
    }
    if let Some(roots) = approved_roots {
        if !working_directory.as_os_str().is_empty() {
            let cwd = working_directory
                .canonicalize()
                .unwrap_or_else(|_| working_directory.to_path_buf());
            let ok = roots.iter().any(|root| {
                let root_path = Path::new(root);
                let canon = root_path
                    .canonicalize()
                    .unwrap_or_else(|_| root_path.to_path_buf());
                cwd.starts_with(&canon) || working_directory.starts_with(root_path)
            });
            if !ok && !roots.is_empty() {
                return CommandPolicyDecision::Deny {
                    reason: format!(
                        "working directory {} outside approved roots",
                        working_directory.display()
                    ),
                };
            }
        }
    }
    base
}

/// Default allow/deny for architect-specified test and inspection commands (MASTER-PLAN §10.3).
pub fn evaluate_command_policy(
    command: &[String],
    working_directory: &Path,
) -> CommandPolicyDecision {
    if command.is_empty() {
        return CommandPolicyDecision::Deny {
            reason: "empty command".into(),
        };
    }
    // Matching is case-insensitive, but the path itself must stay verbatim: Unix paths
    // are case-sensitive, so lowercasing before a containment check corrupts them.
    let prog = command[0].as_str();
    let prog_name = Path::new(prog)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(prog)
        .to_ascii_lowercase();

    let joined = command.join(" ").to_ascii_lowercase();

    // Credential / secret dumping.
    let secret_dump_tokens = [
        "cmdkey",
        "credwizard",
        "mimikatz",
        "procdump",
        "secretsdump",
        "aws configure",
        "printenv",
        "setx ",
    ];
    for token in secret_dump_tokens {
        if prog_name == token || joined.contains(token) {
            return CommandPolicyDecision::Deny {
                reason: format!("credential/secret dumping command denied ('{token}')"),
            };
        }
    }

    // Network publish / deploy.
    let publish_tokens = [
        "curl ",
        "wget ",
        "scp ",
        "sftp ",
        "ftp ",
        "gh release",
        "npm publish",
        "cargo publish",
        "docker push",
        "git push",
        "aws s3 cp",
        "az storage",
    ];
    // Allow git push only when not force (handled below); deny bare network tools.
    if matches!(
        prog_name.as_str(),
        "curl" | "curl.exe" | "wget" | "wget.exe" | "scp" | "scp.exe" | "sftp" | "sftp.exe"
    ) {
        return CommandPolicyDecision::Deny {
            reason: format!("network publish/transfer tool '{prog_name}' denied"),
        };
    }
    for token in publish_tokens {
        if token == "git push" {
            continue;
        }
        if joined.contains(token) {
            return CommandPolicyDecision::Deny {
                reason: format!("network publish/deploy token '{token}' denied"),
            };
        }
    }

    let denied_tokens = [
        "format",
        "diskpart",
        "reg.exe",
        "reg ",
        "netsh",
        "sc.exe",
        "schtasks",
        "powershell",
        "pwsh",
        "cmd.exe",
        "cmd ",
        "rmdir /s",
        "del /f",
        "rd /s",
    ];
    for token in denied_tokens {
        if prog_name == token.trim() || (joined.contains(token) && matches_deny_program(&prog_name))
        {
            return CommandPolicyDecision::Deny {
                reason: format!("denied system/config command token '{token}'"),
            };
        }
    }

    // Explicit destructive git denials.
    if prog_name == "git" || prog_name == "git.exe" {
        let sub = command.get(1).map(|s| s.as_str()).unwrap_or("");
        let rest: Vec<&str> = command.iter().skip(1).map(|s| s.as_str()).collect();
        if sub == "push" && rest.iter().any(|a| *a == "--force" || *a == "-f") {
            return CommandPolicyDecision::Deny {
                reason: "force push denied".into(),
            };
        }
        // Non-force push still denied in default test policy (no publishing).
        if sub == "push" {
            return CommandPolicyDecision::Deny {
                reason: "git push / network publishing denied by default policy".into(),
            };
        }
        if sub == "reset" && rest.iter().any(|a| a.starts_with("--hard")) {
            return CommandPolicyDecision::Deny {
                reason: "destructive git reset --hard denied for test commands".into(),
            };
        }
        if sub == "clean" {
            return CommandPolicyDecision::Deny {
                reason: "git clean denied for test commands".into(),
            };
        }
    }

    // Working directory must exist (caller validates managed-root containment).
    if !working_directory.as_os_str().is_empty() && !working_directory.exists() {
        return CommandPolicyDecision::Deny {
            reason: format!(
                "working directory does not exist: {}",
                working_directory.display()
            ),
        };
    }

    let allowed_basenames = [
        "node",
        "node.exe",
        "npm",
        "npm.cmd",
        "npx",
        "npx.cmd",
        "cargo",
        "cargo.exe",
        "rustc",
        "rustc.exe",
        "python",
        "python.exe",
        "py",
        "py.exe",
        "git",
        "git.exe",
        "vitest",
        "vitest.cmd",
        "playwright",
        "playwright.cmd",
        "tsc",
        "tsc.cmd",
        "eslint",
        "eslint.cmd",
        "prettier",
        "prettier.cmd",
        "dotnet",
        "dotnet.exe",
        "go",
        "go.exe",
        "java",
        "java.exe",
        "mvn",
        "mvn.cmd",
        "gradle",
        "gradle.bat",
        "make",
        "make.exe",
        "cmake",
        "cmake.exe",
        "ctest",
        "ctest.exe",
        "pytest",
        "pytest.exe",
        "pnpm",
        "pnpm.cmd",
        "yarn",
        "yarn.cmd",
        "bun",
        "bun.exe",
    ];
    if allowed_basenames.iter().any(|p| prog_name == *p) {
        // npm/pnpm/yarn lifecycle scripts remain untrusted; still allowed under containment
        // but never imply network enablement.
        // Note: Cursor `--force` makes prompt/command policy advisory for Cursor-internal
        // tools (KNOWN-LIMITATIONS); this allow-list still gates architect/test argv.
        return CommandPolicyDecision::Allow;
    }

    // Local project tools under the working directory (managed tool root) only —
    // never accept arbitrary .exe/.cmd/.bat by extension alone.
    if is_managed_tool_candidate(prog, &prog_name, working_directory) {
        return CommandPolicyDecision::Allow;
    }

    CommandPolicyDecision::Deny {
        reason: format!("command '{prog_name}' not on default allow list"),
    }
}

/// Allow script/shim binaries only when resolved under the managed working directory.
fn is_managed_tool_candidate(prog: &str, prog_name: &str, working_directory: &Path) -> bool {
    // Unix project tooling is commonly extensionless (`./gradlew`, `bin/test`), which is
    // the direct analogue of the `.cmd`/`.exe` shims allowed on Windows. Both are only
    // ever accepted below, after the path is proven to sit inside the managed root.
    let extensionless = !prog_name.contains('.');
    let allowed_ext = prog_name.ends_with(".mjs")
        || prog_name.ends_with(".js")
        || prog_name.ends_with(".ts")
        || prog_name.ends_with(".cmd")
        || prog_name.ends_with(".bat")
        || prog_name.ends_with(".exe")
        || prog_name.ends_with(".sh")
        || extensionless;
    if !allowed_ext {
        return false;
    }
    if prog_name.ends_with(".ps1") {
        return false;
    }
    // Bare basename (no directory components) is not enough — must be path-scoped.
    let prog_path = Path::new(prog);
    if prog_path.components().count() <= 1 {
        return false;
    }
    if working_directory.as_os_str().is_empty() {
        return false;
    }
    let abs = if prog_path.is_absolute() {
        prog_path.to_path_buf()
    } else {
        working_directory.join(prog_path)
    };
    crate::intake::is_path_within_root(working_directory, &abs)
}

fn matches_deny_program(prog: &str) -> bool {
    matches!(
        prog,
        "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
            | "cmd"
            | "cmd.exe"
            | "format.com"
            | "diskpart.exe"
            | "reg.exe"
            | "netsh.exe"
            | "sc.exe"
            | "schtasks.exe"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn allows_node_npm_and_cargo() {
        let cwd = PathBuf::from(".");
        assert_eq!(
            evaluate_command_policy(&["node".into(), "test.mjs".into()], &cwd),
            CommandPolicyDecision::Allow
        );
        assert_eq!(
            evaluate_command_policy(&["npm".into(), "test".into()], &cwd),
            CommandPolicyDecision::Allow
        );
        assert_eq!(
            evaluate_command_policy(&["cargo".into(), "test".into()], &cwd),
            CommandPolicyDecision::Allow
        );
        // An absolute path to an allow-listed interpreter is judged on its basename.
        #[cfg(windows)]
        let absolute_node = r"C:\Program Files\nodejs\node.exe";
        #[cfg(not(windows))]
        let absolute_node = "/usr/local/bin/node";
        assert_eq!(
            evaluate_command_policy(&[absolute_node.into(), "-v".into()], &cwd),
            CommandPolicyDecision::Allow
        );
    }

    #[test]
    fn denies_arbitrary_exe_by_extension_alone() {
        let cwd = PathBuf::from(".");
        assert!(matches!(
            evaluate_command_policy(&[r"C:\temp\odd.exe".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            evaluate_command_policy(&["odd.exe".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            evaluate_command_policy(&["malware.cmd".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
    }

    #[test]
    fn allows_managed_tool_shim_under_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().to_path_buf();
        let shim = cwd
            .join("node_modules")
            .join(".bin")
            .join("custom-tool.cmd");
        std::fs::create_dir_all(shim.parent().unwrap()).unwrap();
        std::fs::write(&shim, "@echo off\n").unwrap();
        assert_eq!(
            evaluate_command_policy(&[shim.to_string_lossy().into()], &cwd),
            CommandPolicyDecision::Allow
        );
        // Outside managed working directory remains denied.
        #[cfg(windows)]
        let outside = r"C:\other\node_modules\.bin\custom-tool.cmd";
        #[cfg(not(windows))]
        let outside = "/other/node_modules/.bin/custom-tool.cmd";
        assert!(matches!(
            evaluate_command_policy(&[outside.into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
    }

    #[test]
    fn denies_force_push_shells_curl_and_secret_dump() {
        let cwd = PathBuf::from(".");
        assert!(matches!(
            evaluate_command_policy(&["git".into(), "push".into(), "--force".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            evaluate_command_policy(&["powershell".into(), "-Command".into(), "1".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            evaluate_command_policy(&["curl".into(), "https://evil".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            evaluate_command_policy(&["cmdkey".into(), "/list".into()], &cwd),
            CommandPolicyDecision::Deny { .. }
        ));
    }

    #[test]
    fn denies_cwd_outside_approved_roots() {
        let cwd = PathBuf::from(".");
        let decision = evaluate_command_policy_in_roots(
            &["node".into(), "t.mjs".into()],
            &cwd,
            Some(&["C:\\nonexistent-approved-root-tiamat".into()]),
        );
        assert!(matches!(decision, CommandPolicyDecision::Deny { .. }));
    }
}
