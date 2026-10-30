use std::future::Future;
use std::sync::Arc;
use std::thread;

use core_domain::{
    DomainError, FileDisposition, GitRepository, Hunk, Patch, ResetMode, ViewEvent, Worktree,
};
use iced::futures::channel::oneshot;

/// A repository adapter that can be shared with background work.
pub(crate) type SharedRepository = Arc<dyn GitRepository + Send + Sync>;

/// A diff the patch pane asked to read.
#[derive(Debug, Clone)]
pub(crate) enum PatchQuery {
    Commit(String),
    File {
        path: String,
        place: FileDisposition,
    },
}

/// A stage, unstage, or commit requested by the ribbon.
#[derive(Debug, Clone)]
pub(crate) enum WorktreeEdit {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    /// Stop tracking these paths (files stay on disk), then append `ignore` to `.gitignore`
    /// when it is not empty, so they stay out of the next `git add .`.
    Untrack { paths: Vec<String>, ignore: Vec<String> },
    Commit(String),
}

/// Number of commits requested from the repository and rendered in the graph pane.
pub(crate) const COMMIT_PAGE_SIZE: usize = 50;

/// Read the repository through the port and translate each result into a [`ViewEvent`].
///
/// Synchronous by design: the caller chooses where it runs.
/// Each call is reported independently, so one failure never hides another result.
pub(crate) fn load_repository(repo: &dyn GitRepository) -> Vec<ViewEvent> {
    vec![
        match repo.status() {
            Ok(status) => ViewEvent::StatusUpdated(status),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.log(Some(COMMIT_PAGE_SIZE)) {
            Ok(commits) => ViewEvent::LogUpdated(commits),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.branches() {
            Ok(branches) => ViewEvent::BranchesUpdated(branches),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.stashes() {
            Ok(stashes) => ViewEvent::StashesUpdated(stashes),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
        match repo.tags() {
            Ok(tags) => ViewEvent::TagsUpdated(tags),
            Err(error) => ViewEvent::ErrorOccurred(error),
        },
    ]
}

/// Run blocking Git work on a dedicated thread so it never stalls the UI or iced's
/// executor. If the thread dies before finishing, `interrupted` supplies the result.
fn in_background<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
    interrupted: impl FnOnce() -> T,
) -> impl Future<Output = T> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || {
        let _ = sender.send(job());
    });
    async move { receiver.await.unwrap_or_else(|_| interrupted()) }
}

/// What a running shell command reports, in order: `Started` once, `Line` for each line as it
/// is printed, then `Finished` once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellEvent {
    /// The process id, so Stop / Ctrl+C can end it.
    Started(u32),
    Line { error: bool, text: String },
    /// `cwd` is the folder the command left the shell in (after `cd`), when it got that far.
    Finished { cwd: Option<String>, code: Option<i32> },
}

/// Printed by the wrapper after the command so the next command starts where this one ended.
const CWD_MARK: &str = "\u{1e}GITCTHULHU_CWD\u{1e}";

/// Run `command` in `cwd` through the platform shell, streaming its output line by line.
/// Each command is its own process; only the folder carries over, like `cd` in a real shell.
pub(crate) fn shell_stream(
    cwd: String,
    command: String,
) -> impl iced::futures::Stream<Item = ShellEvent> {
    let (sender, receiver) = iced::futures::channel::mpsc::unbounded();
    thread::spawn(move || run_shell(&cwd, &command, sender));
    receiver
}

