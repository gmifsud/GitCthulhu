//! `git stash list` lines. The adapter owns the git command; this only reads its text.

use core_domain::{DomainError, Stash};

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
}
