//! Read-only Git access. No shell interpolation, mutating commands or GUI calls.
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
pub const MAX_DIFF_FILES: usize = 60;
pub const MAX_ROWS: usize = 20_000;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("{source}"))]
    Io { source: std::io::Error },
    #[snafu(display("git {operation} failed in {}", repo.display()))]
    Git { operation: String, repo: PathBuf },
    #[snafu(display("Preview exceeds the 8 MiB limit"))]
    Limit,
}
impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Changes,
    Files,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
    pub body: String,
    pub email: String,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub binary: bool,
    pub untracked: bool,
    pub status: String,
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    #[default]
    Context,
    Added,
    Removed,
    Hunk,
    File,
    Note,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Line {
    pub kind: LineKind,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: String,
    pub file: String,
    pub spans: Vec<Token>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub light: u32,
    pub dark: u32,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub rows: Vec<Line>,
    pub file_rows: BTreeMap<String, usize>,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(skip)]
    pub avatars: Arc<BTreeMap<String, plico_core::ImageAsset>>,
    pub repo: String,
    pub name: String,
    pub branch: String,
    pub commits: Arc<Vec<Commit>>,
    pub files: Arc<Vec<File>>,
    pub document: Arc<Document>,
    pub revision: String,
    pub mode: Mode,
    pub opened_file: String,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct Repository {
    root: PathBuf,
}
impl Repository {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let candidate = Self {
            root: path.to_owned(),
        };
        let root = candidate.text(&["rev-parse", "--show-toplevel"])?;
        Ok(Self {
            root: PathBuf::from(root.trim_end_matches(['\r', '\n'])).canonicalize()?,
        })
    }
    pub fn path(&self) -> &Path {
        &self.root
    }
    fn bytes(&self, args: &[&str]) -> Result<Vec<u8>, Error> {
        let mut child = Command::new("git")
            .args([
                "--no-pager",
                "-c",
                "color.ui=false",
                "-c",
                "core.quotepath=false",
            ])
            .args(args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut data = Vec::new();
        let read = child
            .stdout
            .take()
            .unwrap()
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut data);
        if read.is_err() || data.len() as u64 > OUTPUT_LIMIT {
            let _ = child.kill();
        }
        let status = child.wait()?;
        read?;
        if data.len() as u64 > OUTPUT_LIMIT {
            return Err(Error::Limit);
        }
        if !status.success() {
            return Err(Error::Git {
                operation: args.join(" "),
                repo: self.root.clone(),
            });
        }
        Ok(data)
    }
    fn text(&self, args: &[&str]) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(&self.bytes(args)?).into_owned())
    }
    pub fn history(&self) -> Result<Snapshot, Error> {
        // A repository with no commits still exposes its untracked files.
        let output = self.text(&[
            "log",
            "-100",
            "--date=short",
            "--format=%H%x00%h%x00%an%x00%ad%x00%s%x00%b%x00%ae%x00",
        ]);
        let commits = match output {
            Ok(output) => parse_history(&output),
            Err(_) if self.text(&["rev-parse", "--verify", "HEAD"]).is_err() => Vec::new(),
            Err(error) => return Err(error),
        };
        let branch = self
            .text(&["symbolic-ref", "--short", "-q", "HEAD"])
            .unwrap_or_else(|_| "detached HEAD".into());
        let name = self
            .root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(Snapshot {
            repo: self.root.to_string_lossy().into_owned(),
            name,
            branch: branch.trim().into(),
            commits: Arc::new(commits),
            ..Default::default()
        })
    }
    pub fn browse(
        &self,
        snapshot: &mut Snapshot,
        revision: &str,
        mode: Mode,
        requested_file: &str,
    ) -> Result<(), Error> {
        // Only use hashes already returned by Git. Never accept arbitrary rev/path expressions.
        if !revision.is_empty() && !snapshot.commits.iter().any(|c| c.hash == revision) {
            return Err(Error::Git {
                operation: "select unknown commit".into(),
                repo: self.root.clone(),
            });
        }
        let files = self.files(revision, mode)?;
        let opened = files
            .iter()
            .find(|f| f.path == requested_file)
            .or(files.first())
            .map(|f| f.path.clone())
            .unwrap_or_default();
        let mut document = Document::default();
        if mode == Mode::Changes {
            for file in files.iter().take(MAX_DIFF_FILES) {
                if document.rows.len() >= MAX_ROWS {
                    break;
                }
                document
                    .file_rows
                    .insert(file.path.clone(), document.rows.len());
                document.rows.push(Line {
                    kind: LineKind::File,
                    file: file.path.clone(),
                    text: file.path.clone(),
                    ..Default::default()
                });
                match self.patch(revision, file) {
                    Ok(patch) => document.rows.extend(parse_patch(&patch, &file.path)),
                    Err(error) => document.rows.push(Line {
                        kind: LineKind::Note,
                        text: error.to_string(),
                        ..Default::default()
                    }),
                }
            }
            if files.len() > MAX_DIFF_FILES || document.rows.len() > MAX_ROWS {
                document.rows.truncate(MAX_ROWS);
                document.rows.push(Line {
                    kind: LineKind::Note,
                    text: "Preview limited. Select Files to read an individual file.".into(),
                    ..Default::default()
                });
            }
        } else if !opened.is_empty() {
            match self.contents(revision, &opened) {
                Ok(content) if !content.contains('\0') => {
                    document.rows = content
                        .lines()
                        .take(MAX_ROWS)
                        .enumerate()
                        .map(|(i, line)| Line {
                            new: Some(i + 1),
                            text: line.replace('\t', "    "),
                            file: opened.clone(),
                            ..Default::default()
                        })
                        .collect();
                    if content.lines().count() > MAX_ROWS {
                        document.rows.push(Line {
                            kind: LineKind::Note,
                            text: "File preview limited to 20,000 lines.".into(),
                            ..Default::default()
                        });
                    }
                }
                Ok(_) => document.rows.push(Line {
                    kind: LineKind::Note,
                    text: "Binary file — text preview unavailable".into(),
                    ..Default::default()
                }),
                Err(error) => document.rows.push(Line {
                    kind: LineKind::Note,
                    text: error.to_string(),
                    ..Default::default()
                }),
            }
        }
        crate::highlight::highlight(&mut document);
        snapshot.files = Arc::new(files);
        snapshot.document = Arc::new(document);
        snapshot.revision = revision.into();
        snapshot.mode = mode;
        snapshot.opened_file = opened;
        snapshot.loading = false;
        snapshot.error = None;
        Ok(())
    }
    fn files(&self, rev: &str, mode: Mode) -> Result<Vec<File>, Error> {
        let head = self.text(&["rev-parse", "--verify", "HEAD"]).is_ok();
        let mut files = if mode == Mode::Files {
            let bytes = if rev.is_empty() {
                self.bytes(&[
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ])?
            } else {
                self.bytes(&["ls-tree", "-r", "--name-only", "-z", rev])?
            };
            bytes
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| File {
                    path: String::from_utf8_lossy(p).into_owned(),
                    ..Default::default()
                })
                .collect()
        } else if !rev.is_empty() {
            parse_numstat(&self.bytes(&[
                "show",
                "--format=",
                "--numstat",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                rev,
            ])?)
        } else if head {
            parse_numstat(&self.bytes(&[
                "diff",
                "HEAD",
                "--numstat",
                "-z",
                "--no-renames",
                "--no-ext-diff",
            ])?)
        } else {
            Vec::new()
        };
        if mode == Mode::Changes && (head || !rev.is_empty()) {
            let statuses = if rev.is_empty() {
                self.bytes(&[
                    "diff",
                    "HEAD",
                    "--name-status",
                    "-z",
                    "--no-renames",
                    "--no-ext-diff",
                ])?
            } else {
                self.bytes(&[
                    "show",
                    "--format=",
                    "--name-status",
                    "-z",
                    "--no-renames",
                    "--no-ext-diff",
                    rev,
                ])?
            };
            let fields: Vec<_> = statuses.split(|byte| *byte == 0).collect();
            for pair in fields.as_chunks::<2>().0 {
                let path = String::from_utf8_lossy(pair[1]);
                if let Some(file) = files.iter_mut().find(|file| file.path == path) {
                    file.status = String::from_utf8_lossy(pair[0]).into_owned();
                }
            }
        }
        if rev.is_empty() {
            for path in self
                .bytes(&["ls-files", "--others", "--exclude-standard", "-z"])?
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
            {
                let path = String::from_utf8_lossy(path).into_owned();
                if let Some(file) = files.iter_mut().find(|f| f.path == path) {
                    file.untracked = true;
                } else {
                    files.push(File {
                        path,
                        untracked: true,
                        ..Default::default()
                    });
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files.dedup_by(|a, b| a.path == b.path);
        Ok(files)
    }
    fn contents(&self, rev: &str, path: &str) -> Result<String, Error> {
        if rev.is_empty() {
            let candidate = self.root.join(path).canonicalize()?;
            if !candidate.starts_with(&self.root) {
                return Err(Error::Git {
                    operation: "read outside repository".into(),
                    repo: self.root.clone(),
                });
            }
            let mut data = String::new();
            std::fs::File::open(candidate)?
                .take(OUTPUT_LIMIT + 1)
                .read_to_string(&mut data)?;
            if data.len() as u64 > OUTPUT_LIMIT {
                return Err(Error::Limit);
            }
            Ok(data)
        } else {
            self.text(&["show", &format!("{rev}:{path}")])
        }
    }
    fn patch(&self, rev: &str, file: &File) -> Result<String, Error> {
        if rev.is_empty() && file.untracked {
            let data = self.contents("", &file.path)?;
            if data.contains('\0') {
                return Ok("Binary file".into());
            }
            return Ok(format!(
                "@@ -0,0 +1,{} @@\n{}",
                data.lines().count(),
                data.lines()
                    .map(|line| format!("+{line}\n"))
                    .collect::<String>()
            ));
        }
        if rev.is_empty() {
            self.text(&[
                "diff",
                "HEAD",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--unified=4",
                "--",
                &file.path,
            ])
        } else {
            self.text(&[
                "show",
                "--format=",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--unified=4",
                rev,
                "--",
                &file.path,
            ])
        }
    }
}

pub fn parse_history(output: &str) -> Vec<Commit> {
    let fields: Vec<_> = output.split('\0').collect();
    fields
        .as_chunks::<7>()
        .0
        .iter()
        .map(|f| Commit {
            hash: f[0].trim_start_matches('\n').into(),
            short: f[1].into(),
            author: f[2].into(),
            date: f[3].into(),
            subject: f[4].into(),
            body: f[5].trim().into(),
            email: f[6].into(),
        })
        .filter(|c| !c.hash.is_empty())
        .collect()
}
pub fn parse_numstat(bytes: &[u8]) -> Vec<File> {
    bytes
        .split(|b| *b == 0)
        .filter_map(|record| {
            let mut parts = record.splitn(3, |b| *b == b'\t');
            let a = std::str::from_utf8(parts.next()?).ok()?;
            let d = std::str::from_utf8(parts.next()?).ok()?;
            let path = String::from_utf8_lossy(parts.next()?).into_owned();
            Some(File {
                path,
                added: a.parse().unwrap_or(0),
                removed: d.parse().unwrap_or(0),
                binary: a == "-",
                untracked: false,
                status: String::new(),
            })
        })
        .collect()
}
pub fn parse_patch(patch: &str, path: &str) -> Vec<Line> {
    let (mut old, mut new) = (0, 0);
    let mut rows = Vec::new();
    for raw in patch.lines() {
        let kind = if raw.starts_with("@@") {
            let parts: Vec<_> = raw.split_whitespace().collect();
            old = parts
                .get(1)
                .and_then(|n| n.trim_start_matches('-').split(',').next()?.parse().ok())
                .unwrap_or(0);
            new = parts
                .get(2)
                .and_then(|n| n.trim_start_matches('+').split(',').next()?.parse().ok())
                .unwrap_or(0);
            LineKind::Hunk
        } else if (old == 0 && new == 0)
            && (raw.starts_with("+++")
                || raw.starts_with("---")
                || raw.starts_with("diff --git")
                || raw.starts_with("index "))
        {
            continue;
        } else if old == 0 && new == 0 {
            LineKind::Note
        } else if raw.starts_with('+') {
            LineKind::Added
        } else if raw.starts_with('-') {
            LineKind::Removed
        } else if raw.starts_with(' ') {
            LineKind::Context
        } else {
            LineKind::Note
        };
        let (o, n) = match kind {
            LineKind::Added => {
                let n = new;
                new += 1;
                (None, Some(n))
            }
            LineKind::Removed => {
                let o = old;
                old += 1;
                (Some(o), None)
            }
            LineKind::Context => {
                let (o, n) = (old, new);
                old += 1;
                new += 1;
                (Some(o), Some(n))
            }
            _ => (None, None),
        };
        let text = if matches!(
            kind,
            LineKind::Added | LineKind::Removed | LineKind::Context
        ) {
            raw.get(1..).unwrap_or("")
        } else {
            raw
        };
        rows.push(Line {
            kind,
            old: o,
            new: n,
            text: text.replace('\t', "    "),
            file: path.into(),
            ..Default::default()
        });
        if rows.len() >= MAX_ROWS {
            break;
        }
    }
    rows
}
