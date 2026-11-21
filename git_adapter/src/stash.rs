//! `git stash list` lines. The adapter owns the git command; this only reads its text.

use core_domain::{DomainError, LostStash, Stash};

/// Unit separator between fields: cannot appear in a ref name, a hash, or a reflog subject.
const FIELD: char = '\u{1f}';

/// An explicit format, so the user's `log.*`/`color.*`/`format.*` config cannot change the
/// lines (the default human format broke parsing under some configs, and one bad line hid
/// the whole list).
pub(crate) const LIST_ARGS: [&str; 7] = [
    "-c",
    "color.ui=never",
    "-c",
    "log.showSignature=false",
    "stash",
    "list",
    "--format=%gd%x1f%H%x1f%gs",
];

pub(crate) fn parse_stash_list(text: &str) -> Result<Vec<Stash>, DomainError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_stash_line)
        .collect()
}

fn parse_stash_line(line: &str) -> Result<Stash, DomainError> {
    let line = line.trim_end_matches('\r');
    let mut fields = line.splitn(3, FIELD);
    let (Some(selector), Some(commit_id), Some(message)) = (fields.next(), fields.next(), fields.next())
    else {
        return Err(unknown(line));
    };
    let index = selector
        .strip_prefix("stash@{")
        .and_then(|rest| rest.strip_suffix('}'))
        .and_then(|index| index.parse().ok())
        .ok_or_else(|| unknown(line))?;
    if commit_id.len() < 40 || !commit_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unknown(line));
    }
    Ok(Stash {
        index,
        message: message.to_string(),
        commit_id: commit_id.to_string(),
    })
}

/// `git fsck` with reflogs ignored, so a commit only a reflog still names counts as lost too.
pub(crate) const FSCK_ARGS: [&str; 5] = ["fsck", "--no-reflogs", "--no-progress", "--unreachable", "--no-dangling"];

/// Reads every candidate in one call, ids on stdin. One line per commit: id, parents,
/// subject, committer time, split by NUL (a subject is one line, so no field holds a newline).
pub(crate) const CANDIDATE_ARGS: [&str; 6] = [
    "log",
    "--no-walk=unsorted",
    "--stdin",
    "--no-show-signature",
    "--format=%H%x00%P%x00%s%x00%ct",
    "--",
];

/// The ids on `unreachable commit <sha>` lines. Trees, blobs and tags are not stashes.
pub(crate) fn unreachable_commit_ids(fsck_output: &str) -> Vec<&str> {
    fsck_output
        .lines()
        .filter_map(|line| line.trim_end().strip_prefix("unreachable commit "))
        .map(str::trim)
        .filter(|id| id.len() >= 40 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .collect()
}

/// A stash commit has the HEAD it was taken on and the index as parents, and git words its
/// subject `WIP on <branch>: …` or, with a message, `On <branch>: …`. Anything else that
/// went unreachable (an amended commit, an abandoned branch) is not one.
pub(crate) fn parse_candidate(line: &str) -> Option<LostStash> {
    let mut fields = line.trim_end().splitn(4, '\0');
    let (commit_id, parents, subject, time) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?);
    let looks_like_stash = parents.split_whitespace().count() >= 2
        && (subject.starts_with("WIP on ") || subject.starts_with("On "));
    looks_like_stash.then(|| LostStash {
        commit_id: commit_id.to_string(),
        message: subject.to_string(),
        timestamp: time.trim().parse().unwrap_or(0),
    })
}

/// The stashes among the lines [`CANDIDATE_ARGS`] printed, newest first.
pub(crate) fn parse_candidates(log_text: &str) -> Vec<LostStash> {
    let mut lost: Vec<LostStash> = log_text.lines().filter_map(parse_candidate).collect();
    lost.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.commit_id.cmp(&b.commit_id)));
    lost
}

fn unknown(line: &str) -> DomainError {
    DomainError::Unknown(format!("unrecognised stash list line: {line}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_list_has_no_stashes() {
        assert!(parse_stash_list("").unwrap().is_empty());
    }

    const A: &str = "1558f59aa1558f59aa1558f59aa1558f59aa1558";
    const B: &str = "2668f59aa1558f59aa1558f59aa1558f59aa1559";

    #[test]
    fn a_list_keeps_the_index_the_commit_and_the_subject() {
        let text = format!(
            "stash@{{0}}\u{1f}{A}\u{1f}WIP on dev: 1558f59 subject: with colon\nstash@{{1}}\u{1f}{B}\u{1f}On main: wip\n"
        );
        assert_eq!(
            parse_stash_list(&text).unwrap(),
            vec![
                Stash {
                    index: 0,
                    message: "WIP on dev: 1558f59 subject: with colon".into(),
                    commit_id: A.into(),
                },
                Stash {
                    index: 1,
                    message: "On main: wip".into(),
                    commit_id: B.into(),
                },
            ]
        );
    }

    #[test]
    fn crlf_and_blank_lines_are_tolerated() {
        let text = format!("stash@{{0}}\u{1f}{A}\u{1f}On main: wip\r\n\n");
        assert_eq!(parse_stash_list(&text).unwrap().len(), 1);
    }

    #[test]
    fn a_line_without_an_index_is_an_error() {
        assert!(parse_stash_list("not a stash").is_err());
    }

    #[test]
    fn only_unreachable_commit_lines_name_candidates() {
        let text = format!(
            "unreachable blob {A}\nunreachable commit {B}\nunreachable tree {A}\nunreachable commit nothex\n"
        );
        assert_eq!(unreachable_commit_ids(&text), vec![B]);
    }

    #[test]
    fn a_candidate_needs_two_parents_and_a_stash_subject() {
        let stash = parse_candidate("A\0p1 p2\0WIP on main: abc subject\x001700000000\n").unwrap();
        assert_eq!(stash.commit_id, "A");
        assert_eq!(stash.message, "WIP on main: abc subject");
        assert_eq!(stash.timestamp, 1_700_000_000);
        assert!(parse_candidate("A\0p1 p2\0On main: named\x001\n").is_some());
        assert!(parse_candidate("A\0p1\0WIP on main: x\x001\n").is_none());
        assert!(parse_candidate("A\0p1 p2\0Merge branch 'x'\x001\n").is_none());
        assert!(parse_candidate("garbage").is_none());
    }

    #[test]
    fn candidates_are_filtered_and_sorted_newest_first() {
        let text = "A\0p1 p2\0On main: old\x00100\nB\0p1\0amended\x00300\nC\0p1 p2\0WIP on main: new\x00200\n";
        let ids: Vec<String> = parse_candidates(text).into_iter().map(|stash| stash.commit_id).collect();
        assert_eq!(ids, vec!["C", "A"]);
    }
}

