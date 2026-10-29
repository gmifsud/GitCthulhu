//! Quick-open palette (Ctrl+P) and the history search predicate.
//!
//! Pure: builds an index from what a session already loaded and ranks it against a
//! query. No adapter calls, so ranking is deterministic and tested without a window.

use core_domain::{Branch, Commit, FileDisposition, RepoStatus, Stash, Tag, Worktree};

/// Most rows the palette draws. Ranking still considers every item.
pub(crate) const PALETTE_LIMIT: usize = 60;

/// What a palette row names. Order is the tie-break order between kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PaletteKind {
    Branch,
    Remote,
    Worktree,
    Tag,
    Stash,
    File,
    Commit,
    Repository,
}

impl PaletteKind {
    pub(crate) fn badge(self) -> &'static str {
        match self {
            Self::Branch => "branch",
            Self::Remote => "remote",
            Self::Worktree => "worktree",
            Self::Tag => "tag",
            Self::Stash => "stash",
            Self::File => "file",
            Self::Commit => "commit",
            Self::Repository => "repo",
        }
    }
}

/// What activating a row does. The window maps each to one existing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PaletteTarget {
    Branch {
        name: String,
        current: bool,
    },
    Remote {
        name: String,
    },
    Worktree {
        path: String,
    },
    Tag {
        name: String,
        commit_id: String,
    },
    Stash {
        index: usize,
        message: String,
    },
    File {
        path: String,
        place: FileDisposition,
    },
    Commit {
        id: String,
    },
    Repository {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaletteItem {
    pub kind: PaletteKind,
    /// Matched first and drawn bold.
    pub label: String,
    /// Matched as a fallback (hash, author, path) and drawn muted.
    pub detail: String,
    pub target: PaletteTarget,
}

/// Scope prefixes. `b:` local and remote branches, `r:` recent repositories, and so on.
const SCOPES: [(&str, &[PaletteKind]); 7] = [
    ("b:", &[PaletteKind::Branch, PaletteKind::Remote]),
    ("t:", &[PaletteKind::Tag]),
    ("s:", &[PaletteKind::Stash]),
    ("w:", &[PaletteKind::Worktree]),
    ("c:", &[PaletteKind::Commit]),
    ("f:", &[PaletteKind::File]),
    ("r:", &[PaletteKind::Repository]),
];

/// Footer hint listing the scope prefixes.
pub(crate) const SCOPE_HINT: &str =
    "b: branches · t: tags · s: stashes · w: worktrees · c: commits · f: files · r: repositories";

/// Prefix that opens the palette on the recent-repository list.
pub(crate) const REPOSITORY_SCOPE: &str = "r:";

/// Everything a session has loaded, plus the recent repositories.
pub(crate) struct PaletteSource<'a> {
    pub branches: &'a [Branch],
    pub tags: &'a [Tag],
    pub stashes: &'a [Stash],
    pub worktrees: &'a [Worktree],
    pub commits: &'a [Commit],
    pub status: Option<&'a RepoStatus>,
    pub recent_repositories: &'a [String],
    /// Path of the session on screen; left out of the repository list.
    pub current_path: &'a str,
}

pub(crate) fn palette_items(source: &PaletteSource<'_>) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for branch in source.branches {
        let short_tip: String = branch.tip.chars().take(8).collect();
        if branch.is_remote {
            items.push(PaletteItem {
                kind: PaletteKind::Remote,
                label: branch.name.clone(),
                detail: short_tip,
                target: PaletteTarget::Remote {
                    name: branch.name.clone(),
                },
            });
        } else {
            items.push(PaletteItem {
                kind: PaletteKind::Branch,
                label: branch.name.clone(),
                detail: if branch.is_head {
                    format!("current · {short_tip}")
                } else {
                    short_tip
                },
                target: PaletteTarget::Branch {
                    name: branch.name.clone(),
                    current: branch.is_head,
                },
            });
        }
    }
    for worktree in source.worktrees {
        items.push(PaletteItem {
            kind: PaletteKind::Worktree,
            label: if worktree.branch.is_empty() {
                worktree.path.clone()
            } else {
                worktree.branch.clone()
            },
            detail: worktree.path.clone(),
            target: PaletteTarget::Worktree {
                path: worktree.path.clone(),
            },
        });
    }
    for tag in source.tags {
        items.push(PaletteItem {
            kind: PaletteKind::Tag,
            label: tag.name.clone(),
            detail: tag.commit_id.chars().take(8).collect(),
            target: PaletteTarget::Tag {
                name: tag.name.clone(),
                commit_id: tag.commit_id.clone(),
            },
        });
    }
    for stash in source.stashes {
        items.push(PaletteItem {
            kind: PaletteKind::Stash,
            label: stash.message.clone(),
            detail: format!("stash@{{{}}}", stash.index),
            target: PaletteTarget::Stash {
                index: stash.index,
                message: stash.message.clone(),
            },
        });
    }
    if let Some(status) = source.status {
        let places = [
            (&status.staged_files, FileDisposition::Staged, "staged"),
            (
                &status.unstaged_files,
                FileDisposition::Unstaged,
                "unstaged",
            ),
            (
                &status.untracked_files,
                FileDisposition::Untracked,
                "untracked",
            ),
        ];
        for (paths, place, word) in places {
            for path in paths {
                items.push(PaletteItem {
                    kind: PaletteKind::File,
                    label: path.clone(),
                    detail: word.to_string(),
                    target: PaletteTarget::File {
                        path: path.clone(),
                        place,
                    },
                });
            }
        }
    }
    for commit in source.commits {
        let subject = commit
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let short: String = commit.id.chars().take(8).collect();
        items.push(PaletteItem {
            kind: PaletteKind::Commit,
            label: subject,
            detail: format!("{short} · {}", commit.author),
            target: PaletteTarget::Commit {
                id: commit.id.clone(),
            },
        });
    }
    for path in source.recent_repositories {
        if core_domain::same_repository_path(path, source.current_path) {
            continue;
        }
        items.push(PaletteItem {
            kind: PaletteKind::Repository,
            label: repository_name(path).to_string(),
            detail: path.clone(),
            target: PaletteTarget::Repository { path: path.clone() },
        });
    }
    items
}

