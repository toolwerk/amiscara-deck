//! The branch of a working folder, read from `.git/HEAD`. No `git` process.

use std::path::{Path, PathBuf};

/// Branch name, or the short commit if HEAD is detached. `None` outside a repo.
pub fn branch_of(cwd: &Path) -> Option<String> {
    let git = cwd
        .ancestors()
        .map(|d| d.join(".git"))
        .find(|p| p.exists())?;
    let head = resolve_git_dir(&git)?.join("HEAD");
    let text = std::fs::read_to_string(head).ok()?;
    let text = text.trim();
    match text.strip_prefix("ref: ") {
        Some(r) => Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()),
        None if text.len() >= 7 && text.chars().all(|c| c.is_ascii_hexdigit()) => {
            Some(text[..7].to_owned())
        }
        None => None,
    }
}

/// `.git` is a folder, or in a worktree a file `gitdir: <path>`.
fn resolve_git_dir(git: &Path) -> Option<PathBuf> {
    if git.is_dir() {
        return Some(git.to_owned());
    }
    let text = std::fs::read_to_string(git).ok()?;
    let target = text.trim().strip_prefix("gitdir: ")?;
    let target = Path::new(target);
    Some(if target.is_absolute() {
        target.to_owned()
    } else {
        git.parent()?.join(target)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(head: &str) -> tempfile::TempDir {
        let d = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(d.path().join(".git")).expect("mkdir");
        std::fs::write(d.path().join(".git/HEAD"), head).expect("write");
        d
    }

    #[test]
    fn reads_a_branch() {
        let d = repo("ref: refs/heads/feature/x\n");
        assert_eq!(branch_of(d.path()).as_deref(), Some("feature/x"));
    }

    #[test]
    fn finds_the_repo_from_a_sub_folder() {
        let d = repo("ref: refs/heads/main\n");
        let sub = d.path().join("src/deep");
        std::fs::create_dir_all(&sub).expect("mkdir");
        assert_eq!(branch_of(&sub).as_deref(), Some("main"));
    }

    #[test]
    fn a_detached_head_gives_the_short_commit() {
        let d = repo("0123456789abcdef0123456789abcdef01234567\n");
        assert_eq!(branch_of(d.path()).as_deref(), Some("0123456"));
    }

    #[test]
    fn follows_a_worktree_file() {
        let d = tempfile::tempdir().expect("tempdir");
        let real = d.path().join("main/.git/worktrees/wt");
        std::fs::create_dir_all(&real).expect("mkdir");
        std::fs::write(real.join("HEAD"), "ref: refs/heads/wt-branch\n").expect("write");
        let wt = d.path().join("wt");
        std::fs::create_dir_all(&wt).expect("mkdir");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).expect("write");
        assert_eq!(branch_of(&wt).as_deref(), Some("wt-branch"));
    }

    #[test]
    fn no_repo_no_branch_and_garbage_is_ignored() {
        let d = tempfile::tempdir().expect("tempdir");
        assert_eq!(branch_of(d.path()), None);
        let g = repo("what is this\n");
        assert_eq!(branch_of(g.path()), None);
    }
}