fn run_shell(
    cwd: &str,
    command: &str,
    sender: iced::futures::channel::mpsc::UnboundedSender<ShellEvent>,
) {
    use std::io::BufRead;
    use std::process::Stdio;

    let send = |event: ShellEvent| {
        let _ = sender.unbounded_send(event);
    };
    let mut child = match shell_command(cwd, command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            send(ShellEvent::Line {
                error: true,
                text: format!("could not start the shell: {error}"),
            });
            send(ShellEvent::Finished { cwd: None, code: None });
            return;
        }
    };
    send(ShellEvent::Started(child.id()));

    let errors = child.stderr.take().map(|pipe| {
        let sender = sender.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(pipe).split(b'\n').map_while(Result::ok) {
                let _ = sender.unbounded_send(ShellEvent::Line {
                    error: true,
                    text: String::from_utf8_lossy(&line).trim_end_matches('\r').to_string(),
                });
            }
        })
    });
    let mut final_cwd = None;
    if let Some(pipe) = child.stdout.take() {
        for line in std::io::BufReader::new(pipe).split(b'\n').map_while(Result::ok) {
            let text = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
            // The mark can follow output that did not end in a newline.
            match text.split_once(CWD_MARK) {
                Some((before, folder)) => {
                    if !before.is_empty() {
                        send(ShellEvent::Line { error: false, text: before.to_string() });
                    }
                    final_cwd = Some(folder.trim().to_string()).filter(|folder| !folder.is_empty());
                }
                None => send(ShellEvent::Line { error: false, text }),
            }
        }
    }
    if let Some(errors) = errors {
        let _ = errors.join();
    }
    let code = child.wait().ok().and_then(|status| status.code());
    send(ShellEvent::Finished { cwd: final_cwd, code });
}

/// The shell process for one command. PowerShell 7 (`pwsh`) when installed, else Windows
/// PowerShell; `sh` elsewhere. The script goes in encoded, so quotes in it need no escaping.
fn shell_command(cwd: &str, command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\n\
             $OutputEncoding = [System.Text.Encoding]::UTF8\n\
             Set-Location -LiteralPath '{folder}'\n\
             try {{\n{command}\n}} finally {{\n[Console]::Out.WriteLine('{mark}' + (Get-Location).ProviderPath)\n}}",
            folder = cwd.replace('\'', "''"),
            mark = CWD_MARK,
        );
        let mut process = std::process::Command::new(powershell_program());
        process
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand"])
            .arg(encode_powershell(&script))
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .creation_flags(CREATE_NO_WINDOW);
        process
    }
    #[cfg(not(windows))]
    {
        let mut process = std::process::Command::new("sh");
        process
            .arg("-c")
            .arg(format!("cd -- \"$1\" || exit; eval \"$2\"; status=$?; printf '{CWD_MARK}%s\\n' \"$PWD\"; exit $status"))
            .arg("sh")
            .arg(cwd)
            .arg(command)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0");
        process
    }
}

/// The shell commands run in, for the terminal's header.
pub(crate) fn shell_name() -> &'static str {
    #[cfg(windows)]
    {
        powershell_program()
    }
    #[cfg(not(windows))]
    {
        "sh"
    }
}

/// `pwsh` when it is on PATH, checked once.
#[cfg(windows)]
fn powershell_program() -> &'static str {
    use std::sync::OnceLock;
    static PROGRAM: OnceLock<&'static str> = OnceLock::new();
    PROGRAM.get_or_init(|| {
        let found = std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|folder| folder.join("pwsh.exe").is_file())
        });
        if found { "pwsh" } else { "powershell" }
    })
}

/// PowerShell's `-EncodedCommand`: base64 of the script in UTF-16LE.
#[cfg_attr(not(windows), allow(dead_code))]
fn encode_powershell(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// End a running shell command and everything it started (git, a pager…).
pub(crate) fn stop_shell(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .spawn();
    }
}

/// Diff of one file between two versions, for the Compare window.
pub(crate) fn compare_in_background(
    repo: SharedRepository,
    path: String,
    left: String,
    right: Option<String>,
) -> impl Future<Output = Result<Patch, DomainError>> {
    in_background(
        move || repo.compare_file(&path, &left, right.as_deref()),
        || Err(DomainError::Unknown("compare was interrupted".to_string())),
    )
}

pub(crate) fn list_models_in_background(
    list: crate::ModelLister,
    token: String,
) -> impl Future<Output = crate::ModelList> {
    in_background(
        move || list(token),
        || crate::ModelList {
            ids: Err(DomainError::Unknown(
                "model list was interrupted".to_string(),
            )),
            exchange: crate::ProviderExchange::kilo("model list was interrupted"),
        },
    )
}