/// Last path segment, or the whole path when it has none.
pub(crate) fn repository_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// Split a leading scope prefix off `query`.
fn scoped(query: &str) -> (Option<&'static [PaletteKind]>, &str) {
    let trimmed = query.trim_start();
    for (prefix, kinds) in SCOPES {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return (Some(kinds), rest.trim());
        }
    }
    (None, trimmed.trim())
}

/// Indices into `items`, best match first, at most [`PALETTE_LIMIT`].
///
/// Ties break on kind order, then original position, so the result is stable.
pub(crate) fn rank(items: &[PaletteItem], query: &str) -> Vec<usize> {
    let (kinds, needle) = scoped(query);
    let mut scored: Vec<(i64, PaletteKind, usize)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| kinds.is_none_or(|kinds| kinds.contains(&item.kind)))
        .filter_map(|(index, item)| {
            if needle.is_empty() {
                return Some((0, item.kind, index));
            }
            let label = fuzzy_score(&item.label, needle).map(|score| score * 2);
            let detail = fuzzy_score(&item.detail, needle);
            label.max(detail).map(|score| (score, item.kind, index))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    scored
        .into_iter()
        .take(PALETTE_LIMIT)
        .map(|(_, _, index)| index)
        .collect()
}

/// Case-insensitive subsequence score, or `None` when `needle` is not a subsequence.
/// Rewards consecutive runs, word starts (after `/ - _ . space` or a lower→upper step),
/// and an early start. Every position of the first character is tried, so `ext` finds
/// the `Ext` in `feature/…-ExternalServices` rather than stopping at `featur(e)`.
/// O(n·m) for haystack length n and needle length m.
pub(crate) fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    let original: Vec<char> = haystack.chars().collect();
    let hay: Vec<char> = haystack.chars().flat_map(char::to_lowercase).collect();
    // Lower-casing can change length (rare non-ASCII); then camel-case starts are skipped.
    let camel = original.len() == hay.len();
    let wanted: Vec<char> = needle
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !c.is_whitespace())
        .collect();
    let first = *wanted.first()?;
    (0..hay.len())
        .filter(|&start| hay[start] == first)
        .filter_map(|start| score_from(&hay, &original, camel, &wanted, start))
        .max()
}

fn is_word_start(hay: &[char], original: &[char], camel: bool, at: usize) -> bool {
    at == 0
        || matches!(hay[at - 1], '/' | '-' | '_' | '.' | ' ' | '\\')
        || (camel && original[at].is_uppercase() && original[at - 1].is_lowercase())
}

/// Greedy match of `wanted` starting with its first character at `start`.
fn score_from(
    hay: &[char],
    original: &[char],
    camel: bool,
    wanted: &[char],
    start: usize,
) -> Option<i64> {
    let mut score = 0i64;
    let mut cursor = start;
    let mut previous: Option<usize> = None;
    let mut first: Option<usize> = None;
    for &wanted in wanted {
        let found = (cursor..hay.len()).find(|&at| hay[at] == wanted)?;
        score += 1;
        if previous.is_some_and(|before| before + 1 == found) {
            score += 5;
        }
        if is_word_start(hay, original, camel, found) {
            score += 8;
        }
        first.get_or_insert(found);
        previous = Some(found);
        cursor = found + 1;
    }
    let lead = first.unwrap_or(0).min(20) as i64;
    Some(score * 4 - lead)
}

