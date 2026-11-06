//! `cargo run -p design_sync` — rebuild the design system's files from the code.
//!
//! Output: `target/design-system/project/`, the files the Design System artifact serves. The
//! `/design-sync` command in `.claude/commands` publishes them.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const OUT_DIR: &str = "target/design-system";

fn main() -> ExitCode {
    let root = repo_root();
    let out = root.join(OUT_DIR);
    let reference = git_reference(&root);
    let by = git(&root, &["config", "user.name"]).unwrap_or_else(|| "Claude Code".to_string());

    match gui_app::design_export::sync(&root, &out, &reference, &by) {
        Ok(report) => {
            println!("Design system built from {reference} into {}", out.display());
            println!("  {} files written", report.written.len());
            for line in &report.changed {
                println!("  colour changed  {line}");
            }
            for name in &report.added {
                println!("  colour added    {name}  (usage note needed in design/system/tokens.json)");
            }
            if !report.authored_only.is_empty() {
                println!("  authored only   {}", report.authored_only.join(", "));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("design_sync failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .expect("design_sync lives one level below the repository root")
}

/// `main@1a2b3c4`, with a `+` when the working tree is dirty.
fn git_reference(root: &Path) -> String {
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "HEAD".to_string());
    let sha = git(root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = git(root, &["status", "--porcelain"]).is_some_and(|status| !status.is_empty());
    format!("{branch}@{sha}{}", if dirty { "+" } else { "" })
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).current_dir(root).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}