pub(crate) fn suggest_commit_in_background(
    repo: SharedRepository,
    files: Vec<(String, FileDisposition)>,
    token: String,
    model: String,
    suggest: crate::CommitSuggester,
) -> impl Future<Output = crate::CommitSuggestion> {
    in_background(
        move || {
            let mut diff = String::new();
            for (path, place) in files {
                if diff.len() >= SUGGEST_DIFF_LIMIT {
                    break;
                }
                match repo.file_patch(&path, place) {
                    Ok(patch) => {
                        diff.push_str(&format_patch(&patch));
                        diff.push('\n');
                    }
                    Err(error) => {
                        return crate::CommitSuggestion::local_failure(error_message(&error))
                    }
                }
            }
            if diff.trim().is_empty() {
                return crate::CommitSuggestion::local_failure(
                    "Those files have no diff to describe.",
                );
            }
            if diff.len() > SUGGEST_DIFF_LIMIT {
                diff.truncate(SUGGEST_DIFF_LIMIT);
                diff.push_str("\n[diff truncated]\n");
            }
            suggest(token, model, diff)
        },
        || crate::CommitSuggestion::local_failure("commit suggestion was interrupted"),
    )
}

const SUGGEST_DIFF_LIMIT: usize = 12_000;

fn format_patch(patch: &Patch) -> String {
    let mut text = format!("# {}\n", patch.path);
    if patch.binary {
        text.push_str("binary file\n");
        return text;
    }
    for hunk in &patch.hunks {
        text.push_str(&hunk.header);
        text.push('\n');
        for line in &hunk.lines {
            let mark = match line.kind {
                core_domain::DiffLineKind::Added => '+',
                core_domain::DiffLineKind::Removed => '-',
                core_domain::DiffLineKind::Context => ' ',
            };
            text.push(mark);
            text.push_str(&line.text);
            text.push('\n');
        }
    }
    text
}

pub(crate) fn load_repository_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Vec<ViewEvent>> {
    in_background(
        move || load_repository(repo.as_ref()),
        || {
            vec![ViewEvent::ErrorOccurred(DomainError::Unknown(
                "repository load was interrupted".to_string(),
            ))]
        },
    )
}

pub(crate) fn checkout_in_background(
    repo: SharedRepository,
    branch: String,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.checkout_branch(&branch),
        || Err(DomainError::Unknown("checkout was interrupted".to_string())),
    )
}

pub(crate) fn checkout_detached_in_background(
    repo: SharedRepository,
    commit_id: String,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.checkout_detached(&commit_id),
        || Err(DomainError::Unknown("detach was interrupted".to_string())),
    )
}

pub(crate) fn fetch_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.fetch(),
        || Err(DomainError::Unknown("fetch was interrupted".to_string())),
    )
}

pub(crate) fn push_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.push(),
        || Err(DomainError::Unknown("push was interrupted".to_string())),
    )
}

pub(crate) fn pull_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.pull(),
        || Err(DomainError::Unknown("pull was interrupted".to_string())),
    )
}

pub(crate) fn edit_in_background(
    repo: SharedRepository,
    edit: WorktreeEdit,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match &edit {
            WorktreeEdit::Stage(paths) => repo.stage_paths(paths),
            WorktreeEdit::Unstage(paths) => repo.unstage_paths(paths),
            WorktreeEdit::Untrack { paths, ignore } => {
                repo.untrack_paths(paths)?;
                if ignore.is_empty() {
                    Ok(())
                } else {
                    repo.append_gitignore(ignore)
                }
            }
            WorktreeEdit::Commit(message) => repo.commit(message),
        },
        || {
            Err(DomainError::Unknown(
                "worktree edit was interrupted".to_string(),
            ))
        },
    )
}

/// Native folder picker. Cancel and a crashed dialog both yield `None`.
pub(crate) fn choose_repository_folder(
    title: &'static str,
) -> impl Future<Output = Option<String>> {
    in_background(
        move || {
            rfd::FileDialog::new()
                .set_title(title)
                .pick_folder()
                .map(|path| path.to_string_lossy().into_owned())
        },
        || None,
    )
}

