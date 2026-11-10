//! One colour per worktree, the same wherever it is drawn: its tab, its row in the Explorer, its
//! branch in the branch tree, its pill and working changes in the history, and the graph line
//! from its tip down.
//!
//! The colour comes from the worktree's path, never from a lane. Lanes are reused by whichever
//! branch needs one next, so a lane colour changed under a worktree as history streamed in or a
//! filter was set. A path's colour is its preferred slot in [`WORKTREE_COLOURS`] (a fixed hash
//! of the path), and worktrees that would share a slot take the next free one, in path order,
//! so the first ten worktrees of a repository always read apart. Deterministic: the same
//! worktrees give the same colours on every run and machine.

use core_domain::{repository_key, Worktree};
use iced::Color;

use crate::theme::hex_color;

/// The worktree palette. Exported to the design system as `worktree-0` … `worktree-9`.
/// The accent orange is left out: it means "current" and "active" elsewhere.
pub(crate) const WORKTREE_COLOURS: [u32; 10] = [
    0x3B82F6, 0x22C55E, 0xA855F7, 0xEC4899, 0x14B8A6, 0xEAB308, 0xF43F5E, 0x38BDF8, 0x84CC16, 0x818CF8,
];

const SLOTS: usize = WORKTREE_COLOURS.len();

/// FNV-1a, 64-bit: fixed, so a path hashes the same on every build (unlike `DefaultHasher`,
/// whose algorithm the standard library may change).
fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The slot a folder would take on its own.
fn preferred_slot(key: &str) -> usize {
    (fnv1a(key.bytes()) % SLOTS as u64) as usize
}

/// The colour of every worktree of the repository on screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WorktreeInks {
    /// `(repository_key, slot)`, sorted by key.
    slots: Vec<(String, usize)>,
}

impl WorktreeInks {
    /// Colours for the worktrees at `paths` (each folder once, whatever its spelling).
    pub(crate) fn for_paths<'p>(paths: impl IntoIterator<Item = &'p str>) -> Self {
        let mut keys: Vec<String> = paths.into_iter().map(repository_key).collect();
        keys.sort();
        keys.dedup();
        let mut taken = [false; SLOTS];
        let mut used = 0;
        let mut slots = Vec::with_capacity(keys.len());
        for key in keys {
            // Past ten worktrees colours must repeat: start a fresh round of the palette.
            if used == SLOTS {
                taken = [false; SLOTS];
                used = 0;
            }
            let mut slot = preferred_slot(&key);
            while taken[slot] {
                slot = (slot + 1) % SLOTS;
            }
            taken[slot] = true;
            used += 1;
            slots.push((key, slot));
        }
        Self { slots }
    }

    /// Colours for the repository's worktrees as `git worktree list` gave them.
    pub(crate) fn for_catalogue(catalogue: &[Worktree]) -> Self {
        Self::for_paths(catalogue.iter().map(|entry| entry.path.as_str()))
    }

    /// The colour of the worktree at `path`. A folder not in the catalogue (its tab opened
    /// before the catalogue arrived) gets its preferred colour.
    pub(crate) fn colour(&self, path: &str) -> Color {
        let key = repository_key(path);
        let slot = self
            .slots
            .binary_search_by(|(known, _)| known.as_str().cmp(key.as_str()))
            .map(|at| self.slots[at].1)
            .unwrap_or_else(|_| preferred_slot(&key));
        hex_color(WORKTREE_COLOURS[slot])
    }

    /// The colour of the worktree that has `branch` checked out, if one does.
    pub(crate) fn colour_of_branch(&self, catalogue: &[Worktree], branch: &str) -> Option<Color> {
        catalogue
            .iter()
            .find(|entry| !entry.branch.is_empty() && entry.branch == branch)
            .map(|entry| self.colour(&entry.path))
    }
}

/// What the history's colours depend on in the catalogue: which folders, and which branch each
/// has checked out. A change in either means the row index rebuilds.
pub(crate) fn catalogue_fingerprint(catalogue: &[Worktree]) -> u64 {
    fnv1a(catalogue.iter().flat_map(|entry| {
        repository_key(&entry.path)
            .into_bytes()
            .into_iter()
            .chain([0])
            .chain(entry.branch.clone().into_bytes())
            .chain([0])
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worktree(path: &str, branch: &str) -> Worktree {
        Worktree { path: path.into(), branch: branch.into(), is_main: false, locked: false }
    }

    #[test]
    fn a_worktree_keeps_its_colour_whatever_the_spelling_of_its_path() {
        let inks = WorktreeInks::for_paths(["D:/Repos/Backend", "D:/Repos/wt/feature"]);
        assert_eq!(inks.colour("D:/Repos/wt/feature"), inks.colour(r"D:\Repos\wt\feature\"));
    }

    #[test]
    fn the_first_ten_worktrees_never_share_a_colour() {
        let paths: Vec<String> = (0..SLOTS).map(|index| format!("D:/Repos/wt/topic-{index}")).collect();
        let inks = WorktreeInks::for_paths(paths.iter().map(String::as_str));
        let mut colours: Vec<String> = paths.iter().map(|path| format!("{:?}", inks.colour(path))).collect();
        colours.sort();
        colours.dedup();
        assert_eq!(colours.len(), SLOTS);
    }

    #[test]
    fn the_same_worktrees_give_the_same_colours_in_any_order() {
        let one = WorktreeInks::for_paths(["/r/a", "/r/b", "/r/c"]);
        let other = WorktreeInks::for_paths(["/r/c", "/r/a", "/r/b", "/r/a/"]);
        assert_eq!(one, other);
    }

    #[test]
    fn an_unknown_folder_still_gets_its_own_preferred_colour() {
        let inks = WorktreeInks::default();
        let key = repository_key("/r/elsewhere");
        assert_eq!(inks.colour("/r/elsewhere"), hex_color(WORKTREE_COLOURS[preferred_slot(&key)]));
    }

    #[test]
    fn a_branch_takes_the_colour_of_the_worktree_it_is_checked_out_in() {
        let catalogue = vec![worktree("/r/main", "main"), worktree("/r/wt", "feature/x"), worktree("/r/detached", "")];
        let inks = WorktreeInks::for_catalogue(&catalogue);
        assert_eq!(inks.colour_of_branch(&catalogue, "feature/x"), Some(inks.colour("/r/wt")));
        assert_eq!(inks.colour_of_branch(&catalogue, "other"), None);
        assert_eq!(inks.colour_of_branch(&catalogue, ""), None, "a detached worktree has no branch");
    }

    #[test]
    fn the_fingerprint_changes_with_a_folder_or_a_checked_out_branch() {
        let base = vec![worktree("/r/a", "main"), worktree("/r/b", "x")];
        let moved_branch = vec![worktree("/r/a", "main"), worktree("/r/b", "y")];
        let other_folder = vec![worktree("/r/a", "main"), worktree("/r/c", "x")];
        assert_ne!(catalogue_fingerprint(&base), catalogue_fingerprint(&moved_branch));
        assert_ne!(catalogue_fingerprint(&base), catalogue_fingerprint(&other_folder));
        assert_eq!(catalogue_fingerprint(&base), catalogue_fingerprint(&base.clone()));
    }
}
