use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gpui-review-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        git(&path, &["init", "-b", "main"]).unwrap();
        git(&path, &["config", "user.name", "Review Test"]).unwrap();
        git(&path, &["config", "user.email", "review@example.invalid"]).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, text: &[u8]) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    fn commit(&self) {
        git(&self.0, &["add", "--all"]).unwrap();
        git(&self.0, &["commit", "-m", "fixture"]).unwrap();
    }
    fn load(&self, s: Scope) -> Snapshot {
        load(&self.0, &s, false, false).unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn scopes_preserve_index_and_untracked_paths_and_binary_changes() {
    let r = Repo::new();
    let name = "目录/a [b] space\nname.rs";
    r.write(name, b"one\ntwo\n");
    r.commit();
    r.write(name, b"one\nstaged\n");
    git(&r.0, &["add", "--", name]).unwrap();
    r.write(name, b"one\nworking\n");
    r.write("new file.txt", b"new\n");
    r.write("binary.dat", &[0, 1, 2]);
    let staged = r.load(Scope::Staged);
    assert_eq!(staged.files.len(), 1);
    assert_eq!((staged.working_additions, staged.working_deletions), (2, 1));
    assert_eq!((staged.index_additions, staged.index_deletions), (1, 1));
    assert_eq!(staged.files[0].path, name);
    assert!(staged.files[0].patch.contains("+staged"));
    let unstaged = r.load(Scope::Unstaged);
    assert_eq!(unstaged.files.len(), 3);
    let f = unstaged.files.iter().find(|f| f.path == name).unwrap();
    assert!(f.patch.contains("-staged"));
    assert!(f.patch.contains("+working"));
    let all = r.load(Scope::Uncommitted);
    assert!(
        all.files
            .iter()
            .find(|f| f.path == name)
            .unwrap()
            .patch
            .contains("-two")
    );
    assert!(
        all.files
            .iter()
            .find(|f| f.path == "binary.dat")
            .unwrap()
            .binary
    );
}
#[test]
fn stages_and_unstages_only_one_hunk_and_rejects_stale_snapshot() {
    let r = Repo::new();
    let original = (0..30).map(|n| format!("line {n}\n")).collect::<String>();
    r.write("file.txt", original.as_bytes());
    r.commit();
    let changed = original
        .replace("line 2\n", "changed 2\n")
        .replace("line 25\n", "changed 25\n");
    r.write("file.txt", changed.as_bytes());
    let snapshot = r.load(Scope::Unstaged);
    assert_eq!(snapshot.files[0].hunks.len(), 2);
    apply(
        &snapshot,
        &Scope::Unstaged,
        &Mutation::Hunk {
            path: "file.txt".into(),
            index: 0,
            reverse: false,
        },
    )
    .unwrap();
    let staged = r.load(Scope::Staged);
    assert!(staged.files[0].patch.contains("changed 2"));
    assert!(!staged.files[0].patch.contains("changed 25"));
    apply(
        &staged,
        &Scope::Staged,
        &Mutation::Hunk {
            path: "file.txt".into(),
            index: 0,
            reverse: true,
        },
    )
    .unwrap();
    assert!(r.load(Scope::Staged).files.is_empty());
    let fresh = r.load(Scope::Unstaged);
    r.write("file.txt", b"external edit\n");
    assert!(
        apply(&fresh, &Scope::Unstaged, &Mutation::Stage(None))
            .unwrap_err()
            .to_string()
            .contains("其他位置")
    );
}
#[test]
fn discard_unstaged_keeps_staged_content_and_other_files() {
    let r = Repo::new();
    r.write("a.txt", b"base\n");
    r.write("b.txt", b"base b\n");
    r.commit();
    r.write("a.txt", b"staged\n");
    git(&r.0, &["add", "--", "a.txt"]).unwrap();
    r.write("a.txt", b"unstaged\n");
    r.write("b.txt", b"changed b\n");
    apply(
        &r.load(Scope::Unstaged),
        &Scope::Unstaged,
        &Mutation::Discard("a.txt".into()),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(r.0.join("a.txt")).unwrap(),
        "staged\n"
    );
    assert_eq!(r.load(Scope::Staged).files.len(), 1);
    assert_eq!(r.load(Scope::Unstaged).files[0].path, "b.txt");
}
#[test]
fn root_commit_unborn_and_renames_have_correct_paths() {
    let r = Repo::new();
    r.write("new.txt", b"first\n");
    let unborn = r.load(Scope::Uncommitted);
    assert_eq!(unborn.files.len(), 1);
    apply(&unborn, &Scope::Uncommitted, &Mutation::Stage(None)).unwrap();
    assert_eq!(r.load(Scope::Staged).files.len(), 1);
    apply(
        &r.load(Scope::Staged),
        &Scope::Staged,
        &Mutation::Unstage(None),
    )
    .unwrap();
    r.commit();
    let sha = resolve(&r.0, "HEAD").unwrap();
    let commit = r.load(Scope::Commit(sha));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].status, 'A');
    git(&r.0, &["mv", "--", "new.txt", "renamed file.txt"]).unwrap();
    let renamed = r.load(Scope::Staged);
    assert_eq!(renamed.files[0].path, "renamed file.txt");
    assert_eq!(renamed.files[0].old_path.as_deref(), Some("new.txt"));
}
#[test]
fn copied_patch_applies_to_the_same_relative_filename() {
    let source = Repo::new();
    source.write("base", b"base\n");
    source.commit();
    source.write("a [x] 中文.txt", b"line one\nGPUI_REVIEW_PATCH\nlast");
    let s = source.load(Scope::Uncommitted);
    assert!(apply_command(&s.files).starts_with("git apply <<'GPUI_REVIEW_PATCH'"));
    let destination = Repo::new();
    destination.write("base", b"base\n");
    destination.commit();
    apply_stdin(&destination.0, &["apply", "--check"], &s.files[0].patch).unwrap();
    apply_stdin(&destination.0, &["apply"], &s.files[0].patch).unwrap();
    assert_eq!(
        std::fs::read(destination.0.join("a [x] 中文.txt")).unwrap(),
        b"line one\nGPUI_REVIEW_PATCH\nlast"
    );
}
#[test]
fn branch_scope_uses_merge_base_and_invalid_refs_are_data() {
    let r = Repo::new();
    r.write("file", b"base\n");
    r.commit();
    git(&r.0, &["checkout", "-b", "feature"]).unwrap();
    r.write("file", b"branch\n");
    r.commit();
    r.write("new", b"working\n");
    let s = r.load(Scope::Branch("main".into()));
    assert_eq!(s.files.len(), 2);
    assert!(s.files.iter().any(|f| f.patch.contains("+branch")));
    assert!(
        load(
            &r.0,
            &Scope::Branch("--output=/tmp/invalid".into()),
            false,
            false
        )
        .is_err()
    );
    assert!(safe_path("../elsewhere").is_err());
}

