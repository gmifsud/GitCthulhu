# GitCthulhu

Desktop Git client. The window is iced. Repository operations go through the `GitRepository` port in `core_domain`, and `git_adapter` implements that port.

## Run

Rust, and `git` on `PATH`.

```
cargo run -p app -- <path-to-repo>
```

With no path, the window reopens the last repository when that folder is still a repository. Otherwise it opens on the welcome screen.

## Test

```
cargo test --workspace
```
