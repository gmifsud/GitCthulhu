use core_domain::{DomainError, Worktree};

/// Branch label for a worktree whose HEAD is not on a local branch.
const DETACHED_LABEL: &str = "(detached)";

/// Parse `git worktree list --porcelain` stdout into domain rows.
///
/// Records are separated by blank lines; the first one is the main worktree.
/// `prunable` entries (their directory is gone) are omitted.
pub(crate) fn parse_worktree_porcelain(stdout: &str) -> Vec<Worktree> {
    let mut rows = Vec::new();
    let mut record: Vec<&str> = Vec::new();
    let mut index = 0usize;
    for line in stdout.lines().map(|line| line.trim_end_matches('\r')) {
        if line.trim().is_empty() {
            if !record.is_empty() {
                rows.extend(parse_record(&record, index == 0));
                index += 1;
                record.clear();
            }
        } else {
            record.push(line);
        }
    }
    if !record.is_empty() {
        rows.extend(parse_record(&record, index == 0));
    }
    rows
}

fn parse_record(lines: &[&str], is_main: bool) -> Option<Worktree> {
    let mut path: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut locked = false;
    let mut prunable = false;
    for line in lines {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => path = Some(value.trim().to_string()),
            "branch" => {
                branch = Some(
                    value
                        .trim()
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value.trim())
                        .to_string(),
                );
            }
            "detached" => branch = None,
            "locked" => locked = true,
            "prunable" => prunable = true,
            _ => {}
        }
    }
    if prunable {
        return None;
    }
    Some(Worktree {
        path: path?,
        branch: branch
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| DETACHED_LABEL.to_string()),
        is_main,
        locked,
    })
}

/// Reject an empty path, a NUL byte, or a leading `-` before git sees it.
///
/// Unlike `require_safe_path`, absolute paths are allowed: linked worktrees live
/// outside the repository directory.
pub(crate) fn require_worktree_path(path: &str) -> Result<(), DomainError> {
    if path.trim().is_empty() || path.contains('\0') || path.starts_with('-') {
        Err(DomainError::Unknown(format!("invalid path: {path:?}")))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_lists_main_and_linked() {
        let raw = "\
worktree /repo
HEAD abcdef0123456789abcdef0123456789abcdef01
branch refs/heads/main

worktree /repo-feature
HEAD fedcba9876543210fedcba9876543210fedcba98
branch refs/heads/feature
locked
";
        let list = parse_worktree_porcelain(raw);
        assert_eq!(list.len(), 2);
        assert!(list[0].is_main);
        assert_eq!(list[0].branch, "main");
        assert_eq!(list[0].path.replace('\\', "/"), "/repo");
        assert!(!list[1].is_main);
        assert_eq!(list[1].branch, "feature");
        assert!(list[1].locked);
    }

    #[test]
    fn porcelain_handles_detached_locked_reason_prunable_and_crlf() {
        let raw = "worktree C:/repo\r\nHEAD aaaa\r\nbranch refs/heads/main\r\n\r\n\
worktree C:/detached\r\nHEAD bbbb\r\ndetached\r\nlocked in use\r\n\r\n\
worktree C:/gone\r\nHEAD cccc\r\nbranch refs/heads/gone\r\nprunable gitdir file points to non-existent location\r\n\r\n\
worktree C:/with space/linked\r\nHEAD dddd\r\nbranch refs/heads/topic/x\r\n";
        let list = parse_worktree_porcelain(raw);
        let paths: Vec<&str> = list.iter().map(|row| row.path.as_str()).collect();
        assert_eq!(paths, ["C:/repo", "C:/detached", "C:/with space/linked"]);
        assert_eq!(list[1].branch, "(detached)");
        assert!(list[1].locked);
        assert!(!list[1].is_main);
        assert_eq!(list[2].branch, "topic/x");
        assert!(!list[2].locked);
    }

    #[test]
    fn empty_output_is_no_worktrees() {
        assert!(parse_worktree_porcelain("").is_empty());
    }

    #[test]
    fn worktree_path_rejects_empty_nul_and_flags() {
        assert!(require_worktree_path("").is_err());
        assert!(require_worktree_path("-f").is_err());
        assert!(require_worktree_path("a\0b").is_err());
        assert!(require_worktree_path("C:/abs/path").is_ok());
        assert!(require_worktree_path("../sibling").is_ok());
    }
}
