//! What a pasted clone address means: which service it is on, the URL git should clone, the
//! folder name to suggest, and the branch it points at when it names one. People paste what
//! they have: the browser's address bar, the "Clone" box, or a whole `git clone …` command,
//! so all of those are read.

/// Where a repository lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    GitHub,
    AzureDevOps,
    /// Any other git URL or a folder on this machine.
    Other,
}

impl Provider {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::AzureDevOps => "Azure DevOps",
            Self::Other => "Git",
        }
    }
}

/// A clone address, understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloneSource {
    pub provider: Provider,
    /// What git is given.
    pub url: String,
    /// The repository's own name: the folder to suggest.
    pub name: String,
    /// "owner / repo", "org / project / repo": what the dialog shows it found.
    pub path: String,
    /// The branch a browser address pointed at (`…/tree/<branch>`, `?version=GB<branch>`).
    pub branch: Option<String>,
}

/// Read a pasted address. `None` when there is nothing to clone in it.
pub(crate) fn parse(input: &str) -> Option<CloneSource> {
    let mut text = input.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    // A whole command pasted from a "Clone" box: keep only the address.
    if let Some(rest) = text.strip_prefix("git clone ") {
        text = rest.split_whitespace().find(|word| !word.starts_with('-'))?;
    }
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    // An address on a service it knows must have that service's shape: a GitHub page that
    // names no repository is not something to clone.
    let lower = text.to_ascii_lowercase();
    let known = ["github.com", "dev.azure.com", "visualstudio.com"].iter().any(|host| lower.contains(host));
    let found = github(text).or_else(|| azure_devops(text));
    if known { found } else { found.or_else(|| other(text)) }
}

fn strip_git(name: &str) -> &str {
    name.strip_suffix(".git").unwrap_or(name)
}

/// Everything before `?` or `#`, and the query on its own.
fn split_query(text: &str) -> (&str, Option<&str>) {
    let end = text.find(['?', '#']).unwrap_or(text.len());
    let query = text[end..].strip_prefix('?').map(|rest| rest.split('#').next().unwrap_or(rest));
    (&text[..end], query)
}

fn github(text: &str) -> Option<CloneSource> {
    let (address, _) = split_query(text);
    let rest = address
        .strip_prefix("git@github.com:")
        .or_else(|| address.strip_prefix("ssh://git@github.com/"))
        .or_else(|| address.strip_prefix("https://github.com/"))
        .or_else(|| address.strip_prefix("http://github.com/"))
        .or_else(|| address.strip_prefix("github.com/"))?;
    let ssh = address.starts_with("git@") || address.starts_with("ssh://");
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = strip_git(parts.next()?);
    if repo.is_empty() {
        return None;
    }
    // A browser address on a branch: …/tree/<branch> (a branch may itself hold slashes).
    let after: Vec<&str> = parts.collect();
    let branch = match after.as_slice() {
        ["tree", branch @ ..] if !branch.is_empty() => Some(branch.join("/")),
        _ => None,
    };
    let url = if ssh { format!("git@github.com:{owner}/{repo}.git") } else { format!("https://github.com/{owner}/{repo}.git") };
    Some(CloneSource { provider: Provider::GitHub, url, name: repo.to_string(), path: format!("{owner} / {repo}"), branch })
}

