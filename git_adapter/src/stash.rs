//! `git stash list` lines. The adapter owns the git command; this only reads its text.

use core_domain::{DomainError, Stash};

pub(crate) fn parse_stash_list(text: &str) -> Result<Vec<Stash>, DomainError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    text.lines().map(parse_stash_line).collect()
}

fn parse_stash_line(line: &str) -> Result<Stash, DomainError> {
    let line = line.trim_end_matches('\r');
    let rest = line.strip_prefix("stash@{").ok_or_else(|| unknown(line))?;
    let (index, message) = rest.split_once("}: ").ok_or_else(|| unknown(line))?;
    let index = index.parse().map_err(|_| unknown(line))?;
    Ok(Stash {
        index,
        message: message.to_string(),
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

    #[test]
    fn a_list_keeps_the_index_and_the_subject() {
        let stashes =
            parse_stash_list("stash@{0}: WIP on dev: 1558f59 subject\nstash@{1}: On main: wip\n")
                .unwrap();
        assert_eq!(
            stashes,
            vec![
                Stash {
                    index: 0,
                    message: "WIP on dev: 1558f59 subject".into(),
                },
                Stash {
                    index: 1,
                    message: "On main: wip".into(),
                },
            ]
        );
    }

    #[test]
    fn a_line_without_an_index_is_an_error() {
        assert!(parse_stash_list("not a stash").is_err());
    }
}
