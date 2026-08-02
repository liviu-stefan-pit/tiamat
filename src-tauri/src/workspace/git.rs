use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::workspace::error::{WorkspaceError, WorkspaceResult};

pub fn git(cwd: &Path, args: &[&str]) -> WorkspaceResult<Output> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Tiamat")
        .env("GIT_AUTHOR_EMAIL", "tiamat@example.com")
        .env("GIT_COMMITTER_NAME", "Tiamat")
        .env("GIT_COMMITTER_EMAIL", "tiamat@example.com")
        .stdin(Stdio::null())
        .output()
        .map_err(|err| WorkspaceError::Git {
            command: args.join(" "),
            detail: err.to_string(),
        })?;
    if !output.status.success() {
        return Err(WorkspaceError::Git {
            command: args.join(" "),
            detail: format!(
                "stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(output)
}

pub fn git_text(cwd: &Path, args: &[&str]) -> WorkspaceResult<String> {
    let output = git(cwd, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn configure_identity(repo: &Path) -> WorkspaceResult<()> {
    let _ = git(repo, &["config", "user.name", "Tiamat"]);
    let _ = git(repo, &["config", "user.email", "tiamat@example.com"]);
    Ok(())
}
