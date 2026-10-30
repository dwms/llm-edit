//! Git trees in and out of the merge commands.
//!
//! The semantic merger works on text, but a git tree also holds
//! submodule pointers (mode 160000), symlinks (120000) and binary
//! blobs. Those never enter the merger: `read_snapshot` keeps them as
//! (mode, object id) entries, `merge_entry` resolves them the way git
//! does, and `apply_plan` writes every result into the index by object
//! id. Only paths whose result differs from ours are touched, so local
//! changes elsewhere stay out of the merge.

use crate::Tree;
use crate::cli::git_cmd;
use anyhow::{Context, Result, anyhow, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub mode: String,
    pub oid: String,
}

impl Entry {
    pub fn is_regular_file(&self) -> bool {
        self.mode == "100644" || self.mode == "100755"
    }

    fn is_gitlink(&self) -> bool {
        self.mode == "160000"
    }
}

/// Every entry of a commit's tree, plus the UTF-8 contents of its
/// regular files (the merger's input).
pub struct Snapshot {
    pub entries: BTreeMap<PathBuf, Entry>,
    pub text: Tree,
}

pub fn read_snapshot(rev: &str) -> Result<Snapshot> {
    let listing = git_cmd(&["ls-tree", "-r", "-z", "--full-tree", rev])?;
    let mut entries = BTreeMap::new();
    let mut blobs: Vec<(PathBuf, String)> = Vec::new();
    for record in listing.split('\0').filter(|r| !r.is_empty()) {
        let (meta, path) = record
            .split_once('\t')
            .ok_or_else(|| anyhow!("unexpected ls-tree record: {:?}", record))?;
        let mut fields = meta.split(' ');
        let (Some(mode), Some(_kind), Some(oid)) = (fields.next(), fields.next(), fields.next())
        else {
            bail!("unexpected ls-tree record: {:?}", record);
        };
        let entry = Entry {
            mode: mode.to_string(),
            oid: oid.to_string(),
        };
        if entry.is_regular_file() {
            blobs.push((PathBuf::from(path), entry.oid.clone()));
        }
        entries.insert(PathBuf::from(path), entry);
    }

    let contents = read_blobs(blobs.iter().map(|(_, oid)| oid.as_str()))?;
    let text = blobs
        .into_iter()
        .zip(contents)
        .filter_map(|((path, _), bytes)| String::from_utf8(bytes).ok().map(|s| (path, s)))
        .collect();
    Ok(Snapshot { entries, text })
}

/// Contents of `oids`, in order, through one `git cat-file --batch`.
fn read_blobs<'a>(oids: impl Iterator<Item = &'a str>) -> Result<Vec<Vec<u8>>> {
    let request: String = oids.map(|oid| format!("{}\n", oid)).collect();
    let count = request.lines().count();
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("spawn git cat-file --batch")?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    // Feed requests from a thread: cat-file answers as it reads, and a
    // full stdout pipe would otherwise stall both sides.
    let writer = std::thread::spawn(move || stdin.write_all(request.as_bytes()));
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut header = String::new();
        stdout.read_line(&mut header)?;
        let size: usize = header
            .trim_end()
            .rsplit(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("git cat-file: unexpected header {:?}", header))?;
        let mut body = vec![0u8; size + 1];
        stdout.read_exact(&mut body)?;
        body.pop();
        out.push(body);
    }
    writer
        .join()
        .map_err(|_| anyhow!("git cat-file writer panicked"))??;
    if !child.wait()?.success() {
        bail!("git cat-file --batch failed");
    }
    Ok(out)
}

/// Store `text` as a blob; returns its object id.
pub fn write_blob(text: &str) -> Result<String> {
    Ok(
        run_with_stdin(&["hash-object", "-w", "--stdin"], text.as_bytes())?
            .trim()
            .to_string(),
    )
}

