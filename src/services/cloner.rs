//! Clones a git repository into a temp directory for analysis.
//!
//! "Sparse" per the design doc is interpreted as: don't fetch full commit
//! history when we don't need to (shallow, depth=1), not a sparse working
//! tree — the analyzer (a later task) needs to walk the full checked-out
//! tree to detect technologies, so restricting which *files* get checked
//! out would work against it. A specific commit_sha still needs full
//! history to reach, since a depth-1 shallow fetch of a branch tip can't
//! check out an arbitrary older commit.

use std::path::{Path, PathBuf};
use std::time::Duration;

use doki_shared::error::{Error, ErrorCode, Result};
use tempfile::TempDir;

/// A cloned repository. Dropping this removes the temp directory (RAII
/// cleanup) — keep it alive for as long as the checkout is needed.
pub struct ClonedRepo {
    _temp_dir: TempDir,
    pub path: PathBuf,
    pub resolved_commit_sha: String,
}

pub struct Cloner {
    timeout: Duration,
}

impl Cloner {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Clones repo_url at branch (defaults to the remote's default branch
    /// when None) and checks out commit_sha if given, otherwise the
    /// branch tip. Fails with ScannerTimeout if the clone takes longer
    /// than the configured timeout, or ScannerCloneFailed on any git
    /// error.
    pub async fn clone(
        &self,
        repo_url: &str,
        branch: Option<&str>,
        commit_sha: Option<&str>,
    ) -> Result<ClonedRepo> {
        let repo_url = repo_url.to_string();
        let branch = branch.map(str::to_string);
        let commit_sha = commit_sha.map(str::to_string);

        let clone_task = tokio::task::spawn_blocking(move || {
            clone_blocking(&repo_url, branch.as_deref(), commit_sha.as_deref())
        });

        match tokio::time::timeout(self.timeout, clone_task).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_err)) => Err(Error::internal(format!(
                "clone task panicked: {join_err}"
            ))),
            Err(_) => Err(Error::domain(
                ErrorCode::ScannerTimeout,
                format!(
                    "git clone exceeded {}s timeout",
                    self.timeout.as_secs()
                ),
                true,
            )),
        }
    }
}

fn clone_blocking(
    repo_url: &str,
    branch: Option<&str>,
    commit_sha: Option<&str>,
) -> Result<ClonedRepo> {
    let temp_dir = tempfile::Builder::new()
        .prefix("mcp-scanner-")
        .tempdir()
        .map_err(|e| Error::internal(format!("create temp dir: {e}")))?;

    let repo = if let Some(sha) = commit_sha {
        clone_full(repo_url, branch, temp_dir.path())?;
        let repo = git2::Repository::open(temp_dir.path())
            .map_err(|e| clone_failed(format!("open cloned repo: {e}")))?;
        checkout_commit(&repo, sha)?;
        repo
    } else {
        clone_shallow(repo_url, branch, temp_dir.path())?
    };

    let resolved_commit_sha = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map(|commit| commit.id().to_string())
        .map_err(|e| clone_failed(format!("resolve HEAD commit: {e}")))?;

    let path = temp_dir.path().to_path_buf();
    Ok(ClonedRepo {
        _temp_dir: temp_dir,
        path,
        resolved_commit_sha,
    })
}

/// Tries a shallow (depth=1) clone first. Not every transport supports
/// shallow fetch — notably libgit2's local (file-path) transport rejects
/// it outright ("shallow fetch is not supported by the local transport"),
/// and some git hosting/proxy setups may too — so on failure this falls
/// back to a full clone rather than treating "transport can't do shallow"
/// as a hard error.
fn clone_shallow(repo_url: &str, branch: Option<&str>, into: &Path) -> Result<git2::Repository> {
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.depth(1);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_opts);
    if let Some(b) = branch {
        builder.branch(b);
    }

    match builder.clone(repo_url, into) {
        Ok(repo) => Ok(repo),
        Err(shallow_err) => {
            let _ = std::fs::remove_dir_all(into);
            tracing::warn!(
                repo_url,
                error = %shallow_err,
                "shallow clone failed, retrying as a full clone"
            );
            let mut builder = git2::build::RepoBuilder::new();
            if let Some(b) = branch {
                builder.branch(b);
            }
            builder
                .clone(repo_url, into)
                .map_err(|e| clone_failed(format!("clone of {repo_url} (shallow and full both failed): {e}")))
        }
    }
}