fn azure_devops(text: &str) -> Option<CloneSource> {
    let (address, query) = split_query(text);
    // ?version=GBfeature/x is the branch a browser address is on.
    let branch = query.and_then(|query| {
        query.split('&').find_map(|pair| pair.strip_prefix("version=GB")).map(|branch| branch.replace("%2F", "/"))
    });
    // SSH: git@ssh.dev.azure.com:v3/org/project/repo, org@vs-ssh.visualstudio.com:v3/org/project/repo
    if let Some((_, rest)) = address.split_once("ssh.dev.azure.com:v3/").or_else(|| address.split_once("vs-ssh.visualstudio.com:v3/")) {
        let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
        let [org, project, repo] = parts.as_slice() else {
            return None;
        };
        return Some(CloneSource {
            provider: Provider::AzureDevOps,
            url: address.to_string(),
            name: repo.to_string(),
            path: format!("{org} / {project} / {repo}"),
            branch,
        });
    }
    // HTTPS: https://[user@]dev.azure.com/org/project/_git/repo, https://org.visualstudio.com/[collection/]project/_git/repo
    let lower = address.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    let host_start = address.find("://")? + 3;
    let host_end = address[host_start..].find('/').map(|at| host_start + at)?;
    let host = address[host_start..host_end].rsplit('@').next()?.to_ascii_lowercase();
    let segments: Vec<&str> = address[host_end..].split('/').filter(|part| !part.is_empty()).collect();
    let git = segments.iter().position(|part| *part == "_git")?;
    let repo = *segments.get(git + 1)?;
    let (org, project) = if host == "dev.azure.com" {
        (*segments.first()?, *segments.get(git.checked_sub(1)?)?)
    } else if let Some(org) = host.strip_suffix(".visualstudio.com") {
        (org, *segments.get(git.checked_sub(1)?)?)
    } else {
        return None;
    };
    // Up to the repository: anything after it (…/_git/repo/pullrequest/12) is a page, not git.
    let end = address[host_end..]
        .match_indices(repo)
        .map(|(at, _)| host_end + at + repo.len())
        .find(|&end| address[..end].ends_with(&format!("_git/{repo}")))?;
    Some(CloneSource {
        provider: Provider::AzureDevOps,
        url: address[..end].to_string(),
        name: strip_git(repo).to_string(),
        path: format!("{org} / {project} / {repo}"),
        branch,
    })
}

fn other(text: &str) -> Option<CloneSource> {
    let trimmed = text.trim_end_matches(['/', '\\']);
    let name = strip_git(trimmed.rsplit(['/', '\\', ':']).next()?.trim());
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(CloneSource {
        provider: Provider::Other,
        url: text.to_string(),
        name: name.to_string(),
        path: trimmed.to_string(),
        branch: None,
    })
}

/// What a clone's progress line says: the stage ("Receiving objects") and how far it is, in
/// per cent, when git gives one. git's server-side lines carry "remote: ".
pub(crate) fn progress_of(line: &str) -> (String, Option<u8>) {
    let line = line.trim().trim_start_matches("remote:").trim();
    match line.split_once(':') {
        Some((stage, rest)) if rest.contains('%') => {
            let percent = rest.split('%').next().and_then(|before| before.trim().rsplit(' ').next()).and_then(|n| n.parse().ok());
            (stage.trim().to_string(), percent)
        }
        _ => (line.trim_end_matches("...").trim_end_matches('.').to_string(), None),
    }
}

