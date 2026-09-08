//! Local Git operations for the review pane. No UI or agent wire types belong here.
//!
//! Paths come from NUL-delimited Git output. Commands use argv (never a shell),
//! and mutations check that the displayed snapshot is still current first.

mod comments;
mod process;
pub use comments::{ReviewComment, comments_prompt};

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail};

#[cfg(not(test))]
pub fn shutdown() {
    process::shutdown();
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    LastTurn,
    #[default]
    Uncommitted,
    Unstaged,
    Staged,
    Commit(String),
    Branch(String),
}

impl Scope {
    pub fn label(&self) -> &str {
        match self {
            Self::LastTurn => "上一轮",
            Self::Uncommitted => "未提交",
            Self::Unstaged => "未暂存",
            Self::Staged => "已暂存",
            Self::Commit(_) => "已提交",
            Self::Branch(_) => "分支",
        }
    }
    pub fn editable(&self) -> bool {
        matches!(self, Self::Uncommitted | Self::Unstaged | Self::Staged)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
    pub kind: LineKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
    pub patch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: char,
    pub binary: bool,
    pub patch: String,
    pub hunks: Vec<Hunk>,
    pub additions: usize,
    pub deletions: usize,
    pub old_text: Option<String>,
    pub new_text: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub root: PathBuf,
    pub branch: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub branches: Vec<String>,
    pub commits: Vec<(String, String)>,
    pub files: Vec<FileDiff>,
    pub staged: usize,
    pub unstaged: usize,
    pub ahead: usize,
    pub working_additions: usize,
    pub working_deletions: usize,
    pub index_additions: usize,
    pub index_deletions: usize,
    pub fingerprint: u64,
}

#[derive(Clone, Debug)]
pub enum Mutation {
    Stage(Option<String>),
    Unstage(Option<String>),
    Discard(String),
    DiscardAll,
    Hunk {
        path: String,
        index: usize,
        reverse: bool,
    },
    Commit {
        message: String,
        stage_all: bool,
        branch: Option<String>,
    },
    Push,
    PullRequest(PullRequestOptions),
}

#[derive(Clone, Debug)]
pub struct PullRequestOptions {
    pub title: String,
    pub body: String,
    pub base: String,
    pub branch: Option<String>,
    pub include_local: bool,
    pub draft: bool,
}

pub fn existing_pull_request(root: &Path) -> Option<String> {
    let out = process::run(
        Command::new("gh")
            .current_dir(root)
            .args(["pr", "view", "--json", "url", "--jq", ".url"])
            .stdin(Stdio::null()),
        None,
        Duration::from_secs(15),
    )
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    url.starts_with("https://").then_some(url)
}

fn pull_request_command(root: &Path, options: &PullRequestOptions, head: &str) -> Command {
    let mut cmd = Command::new("gh");
    cmd.current_dir(root)
        .args([
            "pr",
            "create",
            "--base",
            &options.base,
            "--head",
            head,
            "--title",
            &options.title,
            "--body",
            &options.body,
        ])
        .stdin(Stdio::null());
    if options.draft {
        cmd.arg("--draft");
    }
    cmd
}

fn create_pull_request(
    snapshot: &Snapshot,
    scope: &Scope,
    options: &PullRequestOptions,
) -> Result<String> {
    let root = &snapshot.root;
    // Check availability/authentication before changing the local branch or index.
    checked(
        process::run(
            Command::new("gh")
                .current_dir(root)
                .args(["auth", "status"])
                .stdin(Stdio::null()),
            None,
            Duration::from_secs(30),
        )
        .context("创建 PR 需要已登录的 GitHub CLI（gh）")?,
    )?;
    if options.branch.is_none()
        && let Some(url) = existing_pull_request(root)
    {
        return Ok(url);
    }
    let base = resolve(root, &options.base)
        .or_else(|_| resolve(root, &format!("origin/{}", options.base)))?;
    if options.include_local && snapshot.staged + snapshot.unstaged > 0 {
        apply(
            snapshot,
            scope,
            &Mutation::Commit {
                message: String::new(),
                stage_all: true,
                branch: options.branch.clone(),
            },
        )?;
    } else if let Some(branch) = &options.branch {
        if branch.starts_with('-') || branch == "codex/" {
            bail!("请输入有效的分支名称");
        }
        git(root, &["check-ref-format", "--branch", branch])?;
        git(root, &["switch", "-c", branch])?;
    }
    let fresh = load(root, &Scope::Uncommitted, false, false)?;
    apply(&fresh, &Scope::Uncommitted, &Mutation::Push)
        .context("推送分支失败；已完成的本地提交会保留")?;
    let mut options = options.clone();
    if options.title.trim().is_empty() {
        options.title = git(root, &["log", "-1", "--format=%s"])?.trim().into();
    }
    if options.body.trim().is_empty() {
        options.body = git(
            root,
            &["log", "--format=- %s", &format!("{base}..HEAD"), "--"],
        )?;
    }
    let output = checked(process::run(
        &mut pull_request_command(root, &options, &fresh.branch),
        None,
        Duration::from_secs(120),
    )?)
    .context("分支已推送，但创建 PR 失败")?;
    String::from_utf8(output)?
        .lines()
        .find(|s| s.starts_with("https://"))
        .map(str::to_owned)
        .context("GitHub CLI 未返回 PR 地址")
}

fn command(root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdin(Stdio::null());
    cmd
}

fn output(root: &Path, args: &[&str]) -> Result<Output> {
    let timeout = if matches!(args.first(), Some(&"push" | &"commit")) {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(15)
    };
    process::run(command(root).args(args), None, timeout).context("无法运行 Git")
}

fn checked(out: Output) -> Result<Vec<u8>> {
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    String::from_utf8(checked(output(root, args)?)?).context("Git 输出不是 UTF-8 文本")
}

fn safe_path(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    if path.is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("无效的仓库相对路径");
    }
    Ok(p)
}

