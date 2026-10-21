# Fetch, pull, and push should use the user's git

Status: Fetch, push, and fast-forward pull are implemented. A merge is not.

## The cut as agreed

Fetch, pull, and push, with one credential value. That was the moment Candidate E (the `fetch` / `fetch_ssh` split) was supposed to land.

## Why that cut is the wrong shape

`GitRepository` still has two fetch methods:

```rust
fn fetch(&self, store: &dyn CredentialStore) -> Result<(), DomainError>;
fn fetch_ssh(&self, ssh_manager: &dyn SshManager) -> Result<(), DomainError>;
```

Both return `Ok(())` after a token lookup or an SSH environment lookup. Nothing in the window calls them.

A caller has to know the transport before it can choose a method. That is the leak Candidate E named. Implementing the feature on this seam makes the leak permanent: the window, or `app`, must inspect the remote URL and then supply a secret.

"One credential" does not match a real repository.

- `CredentialStore::get_token("current_user")` is one token for an invented user. A repository can have several remotes, and git already knows which helper belongs to which URL.
- `SshManager::prepare_ssh_env` returns `GIT_SSH_COMMAND=ssh -i <first key on disk> -o IdentitiesOnly=yes`. That disables the agent and every other key. `get_default_key_path`, `resolve_subsystem_path`, and `preflight_check` are implementation steps sitting on the port. Nothing calls them.

Checkout, stage, unstage, and commit already run the user's `git`, and they keep the user's hooks and signing. Fetch and push are the same kind of porcelain. gix 0.66 is the wrong place to grow a second credential system beside the one git already runs.

June's `GitCredential` enum (`Token`, `SshKey`, `Anonymous`) fixed the method split and kept the mistake: the port still receives a secret. The window does not have one to pass.

Pull is a different product from fetch. `git pull` merges into the worktree and can stop in a conflict. There is no conflict view. A pull button that does that will leave the repository mid-merge and the Changes list looking like ordinary edits.

## Suggestion

One method, no credential argument, the user's git:

```rust
fn fetch(&self) -> Result<(), DomainError>;
```

`git fetch`, default remote, whatever `remote` and `credential` config the repository already has. `GIT_TERMINAL_PROMPT=0` is already set, so a missing helper fails with git's own text on the error line instead of opening a hidden prompt. Then refresh, so the remote names already listed in the sidebar can move.

Push is the same shape: `git push` with no refspec, so git's upstream rules apply. "No upstream" is git's message, shown as the error.

Pull is `git pull --no-rebase --ff-only`. `--no-rebase` wins over `pull.rebase`. A diverged branch fails with git's message and does not start a merge or a rebase. The worktree moves only for a fast-forward.

Delete `fetch_ssh`. Do not pass `CredentialStore` or `SshManager` into fetch. Leave `keyring_adapter`, `ssh_adapter`, and `auth_adapter` unwired. Add a credential helper only after a real fetch fails because the user's git has nothing configured.

## What this does not do

It does not prompt for a password, store a token, or choose an SSH key. A remote that needs an interactive prompt fails with git's own text on the error line.

## Why not a terminal

A terminal in the window would not receive that prompt. `git` is started with no console (`CREATE_NO_WINDOW` on Windows), null stdin, and `GIT_TERMINAL_PROMPT=0`, so it will not talk to a console even if one is drawn. Giving those back is how a passphrase dialog stalls the worker on "Loading…", the same failure a gpg pinentry can cause during commit.

The guide that matches git is an askpass helper, later, and only if a real fetch fails because nothing is configured. Git runs `GIT_ASKPASS` or `SSH_ASKPASS` with the prompt text ("Password for 'https://…'") and reads one line back. The window can show that sentence and return the line. That is a dialog, not a shell, and it does not choose a key or store a token.

Until then the error line is the guide: it is the sentence git already prints, and Fetch can be pressed again after the user's own helper or agent is set up.