fn run_with_stdin(args: &[&str], input: &[u8]) -> Result<String> {
    let mut child = Command::new("git")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn git {}", args.join(" ")))?;
    child.stdin.take().expect("piped stdin").write_all(input)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What the merge does to one path, relative to ours.
#[derive(Clone, Debug)]
pub enum Change {
    Set(Entry),
    Delete,
    /// Unmerged: index stages 1-3 (base, ours, theirs) and, for text,
    /// the merged-with-markers content left on disk to resolve.
    Conflict {
        stages: [Option<Entry>; 3],
        worktree: Option<String>,
    },
}

/// Changes keyed by path. Paths absent from the plan keep ours.
pub type Plan = BTreeMap<PathBuf, Change>;

/// Three-way merge of one entry the way git does for content it can't
/// merge: equal sides agree, a side equal to base yields to the other,
/// and two different changes conflict.
pub fn merge_entry(
    base: Option<&Entry>,
    ours: Option<&Entry>,
    theirs: Option<&Entry>,
) -> Option<Change> {
    if ours == theirs || theirs == base {
        return None;
    }
    if ours == base {
        return Some(theirs.map_or(Change::Delete, |t| Change::Set(t.clone())));
    }
    Some(Change::Conflict {
        stages: [base.cloned(), ours.cloned(), theirs.cloned()],
        worktree: None,
    })
}

/// Write `plan` into the index and working tree on top of `ours`.
///
/// Refuses, before touching anything, when the index differs from
/// HEAD or when a path the plan changes has local edits or an
/// untracked file in the way: the same conditions `git merge` refuses.
pub fn apply_plan(plan: &Plan, ours: &Snapshot) -> Result<()> {
    if plan.is_empty() {
        return Ok(());
    }
    let staged = git_cmd(&["diff-index", "--cached", "--name-only", "-z", "HEAD"])?;
    if !staged.is_empty() {
        bail!(
            "the index has staged changes ({}); commit or stash them first",
            staged
                .split('\0')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let root = PathBuf::from(git_cmd(&["rev-parse", "--show-toplevel"])?.trim());
    let modified: BTreeSet<PathBuf> = git_cmd(&["diff-files", "--name-only", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect();
    let in_the_way: Vec<String> = plan
        .keys()
        .filter(|p| {
            modified.contains(*p)
                || (!ours.entries.contains_key(*p) && root.join(p).symlink_metadata().is_ok())
        })
        .map(|p| p.display().to_string())
        .collect();
    if !in_the_way.is_empty() {
        bail!(
            "local changes would be overwritten by the merge: {}; commit or stash them first",
            in_the_way.join(", ")
        );
    }

    let zero = "0".repeat(ours.entries.values().next().map_or(40, |e| e.oid.len()));
    let mut index_info = String::new();
    let mut checkout: Vec<&Path> = Vec::new();
    for (path, change) in plan {
        let p = path.display();
        match change {
            Change::Set(e) => {
                index_info.push_str(&format!("{} {}\t{}\n", e.mode, e.oid, p));
                if !e.is_gitlink() {
                    checkout.push(path);
                }
            }
            Change::Delete => index_info.push_str(&format!("0 {}\t{}\n", zero, p)),
            Change::Conflict { stages, .. } => {
                index_info.push_str(&format!("0 {}\t{}\n", zero, p));
                for (n, e) in stages.iter().enumerate() {
                    if let Some(e) = e {
                        index_info.push_str(&format!("{} {} {}\t{}\n", e.mode, e.oid, n + 1, p));
                    }
                }
            }
        }
    }
    let root_arg = root.to_string_lossy();
    run_with_stdin(
        &["-C", &root_arg, "update-index", "--index-info"],
        index_info.as_bytes(),
    )?;

    for (path, change) in plan {
        let abs = root.join(path);
        match change {
            Change::Delete if ours.entries.get(path).is_some_and(|e| !e.is_gitlink()) => {
                std::fs::remove_file(&abs).with_context(|| format!("remove {}", abs.display()))?;
                prune_empty_dirs(&root, path);
            }
            Change::Conflict {
                worktree: Some(text),
                ..
            } => {
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&abs, text).with_context(|| format!("write {}", abs.display()))?;
            }
            _ => {}
        }
    }
    if !checkout.is_empty() {
        let paths: String = checkout
            .iter()
            .map(|p| format!("{}\0", p.display()))
            .collect();
        run_with_stdin(
            &[
                "-C",
                &root_arg,
                "checkout-index",
                "-f",
                "-q",
                "-z",
                "--stdin",
            ],
            paths.as_bytes(),
        )?;
    }
    Ok(())
}

fn prune_empty_dirs(root: &Path, path: &Path) {
    let mut dir = path.parent();
    while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
        if std::fs::remove_dir(root.join(d)).is_err() {
            break;
        }
        dir = d.parent();
    }
}