/// A hint to add under a failed clone's message, when git's words point at signing in.
pub(crate) fn sign_in_hint(message: &str, provider: Provider) -> Option<&'static str> {
    let lower = message.to_ascii_lowercase();
    let signing_in = ["authentication failed", "could not read username", "terminal prompts disabled", "403", "401", "permission denied (publickey)", "access denied"]
        .iter()
        .any(|sign| lower.contains(sign));
    if !signing_in {
        return None;
    }
    Some(match provider {
        Provider::GitHub => "git could not sign in to GitHub. Git Credential Manager should open a sign-in window; if none appeared, run `git credential-manager github login` in a terminal, then try again. For an SSH address, make sure your key is loaded.",
        Provider::AzureDevOps => "git could not sign in to Azure DevOps. Git Credential Manager should open a sign-in window; if none appeared, check that you can open the repository in the browser with the same account, then try again. For an SSH address, make sure your key is added to Azure DevOps.",
        Provider::Other => "git could not sign in. Check the address, and that git has a credential helper or an SSH key for this server.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn got(input: &str) -> CloneSource {
        parse(input).unwrap_or_else(|| panic!("nothing understood in {input:?}"))
    }

    #[test]
    fn github_addresses_from_the_browser_the_clone_box_and_a_command() {
        for input in [
            "https://github.com/gmifsud/GitCthulhu",
            "https://github.com/gmifsud/GitCthulhu.git",
            "https://github.com/gmifsud/GitCthulhu/",
            "github.com/gmifsud/GitCthulhu",
            "git clone https://github.com/gmifsud/GitCthulhu.git",
            "  \"https://github.com/gmifsud/GitCthulhu?tab=readme\"  ",
            "https://github.com/gmifsud/GitCthulhu/pulls",
        ] {
            let source = got(input);
            assert_eq!(source.provider, Provider::GitHub, "{input}");
            assert_eq!(source.url, "https://github.com/gmifsud/GitCthulhu.git", "{input}");
            assert_eq!(source.name, "GitCthulhu");
            assert_eq!(source.path, "gmifsud / GitCthulhu");
            assert_eq!(source.branch, None, "{input}");
        }
        let ssh = got("git@github.com:gmifsud/GitCthulhu.git");
        assert_eq!(ssh.url, "git@github.com:gmifsud/GitCthulhu.git");
        let on_branch = got("https://github.com/gmifsud/GitCthulhu/tree/feature/clone-dialog");
        assert_eq!(on_branch.branch.as_deref(), Some("feature/clone-dialog"));
        assert_eq!(on_branch.url, "https://github.com/gmifsud/GitCthulhu.git");
    }

    #[test]
    fn azure_devops_addresses_in_both_forms_and_over_ssh() {
        let modern = got("https://contoso@dev.azure.com/contoso/Platform/_git/Backend");
        assert_eq!(modern.provider, Provider::AzureDevOps);
        assert_eq!(modern.url, "https://contoso@dev.azure.com/contoso/Platform/_git/Backend");
        assert_eq!(modern.name, "Backend");
        assert_eq!(modern.path, "contoso / Platform / Backend");

        let browser = got("https://dev.azure.com/contoso/Platform/_git/Backend?path=/src&version=GBfeature%2Fx&_a=contents");
        assert_eq!(browser.url, "https://dev.azure.com/contoso/Platform/_git/Backend");
        assert_eq!(browser.branch.as_deref(), Some("feature/x"));

        let page = got("https://dev.azure.com/contoso/Platform/_git/Backend/pullrequest/42");
        assert_eq!(page.url, "https://dev.azure.com/contoso/Platform/_git/Backend", "a page under the repository");

        let legacy = got("https://contoso.visualstudio.com/DefaultCollection/Platform/_git/Backend");
        assert_eq!(legacy.provider, Provider::AzureDevOps);
        assert_eq!(legacy.path, "contoso / Platform / Backend");

        let ssh = got("git@ssh.dev.azure.com:v3/contoso/Platform/Backend");
        assert_eq!((ssh.provider, ssh.name.as_str()), (Provider::AzureDevOps, "Backend"));
        assert_eq!(ssh.url, "git@ssh.dev.azure.com:v3/contoso/Platform/Backend");
    }

    #[test]
    fn any_other_address_or_folder_is_cloned_as_given() {
        let other = got("https://gitlab.com/group/sub/tool.git");
        assert_eq!((other.provider, other.name.as_str()), (Provider::Other, "tool"));
        assert_eq!(other.url, "https://gitlab.com/group/sub/tool.git");
        assert_eq!(got(r"D:\Repos\Backend").name, "Backend");
        assert_eq!(parse("   "), None);
        assert_eq!(parse("two words"), None);
        assert_eq!(parse("https://github.com/only-owner"), None);
    }

    #[test]
    fn progress_lines_give_their_stage_and_percentage() {
        assert_eq!(progress_of("Receiving objects:  45% (450/1000), 1.20 MiB | 2.30 MiB/s"), ("Receiving objects".into(), Some(45)));
        assert_eq!(progress_of("remote: Counting objects: 100% (12/12), done."), ("Counting objects".into(), Some(100)));
        assert_eq!(progress_of("Cloning into 'Backend'..."), ("Cloning into 'Backend'".into(), None));
        assert_eq!(progress_of("Updating files:   7% (812/11227)"), ("Updating files".into(), Some(7)));
    }

    #[test]
    fn a_sign_in_failure_gets_a_hint_for_its_service_and_others_do_not() {
        let failed = "fatal: Authentication failed for 'https://dev.azure.com/contoso/Platform/_git/Backend/'";
        assert!(sign_in_hint(failed, Provider::AzureDevOps).is_some_and(|hint| hint.contains("Azure DevOps")));
        assert!(sign_in_hint("fatal: could not read Username for 'https://github.com': terminal prompts disabled", Provider::GitHub).is_some());
        assert_eq!(sign_in_hint("fatal: repository not found", Provider::GitHub), None);
    }
}