#[test]
fn full_context_and_whitespace_options_do_not_change_the_index_fingerprint() {
    let r = Repo::new();
    let original = (0..50).map(|i| format!("line {i}\n")).collect::<String>();
    r.write("f.txt", original.as_bytes());
    r.commit();
    r.write(
        "f.txt",
        original.replace("line 25\n", "changed 25\n").as_bytes(),
    );
    let full = load(&r.0, &Scope::Uncommitted, true, true).unwrap();
    apply(
        &full,
        &Scope::Uncommitted,
        &Mutation::Stage(Some("f.txt".into())),
    )
    .unwrap();
    assert_eq!(r.load(Scope::Staged).files.len(), 1);
}

#[test]
fn discard_addition_and_rename_restores_paths_without_losing_untracked_content() {
    let r = Repo::new();
    r.write("old", b"old\n");
    r.commit();
    git(&r.0, &["mv", "old", "renamed"]).unwrap();
    r.write("added", b"recoverable\n");
    git(&r.0, &["add", "--", "added"]).unwrap();
    apply(
        &r.load(Scope::Staged),
        &Scope::Staged,
        &Mutation::Discard("renamed".into()),
    )
    .unwrap();
    assert!(r.0.join("old").is_file());
    assert!(!r.0.join("renamed").exists());
    apply(
        &r.load(Scope::Staged),
        &Scope::Staged,
        &Mutation::Discard("added".into()),
    )
    .unwrap();
    assert!(!r.0.join("added").exists());
    assert!(r.load(Scope::Staged).files.is_empty());
    let backups = r.0.join(".git/gpui-discarded");
    let dir = std::fs::read_dir(backups)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read(dir.join("added")).unwrap(), b"recoverable\n");
}
#[test]
fn automatic_commit_preserves_unstaged_changes_when_excluded() {
    let r = Repo::new();
    r.write("staged", b"base\n");
    r.write("working", b"base\n");
    r.commit();
    r.write("staged", b"stage\n");
    git(&r.0, &["add", "--", "staged"]).unwrap();
    r.write("working", b"keep\n");
    apply(
        &r.load(Scope::Uncommitted),
        &Scope::Uncommitted,
        &Mutation::Commit {
            message: String::new(),
            stage_all: false,
            branch: Some("codex/test-commit".into()),
        },
    )
    .unwrap();
    assert_eq!(git(&r.0, &["show", "HEAD:staged"]).unwrap(), "stage\n");
    assert_eq!(git(&r.0, &["show", "HEAD:working"]).unwrap(), "base\n");
    assert_eq!(r.load(Scope::Unstaged).files.len(), 1);
}