fn clone_full(repo_url: &str, branch: Option<&str>, into: &Path) -> Result<()> {
    let mut builder = git2::build::RepoBuilder::new();
    if let Some(b) = branch {
        builder.branch(b);
    }
    builder
        .clone(repo_url, into)
        .map_err(|e| clone_failed(format!("full clone of {repo_url}: {e}")))?;
    Ok(())
}

fn checkout_commit(repo: &git2::Repository, commit_sha: &str) -> Result<()> {
    let oid = git2::Oid::from_str(commit_sha)
        .map_err(|e| Error::bad_request(format!("invalid commit_sha {commit_sha}: {e}")))?;
    let commit = repo
        .find_commit(oid)
        .map_err(|e| clone_failed(format!("commit {commit_sha} not found: {e}")))?;

    repo.set_head_detached(commit.id())
        .map_err(|e| clone_failed(format!("set HEAD to {commit_sha}: {e}")))?;

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .map_err(|e| clone_failed(format!("checkout {commit_sha}: {e}")))?;

    Ok(())
}

fn clone_failed(message: String) -> Error {
    Error::domain(ErrorCode::ScannerCloneFailed, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds a local, hermetic test repo (no network) with two commits,
    /// each adding one file. Returns (repo dir, first commit sha, second
    /// commit sha).
    fn create_test_repo() -> (TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        let sig = git2::Signature::now("Test", "test@example.com").unwrap();

        fs::write(dir.path().join("first.txt"), "first").unwrap();
        let first_sha = {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("first.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "first commit", &tree, &[])
                .unwrap()
                .to_string()
        };

        fs::write(dir.path().join("second.txt"), "second").unwrap();
        let second_sha = {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("second.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "second commit", &tree, &[&parent])
                .unwrap()
                .to_string()
        };

        (dir, first_sha, second_sha)
    }

    fn repo_url(dir: &TempDir) -> String {
        dir.path().to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn clone_defaults_to_head_when_no_commit_sha_given() {
        let (origin, _first_sha, second_sha) = create_test_repo();
        let cloner = Cloner::new(Duration::from_secs(30));

        let cloned = cloner
            .clone(&repo_url(&origin), None, None)
            .await
            .expect("clone should succeed");

        assert_eq!(cloned.resolved_commit_sha, second_sha);
        assert!(cloned.path.join("first.txt").exists());
        assert!(cloned.path.join("second.txt").exists());
    }

    #[tokio::test]
    async fn clone_checks_out_specific_commit_sha() {
        let (origin, first_sha, _second_sha) = create_test_repo();
        let cloner = Cloner::new(Duration::from_secs(30));

        let cloned = cloner
            .clone(&repo_url(&origin), None, Some(&first_sha))
            .await
            .expect("clone should succeed");

        assert_eq!(cloned.resolved_commit_sha, first_sha);
        assert!(cloned.path.join("first.txt").exists());
        assert!(
            !cloned.path.join("second.txt").exists(),
            "second.txt shouldn't exist yet at the first commit"
        );
    }

    #[tokio::test]
    async fn clone_fails_on_invalid_repo() {
        let cloner = Cloner::new(Duration::from_secs(5));
        let result = cloner.clone("/nonexistent/path/to/repo", None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn clone_times_out() {
        let (origin, _first_sha, _second_sha) = create_test_repo();
        // Zero-duration timeout: the clone task can't possibly finish
        // before tokio::time::timeout fires, exercising the ScannerTimeout
        // path deterministically without needing a genuinely slow remote.
        let cloner = Cloner::new(Duration::from_nanos(1));

        let result = cloner.clone(&repo_url(&origin), None, None).await;
        assert!(result.is_err());
    }
}