fn resolve(root: &Path, rev: &str) -> Result<String> {
    if rev.is_empty() || rev.starts_with('-') || rev.contains(['\0', '\n', '\r']) {
        bail!("无效的 Git 引用");
    }
    Ok(git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )?
    .trim()
    .into())
}

#[derive(Debug)]
struct StatusEntry {
    x: char,
    y: char,
    path: String,
    old_path: Option<String>,
}

fn status(root: &Path) -> Result<Vec<StatusEntry>> {
    let bytes = checked(output(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?)?;
    let mut parts = bytes.split(|b| *b == 0).filter(|p| !p.is_empty());
    let mut entries = Vec::new();
    while let Some(part) = parts.next() {
        if part.len() < 4 {
            bail!("无法解析 Git 文件状态");
        }
        let path = String::from_utf8(part[3..].to_vec()).context("不支持非 UTF-8 文件名")?;
        safe_path(&path)?;
        let old_path = if [part[0], part[1]].iter().any(|c| *c == b'R' || *c == b'C') {
            Some(String::from_utf8(
                parts.next().context("重命名状态缺少原路径")?.to_vec(),
            )?)
        } else {
            None
        };
        entries.push(StatusEntry {
            x: part[0] as char,
            y: part[1] as char,
            path,
            old_path,
        });
    }
    Ok(entries)
}

fn stamp(root: &Path) -> Result<u64> {
    let mut hash = DefaultHasher::new();
    output(root, &["rev-parse", "HEAD"])?.stdout.hash(&mut hash);
    checked(output(
        root,
        &["diff", "--binary", "--no-ext-diff", "--no-textconv"],
    )?)?
    .hash(&mut hash);
    checked(output(
        root,
        &[
            "diff",
            "--cached",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )?)?
    .hash(&mut hash);
    for entry in status(root)? {
        entry.path.hash(&mut hash);
        entry.x.hash(&mut hash);
        entry.y.hash(&mut hash);
        if entry.x == '?' {
            let path = root.join(&entry.path);
            // Never follow untracked symlinks outside the repository.
            if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
                std::fs::read_link(path)?.hash(&mut hash);
            } else if let Ok(data) = std::fs::read(path) {
                data.hash(&mut hash);
            }
        }
    }
    Ok(hash.finish())
}

fn diff_counts(root: &Path, revision: Option<&str>, cached: bool) -> Result<(usize, usize)> {
    let mut args = vec!["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv"];
    if cached {
        args.push("--cached");
    }
    if let Some(rev) = revision {
        args.push(rev);
    }
    args.push("--");
    let raw = git(root, &args)?;
    let mut parts = raw.split('\0').filter(|s| !s.is_empty());
    let mut result = (0, 0);
    while let Some(record) = parts.next() {
        let fields = record.splitn(3, '\t').collect::<Vec<_>>();
        if fields.len() != 3 {
            continue;
        }
        result.0 += fields[0].parse::<usize>().unwrap_or(0);
        result.1 += fields[1].parse::<usize>().unwrap_or(0);
        if fields[2].is_empty() {
            parts.next();
            parts.next();
        }
    }
    Ok(result)
}

pub fn load(cwd: &Path, scope: &Scope, full: bool, ignore_whitespace: bool) -> Result<Snapshot> {
    load_with_options(
        cwd,
        scope,
        full,
        ignore_whitespace,
        true,
        &std::collections::HashSet::new(),
    )
}

pub fn load_with_options(
    cwd: &Path,
    scope: &Scope,
    full: bool,
    ignore_whitespace: bool,
    load_files: bool,
    expanded_files: &std::collections::HashSet<String>,
) -> Result<Snapshot> {
    let root = PathBuf::from(
        git(cwd, &["rev-parse", "--show-toplevel"])
            .context("此目录不是 Git 仓库")?
            .trim(),
    );
    let initial_fingerprint = stamp(&root)?;
    let head = resolve(&root, "HEAD").ok();
    let symbolic_branch = git(&root, &["symbolic-ref", "--short", "HEAD"]).ok();
    let detached = symbolic_branch.is_none();
    let branch = symbolic_branch
        .map(|s| s.trim().into())
        .unwrap_or_else(|| "HEAD".into());
    let upstream = git(
        &root,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()
    .map(|s| s.trim().to_owned());
    let branches = git(
        &root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
        ],
    )?
    .lines()
    .filter(|s| !s.ends_with("/HEAD"))
    .map(String::from)
    .collect();
    let commit_range = upstream.as_ref().map(|u| format!("{u}..HEAD"));
    let commits: Vec<_> = git(
        &root,
        &[
            "log",
            "-100",
            "--format=%H%x09%s",
            commit_range.as_deref().unwrap_or("HEAD"),
            "--",
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|s| s.split_once('\t').map(|(a, b)| (a.into(), b.into())))
    .collect();
    let states = status(&root)?;
    let index_counts = diff_counts(&root, None, true)?;
    let mut working_counts = if head.is_some() {
        diff_counts(&root, head.as_deref(), false)?
    } else {
        (0, 0)
    };
    for entry in &states {
        if (entry.x == '?' || head.is_none())
            && let Some(text) = read_worktree(&root, &entry.path)
        {
            working_counts.0 += text.lines().count();
        }
    }
    let mut result = Snapshot {
        root: root.clone(),
        branch,
        detached,
        upstream,
        branches,
        ahead: commit_range.as_ref().map_or(0, |_| commits.len()),
        commits,
        working_additions: working_counts.0,
        working_deletions: working_counts.1,
        index_additions: index_counts.0,
        index_deletions: index_counts.1,
        staged: states.iter().filter(|e| e.x != ' ' && e.x != '?').count(),
        unstaged: states.iter().filter(|e| e.y != ' ').count(),
        ..Default::default()
    };
    let mut args = vec![
        "diff".to_owned(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        "--find-renames".into(),
    ];
    let mut old_ref = head.clone();
    let mut new_ref = None;
    match scope {
        Scope::LastTurn => {}
        Scope::Uncommitted => {
            if let Some(h) = &head {
                args.push(h.clone());
            } else {
                args.push("--cached".into());
            }
        }
        Scope::Unstaged => {
            old_ref = Some(":".into());
        }
        Scope::Staged => {
            args.push("--cached".into());
            new_ref = Some(":".into());
        }
        Scope::Branch(base) => {
            let base = resolve(&root, base)?;
            let merge = git(&root, &["merge-base", &base, "HEAD"])?;
            old_ref = Some(merge.trim().into());
            args.push(merge.trim().into());
        }
        Scope::Commit(sha) => {
            let sha = resolve(&root, sha)?;
            old_ref = resolve(&root, &format!("{sha}^")).ok();
            new_ref = Some(sha.clone());
            if let Some(old) = &old_ref {
                args.push(old.clone());
                args.push(sha);
            } else {
                args = vec![
                    "show".into(),
                    "--format=".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    sha,
                ];
            }
        }
    }
    if !matches!(scope, Scope::LastTurn) {
        let mut names = args.clone();
        names.extend(["--name-status".into(), "-z".into(), "--".into()]);
        let raw = git(&root, &names.iter().map(String::as_str).collect::<Vec<_>>())?;
        let mut parts = raw.split('\0').filter(|s| !s.is_empty());
        while let Some(kind) = parts.next() {
            let first = parts.next().context("Git 差异缺少文件名")?;
            let (old_path, path) = if kind.starts_with(['R', 'C']) {
                (
                    Some(first.to_owned()),
                    parts.next().context("Git 重命名差异缺少目标文件名")?,
                )
            } else {
                (None, first)
            };
            safe_path(path)?;
            let mut patch_args = args.clone();
            patch_args.extend([
                "--binary".into(),
                "--full-index".into(),
                format!(
                    "--unified={}",
                    if full || expanded_files.contains(path) {
                        100000
                    } else {
                        3
                    }
                ),
            ]);
            if ignore_whitespace {
                patch_args.push("--ignore-all-space".into());
            }
            patch_args.push("--".into());
            if let Some(old) = &old_path {
                patch_args.push(old.clone());
            }
            patch_args.push(path.into());
            let patch = git(
                &root,
                &patch_args.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            let mut file = parse_patch(path.into(), patch, kind.chars().next().unwrap_or('M'));
            file.old_path = old_path;
            let old_path = file.old_path.as_deref().unwrap_or(path);
            if load_files {
                file.old_text = read_revision(&root, old_ref.as_deref(), old_path);
                file.new_text = match &new_ref {
                    Some(r) => read_revision(&root, Some(r), path),
                    None => read_worktree(&root, path),
                };
            }
            result.files.push(file);
        }
        if matches!(
            scope,
            Scope::Uncommitted | Scope::Unstaged | Scope::Branch(_)
        ) {
            for entry in &states {
                if entry.x == '?' || (head.is_none() && matches!(scope, Scope::Uncommitted)) {
                    result.files.retain(|f| f.path != entry.path);
                    let path = root.join(&entry.path);
                    let out = output(
                        &root,
                        &[
                            "diff",
                            "--no-index",
                            "--binary",
                            "--full-index",
                            "--",
                            "/dev/null",
                            path.to_str().context("无效路径")?,
                        ],
                    )?;
                    if out.status.code() != Some(1) && !out.status.success() {
                        checked(out)?;
                        continue;
                    }
                    let raw = String::from_utf8(out.stdout)?;
                    // no-index emits absolute paths. Use repository-relative quoted headers
                    // so copied patches and git apply never target a different checkout.
                    let quoted = quote_patch_path(&format!("b/{}", entry.path));
                    let mut patch = format!(
                        "diff --git {} {}\n",
                        quote_patch_path(&format!("a/{}", entry.path)),
                        quoted
                    );
                    for line in raw.lines().skip(1) {
                        if line.starts_with("+++ ") {
                            patch.push_str(&format!("+++ {quoted}\n"));
                        } else {
                            patch.push_str(line);
                            patch.push('\n');
                        }
                    }
                    let mut file = parse_patch(entry.path.clone(), patch, 'A');
                    if load_files {
                        file.new_text = read_worktree(&root, &entry.path);
                    }
                    result.files.push(file);
                }
            }
        }
    }
    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result.fingerprint = stamp(&root)?;
    if result.fingerprint != initial_fingerprint {
        bail!("文件正在更改，请稍后刷新。");
    }
    Ok(result)
}

fn read_worktree(root: &Path, path: &str) -> Option<String> {
    let p = root.join(path);
    if std::fs::symlink_metadata(&p).ok()?.file_type().is_symlink() {
        return None;
    }
    let canonical = p.canonicalize().ok()?;
    if !canonical.starts_with(root) || std::fs::metadata(&canonical).ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    let text = std::fs::read_to_string(canonical).ok()?;
    (!text.contains('\0')).then_some(text)
}

fn read_revision(root: &Path, revision: Option<&str>, path: &str) -> Option<String> {
    let revision = revision?;
    let spec = if revision == ":" {
        format!(":{path}")
    } else {
        format!("{revision}:{path}")
    };
    let text = git(root, &["show", &spec]).ok()?;
    (!text.contains('\0') && text.len() <= 4 * 1024 * 1024).then_some(text)
}

fn quote_patch_path(path: &str) -> String {
    // Git's C quoting agrees with JSON for these ASCII escapes. Non-ASCII UTF-8
    // is left intact by serde_json and accepted by core.quotepath=false.
    serde_json::to_string(path).expect("serializing a string cannot fail")
}

pub fn parse_patch(path: String, patch: String, status: char) -> FileDiff {
    let mut file = FileDiff {
        path,
        old_path: None,
        status,
        binary: patch.contains("GIT binary patch") || patch.contains("Binary files "),
        patch: patch.clone(),
        hunks: Vec::new(),
        additions: 0,
        deletions: 0,
        old_text: None,
        new_text: None,
    };
    let mut header = String::new();
    let (mut old, mut new) = (0, 0);
    for raw in patch.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        if line.starts_with("@@ ") {
            let fields: Vec<_> = line.split_whitespace().collect();
            let number = |s: &str| {
                s.get(1..)
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            if fields.len() < 3 {
                continue;
            }
            old = number(fields[1]);
            new = number(fields[2]);
            file.hunks.push(Hunk {
                header: line.into(),
                lines: Vec::new(),
                patch: format!("{header}{raw}"),
            });
            continue;
        }
        let Some(hunk) = file.hunks.last_mut() else {
            header.push_str(raw);
            continue;
        };
        hunk.patch.push_str(raw);
        let entry = match line.as_bytes().first() {
            Some(b'+') => {
                let l = Line {
                    old: None,
                    new: Some(new),
                    text: line[1..].into(),
                    kind: LineKind::Added,
                };
                new += 1;
                file.additions += 1;
                l
            }
            Some(b'-') => {
                let l = Line {
                    old: Some(old),
                    new: None,
                    text: line[1..].into(),
                    kind: LineKind::Deleted,
                };
                old += 1;
                file.deletions += 1;
                l
            }
            Some(b' ') => {
                let l = Line {
                    old: Some(old),
                    new: Some(new),
                    text: line[1..].into(),
                    kind: LineKind::Context,
                };
                old += 1;
                new += 1;
                l
            }
            _ => continue,
        };
        hunk.lines.push(entry);
    }
    file
}

/// Parse preserved app-server patches without consulting the working tree.
pub fn parse_unified(diff: &str) -> Vec<FileDiff> {
    let mut blocks = Vec::new();
    let mut block = String::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") && !block.is_empty() {
            blocks.push(std::mem::take(&mut block));
        }
        block.push_str(line);
    }
    if !block.is_empty() {
        blocks.push(block);
    }
    blocks
        .into_iter()
        .map(|patch| {
            let path = patch
                .lines()
                .find_map(|l| l.strip_prefix("+++ ").filter(|p| *p != "/dev/null"))
                .or_else(|| patch.lines().find_map(|l| l.strip_prefix("--- ")))
                .map(unquote_path)
                .unwrap_or_else(|| "变更".into());
            let path = path
                .strip_prefix("b/")
                .or_else(|| path.strip_prefix("a/"))
                .unwrap_or(&path)
                .to_owned();
            let status = if patch.contains("new file mode ") {
                'A'
            } else if patch.contains("deleted file mode ") {
                'D'
            } else {
                'M'
            };
            parse_patch(path, patch, status)
        })
        .collect()
}

fn unquote_path(path: &str) -> String {
    let Some(p) = path.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return path.into();
    };
    let mut bytes = Vec::new();
    let mut chars = p.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        let Some(b) = chars.next() else {
            break;
        };
        bytes.push(match b {
            b'n' => b'\n',
            b't' => b'\t',
            b'r' => b'\r',
            b'0'..=b'7' => {
                let mut n = b - b'0';
                for _ in 0..2 {
                    if let Some(c) = chars.peek().copied().filter(|c| matches!(c, b'0'..=b'7')) {
                        chars.next();
                        n = n.wrapping_mul(8).wrapping_add(c - b'0');
                    }
                }
                n
            }
            _ => b,
        });
    }
    String::from_utf8_lossy(&bytes).into()
}

