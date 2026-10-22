//! `helix-toolbox repos` run for real, in a repository of the test's own:
//! staged, committed, branched and stashed through confirmed plans, and a
//! task of the user's run in its folder.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// git with the test's identity and no settings of the user's.
fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The program, in a home of the test's own; whether it succeeded, and
/// what it printed.
fn toolbox(home: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_helix-toolbox"))
        .arg("--catalog-dir")
        .arg(root().join("tests/fixtures"))
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env_remove("HELIX_TOOLBOX_LOG")
        .output()
        .expect("the program runs");
    (
        output.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn a_repository_is_changed_only_through_confirmed_plans() {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("code/sample");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("one.txt"), "one\n").unwrap();
    git(&repo, &["add", "one.txt"]);
    git(&repo, &["commit", "-q", "-m", "First"]);
    // The default folders, those that exist: ~/code here.
    let (listed, text) = toolbox(home.path(), &["repos"]);
    assert!(
        listed && text.contains("sample") && text.contains("clean"),
        "{text}"
    );

    fs::write(repo.join("two.txt"), "two\n").unwrap();
    let (planned, text) = toolbox(
        home.path(),
        &["repos", "stage", "sample", "two.txt", "--dry-run"],
    );
    assert!(planned && text.contains("Dry run"), "{text}");
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]), "");
    let (staged, text) = toolbox(
        home.path(),
        &["repos", "stage", "sample", "two.txt", "--yes"],
    );
    assert!(staged, "{text}");
    assert_eq!(
        git(&repo, &["diff", "--cached", "--name-only"]),
        "two.txt\n"
    );
    let (committed, text) = toolbox(
        home.path(),
        &["repos", "commit", "sample", "-m", "Second", "--yes"],
    );
    assert!(committed, "{text}");
    assert_eq!(git(&repo, &["log", "-1", "--format=%s"]), "Second\n");

    let (branched, text) = toolbox(
        home.path(),
        &["repos", "branch", "sample", "topic", "--create", "--yes"],
    );
    assert!(branched, "{text}");
    assert_eq!(git(&repo, &["branch", "--show-current"]), "topic\n");
    fs::write(repo.join("one.txt"), "changed\n").unwrap();
    let (refused, text) = toolbox(home.path(), &["repos", "branch", "sample", "main", "--yes"]);
    assert!(!refused && text.contains("has changes"), "{text}");
    let (stashed, text) = toolbox(
        home.path(),
        &["repos", "stash", "sample", "-m", "Aside", "--yes"],
    );
    assert!(stashed, "{text}");
    assert_eq!(fs::read_to_string(repo.join("one.txt")).unwrap(), "one\n");
    let (applied, text) = toolbox(
        home.path(),
        &["repos", "stash-apply", "sample", "stash@{0}", "--yes"],
    );
    assert!(applied, "{text}");
    assert_eq!(
        fs::read_to_string(repo.join("one.txt")).unwrap(),
        "changed\n"
    );

    // A task of the user's, kept in workspaces.toml, runs in the folder.
    let (added, text) = toolbox(
        home.path(),
        &[
            "repos",
            "task-add",
            "sample",
            "mark",
            "sh -c 'pwd > ran.txt'",
            "--name",
            "Mark",
        ],
    );
    assert!(added, "{text}");
    let kept =
        fs::read_to_string(home.path().join("config/helix-toolbox/workspaces.toml")).unwrap();
    assert!(kept.contains("id = \"mark\""), "{kept}");
    let (ran, text) = toolbox(home.path(), &["repos", "task", "sample", "mark", "--yes"]);
    assert!(ran, "{text}");
    assert_eq!(
        fs::read_to_string(repo.join("ran.txt")).unwrap().trim(),
        repo.canonicalize().unwrap().display().to_string()
    );
}
