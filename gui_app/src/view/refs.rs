use core_domain::Branch;

pub(super) fn current_branch_name(branches: &[Branch]) -> String {
    branches
        .iter()
        .find(|branch| branch.is_head && !branch.is_remote)
        .map(|branch| branch.name.clone())
        .unwrap_or_else(|| "Detached HEAD".to_string())
}
