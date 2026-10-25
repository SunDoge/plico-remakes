use peek::{
    Application,
    git::{LineKind, Mode, Repository, parse_numstat, parse_patch},
    service::GitService,
};
use plico_dev::services::HostService;
use plico_runtime::{
    ServiceView,
    testing::{Command, Driver},
};
use std::{
    path::{Path, PathBuf},
    process::Command as Process,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "peek-plico-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(
            path.join("src/main.rs"),
            "fn main() {\n    println!(\"first\");\n}\n",
        )
        .unwrap();
        git(&path, &["add", "."]);
        commit(&path, "Initial commit");
        std::fs::write(
            path.join("src/main.rs"),
            "fn main() {\n    println!(\"second\");\n}\n",
        )
        .unwrap();
        std::fs::write(path.join("notes with spaces.md"), "# Notes\n").unwrap();
        git(&path, &["add", "."]);
        commit(&path, "Change greeting");
        std::fs::write(path.join("untracked.rs"), "// local draft\n").unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn git(path: &Path, args: &[&str]) -> String {
    let out = Process::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn commit(path: &Path, message: &str) {
    git(
        path,
        &[
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=ada@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}
#[test]
fn git_reader_handles_real_commits_untracked_and_spaced_paths() {
    let repo = Fixture::new();
    let reader = Repository::open(&repo.0).unwrap();
    let mut state = reader.history().unwrap();
    assert_eq!(state.commits.len(), 2);
    assert_eq!(state.commits[0].subject, "Change greeting");
    reader.browse(&mut state, "", Mode::Changes, "").unwrap();
    assert!(
        state
            .files
            .iter()
            .any(|f| f.path == "untracked.rs" && f.untracked)
    );
    let head = state.commits[0].hash.clone();
    reader.browse(&mut state, &head, Mode::Changes, "").unwrap();
    assert!(state.files.iter().any(|f| f.path == "notes with spaces.md"));
    assert!(
        state
            .document
            .rows
            .iter()
            .any(|l| l.kind == LineKind::Added && l.text.contains("second"))
    );
    assert!(
        state
            .document
            .rows
            .iter()
            .any(|l| l.kind == LineKind::Removed && l.text.contains("first"))
    );
    assert_eq!(
        state
            .files
            .iter()
            .find(|f| f.path == "notes with spaces.md")
            .unwrap()
            .status,
        "A"
    );
    reader
        .browse(&mut state, &head, Mode::Files, "src/main.rs")
        .unwrap();
    assert_eq!(state.opened_file, "src/main.rs");
    assert!(
        state
            .document
            .rows
            .iter()
            .any(|l| l.text.contains("second"))
    );
    assert!(!state.files.iter().any(|f| f.path == "untracked.rs"));
}
#[test]
fn diff_headers_do_not_swallow_code_starting_with_three_signs() {
    let rows = parse_patch("@@ -1 +1 @@\n---old\n+++new\n", "a.txt");
    assert_eq!(rows[1].text, "--old");
    assert_eq!(rows[1].kind, LineKind::Removed);
    assert_eq!(rows[2].text, "++new");
    assert_eq!(rows[2].kind, LineKind::Added);
    let stats = parse_numstat(b"2\t1\tfile\twith-tab.rs\0-\t-\timage.png\0");
    assert_eq!(stats[0].path, "file\twith-tab.rs");
    assert!(stats[1].binary);
}
#[test]
fn semantic_ui_switches_commits_files_and_theme_through_the_real_worker() {
    let repo = Fixture::new();
    let mut service = GitService::new(&repo.0);
    service.wait_ready(Duration::from_secs(10)).unwrap();
    let before = service.snapshot();
    assert!(before.revision.is_empty());
    let mut ui = Driver::new(ServiceView::new(Application::default(), service), 1280, 800).unwrap();
    let click = |label: &str| Command::Click {
        role: "button".into(),
        label: label.into(),
        occurrence: None,
    };
    ui.execute(click("Change greeting")).unwrap();
    ui.source
        .view
        .services
        .wait_ready(Duration::from_secs(10))
        .unwrap();
    ui.source.request_rebuild();
    ui.execute(Command::Snapshot).unwrap();
    assert!(!ui.source.view.services.snapshot().revision.is_empty());
    assert!(
        ui.snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("# Notes"))
    );
    ui.execute(Command::Click {
        role: "checkbox".into(),
        label: "Viewed".into(),
        occurrence: Some(0),
    })
    .unwrap();
    assert!(
        !ui.snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("# Notes"))
    );
    ui.execute(click("notes with spaces.md")).unwrap();
    assert!(
        ui.snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("# Notes"))
    );
    ui.execute(click("Files")).unwrap();
    ui.source
        .view
        .services
        .wait_ready(Duration::from_secs(10))
        .unwrap();
    ui.source.request_rebuild();
    ui.execute(Command::Snapshot).unwrap();
    assert_eq!(ui.source.view.services.snapshot().mode, Mode::Files);
    ui.execute(click("src/main.rs")).unwrap();
    ui.source
        .view
        .services
        .wait_ready(Duration::from_secs(10))
        .unwrap();
    ui.source.request_rebuild();
    ui.execute(Command::Snapshot).unwrap();
    assert_eq!(
        ui.source.view.services.snapshot().opened_file,
        "src/main.rs"
    );
    ui.execute(Command::Focus {
        role: "textbox".into(),
        label: "Filter files".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::Text {
        text: "main".into(),
    })
    .unwrap();
    assert!(
        !ui.snapshot()
            .nodes
            .iter()
            .any(|node| node.label.as_deref() == Some("notes with spaces.md"))
    );
    ui.execute(click("Toggle theme")).unwrap();
    assert!(ui.source.view.app.dark);
    // Reading and navigating must never change the user's index or tracked files.
    assert_eq!(
        git(&repo.0, &["status", "--porcelain"]),
        "?? untracked.rs\n"
    );
}
