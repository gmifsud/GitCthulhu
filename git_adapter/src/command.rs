use core_domain::DomainError;

/// The git executable, used only for porcelain operations gix 0.66 can't perform safely.
const GIT_EXECUTABLE: &str = "git";

pub(crate) const NO_PATHS: &str = "no paths to update";
pub(crate) const INVALID_PATH: &str = "a path is empty or contains a null byte";
pub(crate) const BLANK_COMMIT_MESSAGE: &str = "commit message is blank";

/// Run `git -C <repo_path> <args>`, returning git's own error text on failure.
pub(crate) fn run_git(repo_path: &str, args: &[&str]) -> Result<(), DomainError> {
    let output = git_command(repo_path, args).output().map_err(|error| {
        DomainError::Unknown(format!("could not run {GIT_EXECUTABLE}: {error}"))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(DomainError::Unknown(git_failure_text(&output)))
    }
}

/// Like [`run_git`], returning trimmed stdout when git exits successfully.
pub(crate) fn run_git_stdout(repo_path: &str, args: &[&str]) -> Result<String, DomainError> {
    let output = git_command(repo_path, args).output().map_err(|error| {
        DomainError::Unknown(format!("could not run {GIT_EXECUTABLE}: {error}"))
    })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(DomainError::Unknown(git_failure_text(&output)))
    }
}

fn git_program(repo_path: &str, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(GIT_EXECUTABLE);
    command
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn git_command(repo_path: &str, args: &[&str]) -> std::process::Command {
    let mut command = git_program(repo_path, args);
    command.stdin(std::process::Stdio::null());
    command
}

/// Run git with `stdin` as the patch body. Used by `git apply`.
pub(crate) fn run_git_stdin(
    repo_path: &str,
    args: &[&str],
    stdin: &[u8],
) -> Result<(), DomainError> {
    use std::io::Write;
    let mut child = git_program(repo_path, args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            DomainError::Unknown(format!("could not run {GIT_EXECUTABLE}: {error}"))
        })?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin).map_err(|error| {
            DomainError::Unknown(format!("could not write to {GIT_EXECUTABLE}: {error}"))
        })?;
    }
    let output = child.wait_with_output().map_err(|error| {
        DomainError::Unknown(format!("could not run {GIT_EXECUTABLE}: {error}"))
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(DomainError::Unknown(git_failure_text(&output)))
    }
}

/// `git commit` with a clean index writes its reason to stdout and leaves stderr empty.
fn git_failure_text(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}

/// Reject an empty list, an empty path, or a path containing NUL before git sees it.
fn validate_paths(paths: &[String]) -> Result<(), DomainError> {
    if paths.is_empty() {
        return Err(DomainError::Unknown(NO_PATHS.to_string()));
    }
    if paths
        .iter()
        .any(|path| path.is_empty() || path.contains('\0'))
    {
        return Err(DomainError::Unknown(INVALID_PATH.to_string()));
    }
    Ok(())
}

/// `git -C <repo> <prefix> -- <paths>`. The `--` stops a path from becoming a flag.
pub(crate) fn run_git_paths(
    repo_path: &str,
    prefix: &[&str],
    paths: &[String],
) -> Result<(), DomainError> {
    validate_paths(paths)?;
    let mut args = Vec::with_capacity(prefix.len() + 1 + paths.len());
    args.extend_from_slice(prefix);
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    run_git(repo_path, &args)
}