#[test]
fn stage_all_and_commit_handle_already_staged_renames_and_deletions() {
    let r = Repo::new();
    r.write("old", b"renamed content\n");
    r.write("deleted", b"removed\n");
    r.commit();
    git(&r.0, &["mv", "old", "new"]).unwrap();
    git(&r.0, &["rm", "deleted"]).unwrap();
    r.write("untracked", b"added\n");
    apply(
        &r.load(Scope::Uncommitted),
        &Scope::Uncommitted,
        &Mutation::Stage(None),
    )
    .unwrap();
    assert!(r.load(Scope::Unstaged).files.is_empty());
    r.write("another", b"included by commit\n");
    apply(
        &r.load(Scope::Uncommitted),
        &Scope::Uncommitted,
        &Mutation::Commit {
            message: "Commit mixed changes".into(),
            stage_all: true,
            branch: None,
        },
    )
    .unwrap();
    assert_eq!(
        git(&r.0, &["show", "HEAD:new"]).unwrap(),
        "renamed content\n"
    );
    assert_eq!(
        git(&r.0, &["show", "HEAD:another"]).unwrap(),
        "included by commit\n"
    );
    assert!(r.load(Scope::Uncommitted).files.is_empty());
}
#[test]
fn historical_patch_preserves_quoted_paths_line_numbers_and_no_newline() {
    let patch = "diff --git \"a/目录/a b.rs\" \"b/目录/a b.rs\"\n--- \"a/目录/a b.rs\"\n+++ \"b/目录/a b.rs\"\n@@ -50 +50 @@\n-old\n+new\n\\ No newline at end of file\n";
    let files = parse_unified(patch);
    assert_eq!(files[0].path, "目录/a b.rs");
    assert_eq!(files[0].hunks[0].lines[1].new, Some(50));
    assert_eq!(files[0].patch, patch);
}

#[test]
#[cfg(unix)]
fn pull_request_commits_pushes_and_passes_literal_fields_to_gh() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("GPUI_REVIEW_PR_TEST_CHILD").is_none() {
        let bin = Repo::new();
        let gh = bin.0.join("gh");
        let capture = bin.0.join("argv.json");
        std::fs::write(&gh,"#!/usr/bin/env python3\nimport json,os,sys\nif sys.argv[1:3]==['auth','status']: sys.exit(0)\nif sys.argv[1:3]==['pr','view']: sys.exit(1)\nif sys.argv[1:3]==['pr','create']:\n open(os.environ['GPUI_REVIEW_PR_ARGV'],'w').write(json.dumps(sys.argv[1:]))\n print('https://example.invalid/pull/1')\n sys.exit(0)\nsys.exit(2)\n").unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!(
            "{}:{}",
            bin.0.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "git_review::tests::pull_request_commits_pushes_and_passes_literal_fields_to_gh",
                "--nocapture",
            ])
            .env("GPUI_REVIEW_PR_TEST_CHILD", "1")
            .env("GPUI_REVIEW_PR_ARGV", &capture)
            .env("PATH", path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let args: Vec<String> = serde_json::from_slice(&std::fs::read(capture).unwrap()).unwrap();
        assert!(args.contains(&"--draft".into()));
        assert!(args.contains(&"标题 $(literal) `value`".into()));
        assert!(args.contains(&"First line\nSecond line 🙂".into()));
        return;
    }
    let r = Repo::new();
    r.write("file", b"base\n");
    r.commit();
    let remote = Repo::new();
    git(&remote.0, &["config", "core.bare", "true"]).unwrap();
    git(
        &r.0,
        &[
            "remote",
            "add",
            "origin",
            remote.0.join(".git").to_str().unwrap(),
        ],
    )
    .unwrap();
    git(&r.0, &["push", "-u", "origin", "main"]).unwrap();
    r.write("file", b"PR change\n");
    let snapshot = r.load(Scope::Uncommitted);
    let url = apply(
        &snapshot,
        &Scope::Uncommitted,
        &Mutation::PullRequest(PullRequestOptions {
            title: "标题 $(literal) `value`".into(),
            body: "First line\nSecond line 🙂".into(),
            base: "main".into(),
            branch: Some("codex/pr-test".into()),
            include_local: true,
            draft: true,
        }),
    )
    .unwrap();
    assert_eq!(url, "https://example.invalid/pull/1");
    assert_eq!(git(&r.0, &["show", "HEAD:file"]).unwrap(), "PR change\n");
    assert_eq!(
        resolve(&r.0, "HEAD").unwrap(),
        resolve(&remote.0, "codex/pr-test").unwrap()
    );
}