pub(crate) fn open_repository_in_background(
    open: crate::RepositoryOpener,
    path: String,
) -> impl Future<Output = (String, Result<SharedRepository, DomainError>)> {
    // The interrupted fallback echoes the requested path, like the success path.
    let echoed = path.clone();
    in_background(
        move || {
            let result = open(&path);
            (path, result)
        },
        move || {
            (
                echoed,
                Err(DomainError::Unknown(
                    "repository open was interrupted".to_string(),
                )),
            )
        },
    )
}

pub(crate) fn worktrees_in_background(
    repo: SharedRepository,
) -> impl Future<Output = Result<Vec<Worktree>, DomainError>> {
    in_background(
        move || repo.worktrees(),
        || {
            Err(DomainError::Unknown(
                "worktree list was interrupted".to_string(),
            ))
        },
    )
}

/// `git worktree add`. With `create_branch`, `branch` is made at `start_point` or HEAD.
pub(crate) fn add_worktree_in_background(
    repo: SharedRepository,
    path: String,
    branch: String,
    create_branch: bool,
    start_point: Option<String>,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || {
            repo.add_worktree(
                &path,
                &branch,
                create_branch,
                start_point.as_deref(),
            )
        },
        || {
            Err(DomainError::Unknown(
                "worktree create was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn remove_worktree_in_background(
    repo: SharedRepository,
    path: String,
    force: bool,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.remove_worktree(&path, force),
        || {
            Err(DomainError::Unknown(
                "worktree remove was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn save_settings_in_background(
    save: crate::SettingsSaver,
    settings: core_domain::AppSettings,
) -> impl Future<Output = ()> {
    in_background(
        move || {
            save(settings);
        },
        || {},
    )
}

pub(crate) fn init_in_background(
    initialise: crate::RepositoryInitialiser,
    path: String,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || initialise(&path),
        || Err(DomainError::Unknown("init was interrupted".to_string())),
    )
}

pub(crate) fn clone_in_background(
    clone_repository: crate::RepositoryCloner,
    url: String,
    parent: String,
) -> impl Future<Output = Result<String, DomainError>> {
    in_background(
        move || clone_repository(&url, &parent),
        || Err(DomainError::Unknown("clone was interrupted".to_string())),
    )
}

pub(crate) fn load_patch(
    repo: &dyn GitRepository,
    query: PatchQuery,
) -> Result<Vec<Patch>, DomainError> {
    match query {
        PatchQuery::Commit(id) => repo.commit_patch(&id),
        PatchQuery::File { path, place } => repo.file_patch(&path, place).map(|patch| vec![patch]),
    }
}

pub(crate) fn load_patch_in_background(
    repo: SharedRepository,
    query: PatchQuery,
) -> impl Future<Output = Result<Vec<Patch>, DomainError>> {
    in_background(
        move || load_patch(repo.as_ref(), query),
        || {
            Err(DomainError::Unknown(
                "patch load was interrupted".to_string(),
            ))
        },
    )
}

/// Push, apply, pop, or drop. The index is the `stash@{n}` slot from the last list.
#[derive(Debug, Clone)]
pub(crate) enum StashAction {
    /// `git stash push [-u] [-m <message>]`.
    Push { message: String, include_untracked: bool },
    /// `git stash push [-m <message>] -- <paths>…`
    PushPaths { message: String, paths: Vec<String> },
    Apply(usize),
    Pop(usize),
    Drop(usize),
}

/// Create, rename, or delete one local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BranchAction {
    Create { name: String, start: String },
    Rename { from: String, to: String },
    Delete { name: String },
}

/// Cherry-pick, reset, merge, or rebase the commit or branch the user confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryAction {
    CherryPick { id: String },
    Reset { id: String, mode: ResetMode },
    Merge { name: String, no_ff: bool },
    Rebase { name: String },
    /// Stop the merge, rebase… in progress and restore the state before it.
    Abort(core_domain::RepoOperation),
    /// Finish it once conflicts are resolved.
    Continue(core_domain::RepoOperation),
    /// Keep one side of a conflicted file and stage it.
    Resolve { path: String, side: core_domain::ConflictSide },
}

pub(crate) fn history_in_background(
    repo: SharedRepository,
    action: HistoryAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            HistoryAction::CherryPick { id } => repo.cherry_pick(&id),
            HistoryAction::Reset { id, mode } => repo.reset(&id, mode),
            HistoryAction::Merge { name, no_ff } => repo.merge_branch(&name, no_ff),
            HistoryAction::Abort(operation) => repo.abort_operation(operation),
            HistoryAction::Continue(operation) => repo.continue_operation(operation),
            HistoryAction::Resolve { path, side } => repo.resolve_conflict(&path, side),
            HistoryAction::Rebase { name } => repo.rebase_onto(&name),
        },
        || {
            Err(DomainError::Unknown(
                "history update was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn branch_in_background(
    repo: SharedRepository,
    action: BranchAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            BranchAction::Create { name, start } => repo.create_branch(&name, &start),
            BranchAction::Rename { from, to } => repo.rename_branch(&from, &to),
            BranchAction::Delete { name } => repo.delete_branch(&name),
        },
        || {
            Err(DomainError::Unknown(
                "branch update was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn stash_in_background(
    repo: SharedRepository,
    action: StashAction,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || match action {
            StashAction::Push { message, include_untracked } => repo.stash_push(&message, include_untracked),
            StashAction::PushPaths { message, paths } => repo.stash_push_paths(&message, &paths),
            StashAction::Apply(index) => repo.stash_apply(index),
            StashAction::Pop(index) => repo.stash_pop(index),
            StashAction::Drop(index) => repo.stash_drop(index),
        },
        || Err(DomainError::Unknown("stash was interrupted".to_string())),
    )
}

/// Discard each file from the place it sits in (a selection can span Staged and Changes).
pub(crate) fn discard_paths_in_background(
    repo: SharedRepository,
    files: Vec<crate::changes::MarkedFile>,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || {
            for file in &files {
                repo.discard_path(&file.path, file.place)?;
            }
            Ok(())
        },
        || Err(DomainError::Unknown("discard was interrupted".to_string())),
    )
}

pub(crate) fn append_gitignore_in_background(
    repo: SharedRepository,
    patterns: Vec<String>,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.append_gitignore(&patterns),
        || {
            Err(DomainError::Unknown(
                "gitignore update was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn stage_hunk_in_background(
    repo: SharedRepository,
    path: String,
    hunk: Hunk,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.stage_hunk(&path, &hunk),
        || {
            Err(DomainError::Unknown(
                "stage hunk was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn discard_hunk_in_background(
    repo: SharedRepository,
    path: String,
    hunk: Hunk,
    place: FileDisposition,
) -> impl Future<Output = Result<(), DomainError>> {
    in_background(
        move || repo.discard_hunk(&path, &hunk, place),
        || {
            Err(DomainError::Unknown(
                "discard hunk was interrupted".to_string(),
            ))
        },
    )
}

pub(crate) fn error_message(error: &DomainError) -> String {
    match error {
        DomainError::RepositoryNotFound => "Not a Git repository.".to_string(),
        DomainError::AccessDenied => "Access denied.".to_string(),
        DomainError::AuthenticationFailed => {
            "Kilo authentication failed. Paste a valid API key under Settings → Agents and Models."
                .to_string()
        }
        DomainError::Unknown(message) => message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::{Branch, Commit, DomainError};
    use test_support::FakeGitRepository;

    #[test]
    fn base64_pads_like_the_standard_alphabet() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    /// Every event of one command, in order.
    fn run_to_end(cwd: &str, command: &str) -> Vec<ShellEvent> {
        let (sender, mut receiver) = iced::futures::channel::mpsc::unbounded();
        run_shell(cwd, command, sender);
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }

    #[test]
    fn a_shell_command_streams_its_lines_and_reports_where_cd_left_it() {
        let root = std::env::temp_dir().join(format!("gitcthulhu-shell-{}", std::process::id()));
        let inner = root.join("inner dir");
        std::fs::create_dir_all(&inner).expect("folders");
        let events = run_to_end(&inner.to_string_lossy(), "echo hello; cd ..");
        let _ = std::fs::remove_dir_all(&root);

        assert!(matches!(events.first(), Some(ShellEvent::Started(_))), "{events:?}");
        assert!(
            events.contains(&ShellEvent::Line { error: false, text: "hello".into() }),
            "{events:?}"
        );
        let Some(ShellEvent::Finished { cwd: Some(cwd), .. }) = events.last() else {
            panic!("no folder reported: {events:?}");
        };
        assert!(
            core_domain::same_repository_path(cwd, &root.to_string_lossy()),
            "{cwd} vs {}",
            root.display()
        );
    }

    #[test]
    fn standard_error_is_flagged() {
        let folder = std::env::temp_dir();
        let command = if cfg!(windows) {
            "[Console]::Error.WriteLine('broken')"
        } else {
            "echo broken >&2"
        };
        let events = run_to_end(&folder.to_string_lossy(), command);
        assert!(
            events.contains(&ShellEvent::Line { error: true, text: "broken".into() }),
            "{events:?}"
        );
    }

    fn commit(id: &str) -> Commit {
        Commit {
            id: id.to_string(),
            parent_ids: vec![],
            author: "Test Author".to_string(),
            message: format!("commit {id}"),
            timestamp: 0,
        }
    }

    fn branch(name: &str, is_head: bool) -> Branch {
        Branch {
            name: name.to_string(),
            is_remote: false,
            is_head,
            tip: String::new(),
        ahead: 0,
        behind: 0,
        }
    }

    fn populated_repo() -> FakeGitRepository {
        FakeGitRepository {
            log_result: Ok(vec![commit("aaaaaaa1"), commit("bbbbbbb2")]),
            branches_result: Ok(vec![branch("main", true), branch("feature/x", false)]),
            ..FakeGitRepository::ok()
        }
    }

    struct PanickingRepository;

    impl GitRepository for PanickingRepository {
        fn status(&self) -> Result<core_domain::RepoStatus, DomainError> {
            panic!("simulated adapter crash")
        }
        fn log(&self, _max_count: Option<usize>) -> Result<Vec<Commit>, DomainError> {
            unreachable!("status panics first")
        }
        fn branches(&self) -> Result<Vec<Branch>, DomainError> {
            unreachable!("status panics first")
        }
        fn stashes(&self) -> Result<Vec<core_domain::Stash>, DomainError> {
            unreachable!("status panics first")
        }
        fn tags(&self) -> Result<Vec<core_domain::Tag>, DomainError> {
            unreachable!("status panics first")
        }
        fn worktrees(&self) -> Result<Vec<core_domain::Worktree>, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn add_worktree(
            &self,
            _path: &str,
            _branch: &str,
            _create_branch: bool,
            _start_point: Option<&str>,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn remove_worktree(&self, _path: &str, _force: bool) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn checkout_branch(&self, _branch_name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn checkout_detached(&self, _commit_id: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn create_branch(&self, _name: &str, _start_point: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn rename_branch(&self, _from: &str, _to: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn delete_branch(&self, _name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn cherry_pick(&self, _commit_id: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn reset(
            &self,
            _commit_id: &str,
            _mode: core_domain::ResetMode,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn merge_branch(&self, _name: &str, _no_ff: bool) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn rebase_onto(&self, _name: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn fetch(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn push(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn pull(&self) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn unstage_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn untrack_paths(&self, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn commit(&self, _message: &str) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn commit_patch(&self, _commit_id: &str) -> Result<Vec<core_domain::Patch>, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn file_patch(
            &self,
            _path: &str,
            _place: core_domain::FileDisposition,
        ) -> Result<core_domain::Patch, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn discard_path(
            &self,
            _path: &str,
            _place: core_domain::FileDisposition,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stage_hunk(&self, _path: &str, _hunk: &core_domain::Hunk) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn discard_hunk(
            &self,
            _path: &str,
            _hunk: &core_domain::Hunk,
            _place: core_domain::FileDisposition,
        ) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_push(&self, _message: &str, _include_untracked: bool) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn abort_operation(&self, _operation: core_domain::RepoOperation) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn continue_operation(&self, _operation: core_domain::RepoOperation) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn resolve_conflict(&self, _path: &str, _side: core_domain::ConflictSide) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn compare_file(&self, _path: &str, _left: &str, _right: Option<&str>) -> Result<Patch, DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_push_paths(&self, _message: &str, _paths: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn append_gitignore(&self, _patterns: &[String]) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_apply(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_pop(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
        fn stash_drop(&self, _index: usize) -> Result<(), DomainError> {
            unreachable!("not used by load_repository")
        }
    }

    #[test]
    fn load_repository_reports_status_log_and_branches_in_order() {
        let events = load_repository(&populated_repo());
        assert_eq!(events.len(), 5);
        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(
            matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2 && commits[0].id == "aaaaaaa1")
        );
        assert!(
            matches!(&events[2], ViewEvent::BranchesUpdated(branches) if branches.len() == 2 && branches[0].is_head)
        );
        assert!(matches!(&events[3], ViewEvent::StashesUpdated(stashes) if stashes.is_empty()));
        assert!(matches!(&events[4], ViewEvent::TagsUpdated(tags) if tags.is_empty()));
    }

    #[test]
    fn load_repository_keeps_other_results_when_one_call_fails() {
        let repo = FakeGitRepository {
            branches_result: Err(DomainError::AccessDenied),
            ..populated_repo()
        };
        let events = load_repository(&repo);
        assert!(matches!(&events[0], ViewEvent::StatusUpdated(_)));
        assert!(matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2));
        assert!(matches!(
            &events[2],
            ViewEvent::ErrorOccurred(DomainError::AccessDenied)
        ));
    }

    #[test]
    fn load_repository_reports_missing_repository_for_every_call() {
        let repo = FakeGitRepository {
            status_result: Err(DomainError::RepositoryNotFound),
            log_result: Err(DomainError::RepositoryNotFound),
            branches_result: Err(DomainError::RepositoryNotFound),
            stashes_result: Err(DomainError::RepositoryNotFound),
            tags_result: Err(DomainError::RepositoryNotFound),
            ..FakeGitRepository::ok()
        };
        let events = load_repository(&repo);
        assert!(events.iter().all(|event| matches!(
            event,
            ViewEvent::ErrorOccurred(DomainError::RepositoryNotFound)
        )));
    }

    #[test]
    fn background_load_returns_the_same_events() {
        let events = iced::futures::executor::block_on(load_repository_in_background(Arc::new(
            populated_repo(),
        )));
        assert_eq!(events.len(), 5);
        assert!(matches!(&events[1], ViewEvent::LogUpdated(commits) if commits.len() == 2));
        assert!(matches!(&events[2], ViewEvent::BranchesUpdated(branches) if branches.len() == 2));
        assert!(matches!(&events[3], ViewEvent::StashesUpdated(stashes) if stashes.is_empty()));
        assert!(matches!(&events[4], ViewEvent::TagsUpdated(tags) if tags.is_empty()));
    }

    #[test]
    fn background_load_reports_a_crashed_adapter_as_an_error() {
        let events = iced::futures::executor::block_on(load_repository_in_background(Arc::new(
            PanickingRepository,
        )));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            ViewEvent::ErrorOccurred(DomainError::Unknown(_))
        ));
    }

    #[test]
    fn background_checkout_returns_the_adapter_result() {
        let refusing = FakeGitRepository {
            checkout_result: Err(DomainError::AccessDenied),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(checkout_in_background(
            Arc::new(FakeGitRepository::ok()),
            "feature".to_string(),
        ));
        let refused = iced::futures::executor::block_on(checkout_in_background(
            Arc::new(refusing),
            "feature".to_string(),
        ));
        assert_eq!(ok, Ok(()));
        assert_eq!(refused, Err(DomainError::AccessDenied));
    }

    #[test]
    fn background_fetch_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            fetch_result: Err(DomainError::Unknown("no remote".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(fetch_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(fetch_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(failed, Err(DomainError::Unknown("no remote".to_string())));
    }

    #[test]
    fn background_push_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            push_result: Err(DomainError::Unknown("no upstream".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(push_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(push_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(failed, Err(DomainError::Unknown("no upstream".to_string())));
    }

    #[test]
    fn background_pull_returns_the_adapter_result() {
        let failing = FakeGitRepository {
            pull_result: Err(DomainError::Unknown(
                "not possible to fast-forward".to_string(),
            )),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(pull_in_background(Arc::new(
            FakeGitRepository::ok(),
        )));
        let failed = iced::futures::executor::block_on(pull_in_background(Arc::new(failing)));
        assert_eq!(ok, Ok(()));
        assert_eq!(
            failed,
            Err(DomainError::Unknown(
                "not possible to fast-forward".to_string()
            ))
        );
    }

    #[test]
    fn background_open_returns_the_opener_result() {
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let seen_in_opener = Arc::clone(&seen);
        let open: crate::RepositoryOpener = Arc::new(move |path| {
            *seen_in_opener.lock().expect("path lock") = path.to_string();
            Ok(Arc::new(FakeGitRepository::ok()) as SharedRepository)
        });
        let (path, result) = iced::futures::executor::block_on(open_repository_in_background(
            open,
            r"D:\repo".to_string(),
        ));
        assert_eq!(path, r"D:\repo");
        assert!(result.is_ok());
        assert_eq!(seen.lock().expect("path lock").as_str(), r"D:\repo");
    }

    #[test]
    fn an_interrupted_open_echoes_the_requested_path() {
        let open: crate::RepositoryOpener = Arc::new(|_| panic!("opener thread died"));
        let (path, result) = iced::futures::executor::block_on(open_repository_in_background(
            open,
            r"D:\repo".to_string(),
        ));
        assert_eq!(path, r"D:\repo");
        assert!(result.is_err());
    }

    #[test]
    fn background_edit_returns_the_adapter_result() {
        let declining = FakeGitRepository {
            commit_result: Err(DomainError::Unknown("hook declined".to_string())),
            ..FakeGitRepository::ok()
        };
        let ok = iced::futures::executor::block_on(edit_in_background(
            Arc::new(FakeGitRepository::ok()),
            WorktreeEdit::Commit("record the change".to_string()),
        ));
        let declined = iced::futures::executor::block_on(edit_in_background(
            Arc::new(declining),
            WorktreeEdit::Commit("record the change".to_string()),
        ));
        assert_eq!(ok, Ok(()));
        assert_eq!(
            declined,
            Err(DomainError::Unknown("hook declined".to_string()))
        );
    }

    #[test]
    fn background_worktree_jobs_return_the_adapter_result() {
        let linked = Worktree {
            path: r"D:\repo-wt".to_string(),
            branch: "topic".to_string(),
            is_main: false,
            locked: false,
        };
        let repo = FakeGitRepository {
            worktrees_result: Ok(vec![linked.clone()]),
            add_worktree_result: Err(DomainError::Unknown("path exists".to_string())),
            ..FakeGitRepository::ok()
        };
        let shared: SharedRepository = Arc::new(repo);

        let listed =
            iced::futures::executor::block_on(worktrees_in_background(Arc::clone(&shared)));
        let added = iced::futures::executor::block_on(add_worktree_in_background(
            Arc::clone(&shared),
            r"D:\repo-wt".to_string(),
            "topic".to_string(),
            true,
            None,
        ));
        let removed = iced::futures::executor::block_on(remove_worktree_in_background(
            shared,
            r"D:\repo-wt".to_string(),
            false,
        ));

        assert_eq!(listed, Ok(vec![linked]));
        assert_eq!(added, Err(DomainError::Unknown("path exists".to_string())));
        assert_eq!(removed, Ok(()));
    }
}