/// History search: a commit matches when every whitespace-separated term appears
/// (case-insensitive) in its message, author, id prefix, or one of its ref names.
pub(crate) fn commit_matches(commit: &Commit, ref_names: &[&str], query: &str) -> bool {
    let message = commit.message.to_lowercase();
    let author = commit.author.to_lowercase();
    let id = commit.id.to_lowercase();
    query.split_whitespace().all(|term| {
        let term = term.to_lowercase();
        message.contains(&term)
            || author.contains(&term)
            || id.starts_with(&term)
            || ref_names
                .iter()
                .any(|name| name.to_lowercase().contains(&term))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, remote: bool, head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote: remote,
            is_head: head,
            tip: "abcdef0123".to_string(),
            ahead: 0,
            behind: 0,
        }
    }

    fn commit(id: &str, message: &str, author: &str) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: Vec::new(),
            author: author.to_string(),
            message: message.to_string(),
            timestamp: 0,
        }
    }

    fn items() -> Vec<PaletteItem> {
        let branches = [
            branch("development", false, true),
            branch("feature/23542-Eyesel-SL-ExternalServices", false, false),
            branch(
                "origin/feature/23542-Eyesel-BuildOnboardingSetupScreen",
                true,
                false,
            ),
        ];
        let commits = [commit(
            "d9c2046aa",
            "chore(fork): clean entity module",
            "Gregory",
        )];
        let tags = [Tag {
            name: "v1.0".to_string(),
            commit_id: "d9c2046aa".to_string(),
        }];
        let recent = [
            r"D:\Repos\ISL\ISL-Backend".to_string(),
            r"D:\Repos\Backend".to_string(),
        ];
        palette_items(&PaletteSource {
            branches: &branches,
            tags: &tags,
            stashes: &[],
            worktrees: &[],
            commits: &commits,
            status: None,
            recent_repositories: &recent,
            current_path: r"D:\Repos\ISL\ISL-Backend",
        })
    }

    #[test]
    fn the_current_repository_is_not_offered_again() {
        let items = items();
        let repos: Vec<&str> = items
            .iter()
            .filter(|item| item.kind == PaletteKind::Repository)
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(repos, vec!["Backend"]);
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        let items = items();
        let ranked = rank(&items, "ext");
        assert_eq!(
            items[ranked[0]].label,
            "feature/23542-Eyesel-SL-ExternalServices"
        );
    }

    #[test]
    fn a_word_start_later_in_the_label_beats_an_early_scattered_match() {
        let branch = fuzzy_score("feature/23542-Eyesel-SL-ExternalServices", "ext").unwrap();
        let commit = fuzzy_score(
            "#24275-EYC-Sales Source Code config empty exception replaced",
            "ext",
        )
        .unwrap();
        assert!(branch > commit, "{branch} <= {commit}");
    }

    #[test]
    fn camel_case_humps_count_as_word_starts() {
        let hump = fuzzy_score("QiSLFriendlyException", "ex").unwrap();
        let flat = fuzzy_score("qislfriendlyexception", "ex").unwrap();
        assert!(hump > flat);
    }

    #[test]
    fn a_scope_prefix_limits_kinds() {
        let items = items();
        let ranked = rank(&items, "b: 23542");
        assert!(ranked
            .iter()
            .all(|&index| matches!(items[index].kind, PaletteKind::Branch | PaletteKind::Remote)));
        assert_eq!(ranked.len(), 2);
        let repos = rank(&items, "r:");
        assert_eq!(repos.len(), 1);
    }

    #[test]
    fn a_hash_matches_through_the_detail() {
        let items = items();
        let ranked = rank(&items, "d9c20");
        assert!(ranked
            .iter()
            .any(|&index| items[index].kind == PaletteKind::Commit));
    }

    #[test]
    fn an_empty_query_keeps_index_order() {
        let items = items();
        let ranked = rank(&items, "");
        let mut sorted = ranked.clone();
        sorted.sort_by_key(|&index| (items[index].kind, index));
        assert_eq!(ranked, sorted);
    }

    #[test]
    fn non_subsequences_do_not_match() {
        assert_eq!(fuzzy_score("development", "zz"), None);
        assert!(fuzzy_score("development", "dvl").is_some());
    }

    #[test]
    fn repository_name_is_the_last_segment() {
        assert_eq!(repository_name(r"D:\Repos\ISL\ISL-Backend\"), "ISL-Backend");
        assert_eq!(repository_name("/home/me/GitCthulhu"), "GitCthulhu");
    }

    #[test]
    fn history_search_needs_every_term() {
        let commit = commit(
            "d9c2046aa",
            "chore(fork): clean entity module",
            "Gregory Mifsud",
        );
        assert!(commit_matches(&commit, &[], "fork gregory"));
        assert!(commit_matches(&commit, &[], "D9C2"));
        assert!(commit_matches(&commit, &["development"], "devel"));
        assert!(!commit_matches(&commit, &[], "fork fatih"));
        assert!(
            !commit_matches(&commit, &[], "2046"),
            "hash matches by prefix only"
        );
    }
}