pub fn apply(snapshot: &Snapshot, scope: &Scope, mutation: &Mutation) -> Result<String> {
    let root = &snapshot.root;
    if stamp(root)? != snapshot.fingerprint {
        bail!("文件已在其他位置更改，请刷新差异后重试。");
    }
    let paths = |requested: &Option<String>| -> Result<Vec<String>> {
        match requested {
            Some(path) => {
                safe_path(path)?;
                let file = snapshot
                    .files
                    .iter()
                    .find(|f| &f.path == path)
                    .context("文件不在当前审查中")?;
                Ok(file
                    .old_path
                    .iter()
                    .cloned()
                    .chain(std::iter::once(path.clone()))
                    .collect())
            }
            None => Ok(status(root)?
                .into_iter()
                .flat_map(|e| e.old_path.into_iter().chain(std::iter::once(e.path)))
                .collect()),
        }
    };
    let run_paths = |args: &[&str], p: Vec<String>| -> Result<()> {
        if p.is_empty() {
            return Ok(());
        }
        let mut cmd = command(root);
        cmd.args(args).arg("--").args(p);
        checked(process::run(&mut cmd, None, Duration::from_secs(30))?)?;
        Ok(())
    };
    let stage_paths = |requested: &Option<String>| -> Result<Vec<String>> {
        let wanted = requested.as_ref().map(|_| paths(requested)).transpose()?;
        Ok(status(root)?
            .into_iter()
            .filter(|e| e.x == '?' || e.y != ' ')
            .filter(|e| {
                wanted.as_ref().is_none_or(|paths| {
                    paths.contains(&e.path)
                        || e.old_path.as_ref().is_some_and(|p| paths.contains(p))
                })
            })
            .map(|e| e.path)
            .collect())
    };
    match mutation {
        Mutation::PullRequest(options) => return create_pull_request(snapshot, scope, options),
        Mutation::Stage(path) => run_paths(&["add"], stage_paths(path)?)?,
        Mutation::Unstage(path) => {
            let args = if resolve(root, "HEAD").is_ok() {
                vec!["restore", "--staged"]
            } else {
                vec!["rm", "--cached", "-r"]
            };
            let p = if path.is_none() {
                status(root)?
                    .into_iter()
                    .filter(|e| e.x != '?' && e.x != ' ')
                    .flat_map(|e| e.old_path.into_iter().chain(std::iter::once(e.path)))
                    .collect()
            } else {
                paths(path)?
            };
            run_paths(&args, p)?;
        }
        Mutation::Discard(path) => discard(snapshot, scope, path)?,
        Mutation::DiscardAll => {
            for file in &snapshot.files {
                discard(snapshot, scope, &file.path)?;
            }
        }
        Mutation::Hunk {
            path,
            index,
            reverse,
        } => {
            let file = snapshot
                .files
                .iter()
                .find(|f| &f.path == path)
                .context("找不到文件")?;
            if !scope.editable() || file.status != 'M' {
                bail!("此更改需要按文件暂存或取消暂存");
            }
            let hunk = file.hunks.get(*index).context("找不到差异块")?;
            let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
            if *reverse {
                args.push("--reverse");
            }
            let mut check = args.clone();
            check.push("--check");
            apply_stdin(root, &check, &hunk.patch)?;
            apply_stdin(root, &args, &hunk.patch)?;
        }
        Mutation::Commit {
            message,
            stage_all,
            branch,
        } => {
            if let Some(branch) = branch {
                if branch == "codex/" || branch.starts_with('-') {
                    bail!("请输入有效的分支名称");
                }
                git(root, &["check-ref-format", "--branch", branch])?;
                git(root, &["switch", "-c", branch])?;
            } else if snapshot.detached {
                bail!("请先为此次提交填写新分支名称");
            }
            if *stage_all {
                run_paths(&["add"], stage_paths(&None)?)?;
            }
            let generated = if message.trim().is_empty() {
                let names = git(root, &["diff", "--cached", "--name-only"])?;
                let files = names.lines().collect::<Vec<_>>();
                format!(
                    "Update {}{}\n\n{}",
                    files
                        .first()
                        .and_then(|s| Path::new(s).file_name())
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "workspace files".into()),
                    if files.len() > 1 {
                        " and related files"
                    } else {
                        ""
                    },
                    files
                        .iter()
                        .map(|f| format!("- Update {f}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            } else {
                message.clone()
            };
            git(root, &["commit", "-m", &generated])?;
        }
        Mutation::Push => {
            if snapshot.upstream.is_some() {
                git(root, &["push"])?;
            } else {
                let branch = git(root, &["symbolic-ref", "--short", "HEAD"])
                    .context("请先创建分支再推送")?;
                git(root, &["push", "--set-upstream", "origin", branch.trim()])?;
            }
        }
    }
    Ok(match mutation {
        Mutation::Push => "推送完成",
        Mutation::Commit { .. } => "提交完成",
        _ => "已更新更改",
    }
    .into())
}

fn apply_stdin(root: &Path, args: &[&str], patch: &str) -> Result<()> {
    checked(process::run(
        command(root).args(args),
        Some(patch.as_bytes()),
        Duration::from_secs(30),
    )?)?;
    Ok(())
}

fn discard(snapshot: &Snapshot, scope: &Scope, path: &str) -> Result<()> {
    if !scope.editable() {
        bail!("历史差异不能撤销当前文件");
    }
    safe_path(path)?;
    let root = &snapshot.root;
    let state = status(root)?
        .into_iter()
        .find(|e| e.path == path)
        .context("文件状态已变化")?;
    if state.x == '?' || (state.x == 'A' && !matches!(scope, Scope::Unstaged)) {
        let target = root.join(path);
        if state.x == 'A' {
            git(root, &["rm", "--cached", "--", path])?;
        }
        // Back up untracked content for recovery instead of deleting it permanently.
        let git_dir = PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?.trim());
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let dest = git_dir
            .join("gpui-discarded")
            .join(stamp.to_string())
            .join(path);
        std::fs::create_dir_all(dest.parent().context("无效备份路径")?)?;
        if let Err(error) = std::fs::rename(&target, &dest) {
            if error.kind() != std::io::ErrorKind::CrossesDevices {
                return Err(error.into());
            }
            // Linked worktrees may live on a different volume from their Git
            // metadata. Preserve a recoverable copy before removing the source.
            #[cfg(unix)]
            if std::fs::symlink_metadata(&target)?.file_type().is_symlink() {
                std::os::unix::fs::symlink(std::fs::read_link(&target)?, &dest)?;
                std::fs::remove_file(&target)?;
                return Ok(());
            }
            std::fs::copy(&target, &dest)?;
            std::fs::File::open(&dest)?.sync_all()?;
            std::fs::remove_file(&target)?;
        }
    } else {
        let mut args = vec!["restore"];
        if !matches!(scope, Scope::Unstaged) {
            args.extend(["--source=HEAD", "--staged"]);
        }
        args.extend(["--worktree", "--", path]);
        if !matches!(scope, Scope::Unstaged)
            && let Some(old) = state.old_path.as_deref()
        {
            args.push(old);
        }
        git(root, &args)?;
    }
    Ok(())
}

pub fn apply_command(files: &[FileDiff]) -> String {
    let patch = files.iter().map(|f| f.patch.as_str()).collect::<String>();
    let mut delimiter = "GPUI_REVIEW_PATCH".to_owned();
    while patch.lines().any(|l| l == delimiter) {
        delimiter.push('_');
    }
    format!("git apply <<'{delimiter}'\n{patch}{delimiter}\n")
}

#[cfg(test)]
mod tests;
