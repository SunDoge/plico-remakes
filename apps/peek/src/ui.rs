use crate::{
    git::{File, Line, LineKind, Mode, Snapshot},
    icons,
    service::GitCommand,
};
use plico_core::prelude::*;
use plico_dev::services::{ServiceApp, Services};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// Match Peek: 12px code font with the MyGo editor default 1.5 line-height.
const CODE_FONT_SIZE: u16 = 12;
const CODE_ROW_HEIGHT: f32 = 18.;

type Bridge = Services<Snapshot, GitCommand>;
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Application {
    pub dark: bool,
    pub mode: Mode,
    pub hide_tree: bool,
    pub hide_history: bool,
    open_repo: PopoverState,
    selected_file: String,
    observed_revision: Option<String>,
    observed_repo: String,
    #[serde(skip)]
    history_reveal: bool,
    viewed: BTreeSet<String>,
    collapsed: BTreeSet<String>,
    #[serde(skip)]
    reveal: Option<usize>,
    #[serde(skip)]
    tree_reveal: bool,
    #[serde(skip)]
    tree: TreeCache,
    #[serde(skip)]
    rows: RowsCache,
}
#[derive(Default)]
struct RowsCache {
    source: Option<std::sync::Arc<crate::git::Document>>,
    viewed: BTreeSet<String>,
    indices: std::sync::Arc<Vec<usize>>,
}

#[derive(Default)]
struct TreeCache {
    source: Option<std::sync::Arc<Vec<File>>>,
    query: String,
    folded: BTreeSet<String>,
    entries: Vec<TreeRow>,
}
struct TreeRow {
    path: String,
    name: String,
    depth: usize,
    file: Option<usize>,
}

impl ServiceApp for Application {
    type Snapshot = Snapshot;
    type Command = GitCommand;
    fn theme(&self) -> Theme {
        let mut theme = if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        if self.dark {
            theme.surface = 0x1e1e20;
            theme.panel = 0x272729;
            theme.control = 0x303033;
            theme.control_hover = 0x38383c;
            theme.selection = 0x414145;
            theme.border = 0x38383b;
        } else {
            theme.surface = 0xffffff;
            theme.panel = 0xefeff0;
            theme.control = 0xf9f9fa;
            theme.control_hover = 0xe4e4e7;
            theme.selection = 0xd9d9dc;
            theme.border = 0xdcdcdf;
        }
        theme.metrics.font_size = 14;
        theme
    }
    fn on_key(&mut self, key: &str, bridge: &mut Bridge) -> bool {
        match key {
            "F5" => {
                bridge.send(GitCommand::Refresh);
                true
            }
            "Escape" if !self.selected_file.is_empty() => {
                self.selected_file.clear();
                true
            }
            _ => false,
        }
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext, bridge: &mut Bridge) {
        let snapshot = bridge.snapshot_shared();
        let repo_changed = self.observed_repo != snapshot.repo;
        if repo_changed {
            self.mode = snapshot.mode;
            self.collapsed.clear();
            self.observed_repo.clone_from(&snapshot.repo);
        }
        if repo_changed || self.observed_revision.as_deref() != Some(snapshot.revision.as_str()) {
            self.selected_file = if snapshot.mode == Mode::Files {
                snapshot.opened_file.clone()
            } else {
                snapshot.files.first().map(|f| f.path.clone()).unwrap_or_default()
            };
            self.observed_revision = Some(snapshot.revision.clone());
            self.reveal = Some(0);
            self.tree_reveal = true;
            self.history_reveal = true;
        }
        if snapshot.mode == Mode::Files
            && !snapshot.loading
            && self.selected_file != snapshot.opened_file
        {
            self.selected_file.clone_from(&snapshot.opened_file);
            self.tree_reveal = true;
        }
        column(cx).width_percent(100.).height_percent(100.).gap(0).background(Color::Surface) + |cx| {
            self.toolbar(cx, &snapshot, bridge);
            if let Some(error) = &snapshot.error {
                text(cx, error).font_size(13).color(Color::Literal(0xc2413a)).padding(10)
                    .background(Color::Control);
            }
            row(cx).height(0.).grow(1.).align(Align::Stretch).gap(0) + |cx| {
                if !self.hide_tree {
                    self.file_tree(cx, &snapshot, bridge);
                }
                self.document(cx, &snapshot);
                if !self.hide_history {
                    self.history(cx, &snapshot, bridge);
                }
            };
            row(cx).height(28.).padding_x(14).gap(8).align(Align::Center).background(Color::Panel) + |cx| {
                text(cx, if snapshot.loading { "Loading repository…" } else { "Read-only" }).font_size(12).color(Color::Muted);
                text(cx, &snapshot.repo).font_size(12).color(Color::Muted).ellipsis(true).min_width(0.)
                    .grow(1.);
                text(cx, "Plico · Rust").font_size(12).color(Color::Muted);
            };
        };
    }
}

impl Application {
    pub fn new(dark: bool, mode: Mode) -> Self {
        Self {
            dark,
            mode,
            ..Default::default()
        }
    }
    #[component]
    #[rustfmt::skip]
    fn toolbar(&mut self, cx: &mut UiContext, state: &Snapshot, bridge: &mut Bridge) {
        let path_input = cx.state(TextBuffer::new);
        row(cx).height(44.).padding_x(14).gap(8).align(Align::Center).background(Color::Panel).border(1., Color::Border)
        + |cx|
        {
            if tool(cx, &icons::PANEL_LEFT, "Toggle files") {
                self.hide_tree = !self.hide_tree;
            }
            column(cx).gap(1).min_width(80.) + |cx| {
                text(cx, if state.name.is_empty() { "Peek" } else { &state.name }).font_size(17).weight(650).ellipsis(true);
                text(cx, "Repository").font_size(11).color(Color::Muted);
            };
            button(cx).icon(&icons::GIT_BRANCH).text(&state.branch).background(Color::Control).radius(16.)
                .padding_x(10);
            popover(cx, &mut self.open_repo, "Open").width(360.) + |cx| {
                text(cx, "Open repository").font_size(16).weight(600);
                text_input(cx, "Repository path", &path_input.read()).width_percent(100.);
                let path = path_input.read().text().unwrap_or_default();
                if button(cx).text("Open repository").disabled(path.trim().is_empty()).clicked() {
                    bridge.send(GitCommand::Open { path });
                    cx.close_popover();
                }
            };
            column(cx).grow(1.).children(|_cx| {});
            row(cx).gap(2).padding(3).radius(18.).background(Color::Selection) + |cx| {
                for (mode, label) in [(Mode::Changes, "Changes"), (Mode::Files, "Files")] {
                    cx.keyed(
                        if mode == Mode::Changes { "changes" } else { "files" },
                        |cx| {
                            if button(cx).text(label).disabled(state.loading).padding_x(16).radius(15.)
                                .border(0., Color::Border)
                                .background(if self.mode == mode { Color::Surface } else { Color::Selection })
                                .weight(if self.mode == mode { 600 } else { 400 }).clicked()
                            {
                                self.mode = mode;
                                bridge.send(GitCommand::Browse {
                                    revision: state.revision.clone(),
                                    mode,
                                    file: self.selected_file.clone(),
                                });
                            }
                        },
                    );
                }
            };
            column(cx).grow(1.).children(|_cx| {});
            if tool(cx, &icons::REFRESH_CW, "Refresh") {
                bridge.send(GitCommand::Refresh);
            }
            if tool(cx, if self.dark { &icons::SUN } else { &icons::MOON }, "Toggle theme") {
                self.dark = !self.dark;
            }
            button(cx).text("Review").disabled(true).radius(16.).padding_x(14);
            button(cx).text("Ship").disabled(true).radius(16.).padding_x(14);
            if tool(cx, &icons::PANEL_RIGHT, "Toggle history") {
                self.hide_history = !self.hide_history;
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn file_tree(&mut self, cx: &mut UiContext, state: &Snapshot, bridge: &mut Bridge) {
        let filter = cx.state(TextBuffer::new);
        let query = filter.read().text().unwrap_or_default().to_lowercase();
        let files_changed = self.tree.source.as_ref().is_none_or(|old| !std::sync::Arc::ptr_eq(old, &state.files));
        if files_changed || self.tree.query != query || self.tree.folded != self.collapsed {
            self.tree = TreeCache {
                source: Some(state.files.clone()),
                query: query.clone(),
                folded: self.collapsed.clone(),
                entries: tree_rows(&state.files, &query, &self.collapsed),
            };
        }
        let reveal = self.tree_reveal.then(|| {
            self.tree.entries.iter().position(|row| row.path == self.selected_file)
        }).flatten();
        self.tree_reveal = false;
        column(cx).width(320.).shrink(0.).background(Color::Panel).border(1., Color::Border).gap(0) + |cx| {
            row(cx).height(46.).padding_x(16).gap(6).align(Align::Center) + |cx| {
                text(
                    cx,
                    if self.mode == Mode::Changes { "Changed files" } else { "Files" },
                ).font_size(13).weight(650).color(Color::Muted).grow(1.);
                if self.mode == Mode::Changes {
                    text!(
                        cx,
                        "+{}",
                        state.files.iter().map(|f| f.added).sum::<usize>()
                    ).font_size(12).color(Color::Literal(0x2cad4a));
                    text!(
                        cx,
                        "−{}",
                        state.files.iter().map(|f| f.removed).sum::<usize>()
                    ).font_size(12).color(Color::Literal(0xde5354));
                } else {
                    text!(cx, "{}", state.files.len()).font_size(12).color(Color::Muted);
                }
            };
            virtual_list(cx, self.tree.entries.len(), 32.).fill().smooth_scroll(true).reveal(reveal).rows(
                |i| identity_key(&self.tree.entries[i].path),
                |cx, i| {
                    let entry = &self.tree.entries[i];
                    let selected = entry.file.is_some() && self.selected_file == entry.path;
                    let mut activate = false;
                    button_base(cx, entry.path.as_str()).selected(selected).disabled(state.loading).height(30.)
                        .padding_x(10).padding_left(10. + entry.depth as f32 * 13.).radius(6.)
                        .background(if selected { Color::Selection } else { Color::Panel })
                        .hover_background(Color::ControlHover).children(|cx| {
                        row(cx).gap(7).align(Align::Center).min_width(0.).width_percent(100.) + |cx| {
                            if entry.file.is_none() {
                                icon(
                                    cx,
                                    if self.collapsed.contains(&entry.path) {
                                        &icons::CHEVRON_RIGHT
                                    } else {
                                        &icons::CHEVRON_DOWN
                                    },
                                ).size(12., 12.).color(Color::Muted);
                                icon(cx, &icons::FOLDER).size(16., 16.).color(
                                    Color::Literal(if self.dark { 0x94a4b8 } else { 0x8a9aad }));
                            } else {
                                icon(cx, &icons::FILE_CODE).size(16., 16.)
                                    .color(Color::Literal(0x698ccd));
                            }
                            text(cx, &entry.name).font_size(14).ellipsis(true).min_width(0.).grow(1.);
                            if let Some(index) = entry.file {
                                let file = &state.files[index];
                                if self.mode == Mode::Changes {
                                    let (status, color) = if file.untracked {
                                        ("U", 0xd59120)
                                    } else {
                                        match file.status.as_str() {
                                            "A" => ("A", 0x2cad4a),
                                            "D" => ("D", 0xde5354),
                                            _ => ("M", 0xd59120),
                                        }
                                    };
                                    text(cx, status).font_size(11).weight(600)
                                        .color(Color::Literal(color));
                                }
                            }
                        };
                    }).response.clicked().then(|| activate = true);
                    if activate {
                        if entry.file.is_none() {
                            let path = entry.path.clone();
                            if !self.collapsed.remove(&path) {
                                self.collapsed.insert(path);
                            }
                        } else {
                            self.selected_file.clone_from(&entry.path);
                            self.viewed.remove(&format!(
                                "{}\0{}\0{}",
                                state.repo, state.revision, entry.path
                            ));
                            if self.mode == Mode::Files {
                                bridge.send(GitCommand::Browse {
                                    revision: state.revision.clone(),
                                    mode: Mode::Files,
                                    file: entry.path.clone(),
                                });
                            } else {
                                self.reveal =
                                    state.document.file_rows.get(&entry.path).copied();
                            }
                        }
                    }
                },
            );
            row(cx).padding(10).height(52.).align(Align::Center).gap(6) + |cx| {
                icon(cx, &icons::SEARCH).size(16., 16.).color(Color::Muted);
                text_input(cx, "Filter files", &filter.read()).placeholder("Filter").grow(1.).min_width(0.)
                    .height(30.).font_size(13);
            };
        };
    }
    #[component]
    #[rustfmt::skip]
    fn history(&mut self, cx: &mut UiContext, state: &Snapshot, bridge: &mut Bridge) {
        let reveal = self.history_reveal.then(|| {
            state.commits.iter().position(|c| c.hash == state.revision).map_or(0, |i| i + 1)
        });
        self.history_reveal = false;
        column(cx).width(360.).shrink(0.).background(Color::Panel).border(1., Color::Border).gap(0) + |cx| {
            row(cx).height(46.).padding_x(14).align(Align::Center) + |cx| {
                text(cx, "History").font_size(13).weight(650).color(Color::Muted).grow(1.);
                text!(cx, "{} commits", state.commits.len()).font_size(12).color(Color::Muted);
            };
            virtual_list(cx, state.commits.len() + 1, 64.).fill().smooth_scroll(true).reveal(reveal).rows(
                |i| {
                    if i == 0 { 0 } else { identity_key(&state.commits[i - 1].hash) }
                },
                |cx, i| {
                    let (label, author, detail, revision) = if i == 0 {
                        ("Uncommitted changes", "", "Working tree", "")
                    } else {
                        let c = &state.commits[i - 1];
                        (
                            c.subject.as_str(),
                            c.author.as_str(),
                            c.date.as_str(),
                            c.hash.as_str(),
                        )
                    };
                    button_base(cx, label).selected(state.revision == revision).disabled(state.loading)
                        .height(62.).padding_x(10).radius(7.)
                        .background(if state.revision == revision { Color::Selection } else { Color::Panel })
                        .hover_background(Color::ControlHover).children(|cx| {
                        row(cx).gap(10).align(Align::Center).min_width(0.).width_percent(100.) + |cx| {
                            let color = avatar_color(author);
                            column(cx).width(32.).height(32.).shrink(0.).radius(16.).clip(true)
                                .background(Color::Literal(color)).align(Align::Center)
                                .justify(Justify::Center) + |cx|
                            {
                                if i == 0 {
                                    icon(cx, &icons::CODE).size(16., 16.)
                                        .color(Color::Literal(0xffffff));
                                } else if let Some(asset) = state.avatars.get(
                                    &crate::avatars::normalize(&state.commits[i - 1].email)) {
                                    image(cx, asset).size(32., 32.);
                                } else {
                                    text!(
                                        cx,
                                        "{}",
                                        author
                                            .chars()
                                            .next()
                                            .unwrap_or('?')
                                            .to_uppercase()
                                    ).font_size(14).weight(600).color(Color::Literal(0xffffff));
                                }
                            };
                            column(cx).gap(4).min_width(0.).width(0.).grow(1.) + |cx| {
                                text(cx, label).font_size(14).weight(600).ellipsis(true)
                                    .min_width(0.).width_percent(100.);
                                text!(
                                    cx,
                                    "{}{}{}",
                                    detail,
                                    if author.is_empty() { "" } else { " · " },
                                    author
                                ).font_size(12).color(Color::Muted).ellipsis(true).min_width(0.)
                                    .width_percent(100.);
                            };
                        };
                    }).response.clicked().then(|| {
                        bridge.send(GitCommand::Browse {
                            revision: revision.into(),
                            mode: self.mode,
                            file: String::new(),
                        });
                    });
                },
            );
        };
    }
    #[component]
    #[rustfmt::skip]
    fn document(&mut self, cx: &mut UiContext, state: &Snapshot) {
        let review_scope = format!("{}\0{}", state.repo, state.revision);
        let document_changed = self.rows.source.as_ref()
            .is_none_or(|old| !std::sync::Arc::ptr_eq(old, &state.document));
        if document_changed || self.rows.viewed != self.viewed {
            let hidden: BTreeSet<_> = state.files.iter().filter(|file| {
                self.viewed.contains(&format!("{review_scope}\0{}", file.path))
            }).map(|file| file.path.as_str()).collect();
            self.rows = RowsCache {
                source: Some(state.document.clone()),
                viewed: self.viewed.clone(),
                indices: std::sync::Arc::new(
                    state.document.rows.iter().enumerate().filter(|(_, line)| {
                        line.kind == LineKind::File || !hidden.contains(line.file.as_str())
                    }).map(|(index, _)| index).collect(),
                ),
            };
        }
        let selected = state.commits.iter().find(|c| c.hash == state.revision);
        let title = selected.map(|c| c.subject.as_str()).unwrap_or("Uncommitted changes");
        column(cx).width(0.).grow(1.).min_width(260.).gap(0).clip(true).selectable(true) + |cx| {
            row(cx).height(46.).padding_x(16).gap(10).align(Align::Center).border(1., Color::Border) + |cx| {
                text(
                    cx,
                    if self.mode == Mode::Files { &state.opened_file } else { title },
                ).font_size(14).weight(650).ellipsis(true).min_width(0.).grow(1.);
                text!(cx, "{} files", state.files.len()).font_size(12).color(Color::Muted);
            };
            if let Some(commit) = selected.filter(|_| self.mode == Mode::Changes) {
                row(cx).height(65.).padding_x(16).gap(10).align(Align::Center) + |cx| {
                    column(cx).width(32.).height(32.).radius(16.).clip(true)
                        .background(Color::Literal(avatar_color(&commit.author))).align(Align::Center)
                        .justify(Justify::Center) + |cx|
                    {
                        if let Some(asset) =
                            state.avatars.get(&crate::avatars::normalize(&commit.email))
                        {
                            image(cx, asset).size(32., 32.);
                        } else {
                            text!(
                                cx,
                                "{}",
                                commit
                                    .author
                                    .chars()
                                    .next()
                                    .unwrap_or('?')
                                    .to_uppercase()
                            ).color(Color::Literal(0xffffff)).weight(600);
                        }
                    };
                    column(cx).grow(1.).gap(3) + |cx| {
                        text(cx, &commit.author).font_size(13).weight(600);
                        text(cx, &commit.email).font_size(12).color(Color::Muted);
                    };
                    text(cx, &commit.short).font_family(FontFamily::Monospace).font_size(12)
                        .color(Color::Muted);
                };
            }
            if let Some(commit) =
                selected.filter(|commit| self.mode == Mode::Changes && !commit.body.is_empty())
            {
                text(cx, &commit.body).font_size(14).padding_x(16).padding_y(12).max_height(160.);
            }
            if state.document.rows.is_empty() {
                column(cx).grow(1.).align(Align::Center).justify(Justify::Center).gap(10) + |cx| {
                    icon(cx, &icons::GIT_COMMIT_HORIZONTAL).size(34., 34.).color(Color::Muted);
                    text(cx, if state.loading { "Reading changes…" } else { "Your working tree is clean" }).font_size(17).weight(600);
                    text(cx, "Choose a commit from the history to inspect its changes.").font_size(13).color(Color::Muted);
                };
            } else {
                let rows = &state.document.rows;
                let indices = self.rows.indices.clone();
                let reveal = self.reveal.take().and_then(|row| indices.iter().position(|index| *index == row));
                virtual_list(cx, indices.len(), CODE_ROW_HEIGHT).fill().smooth_scroll(true).overscan(5)
                    .reveal(reveal).rows(|i| indices[i],
                        |cx, i| self.code_row(cx, &rows[indices[i]], &review_scope));
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn code_row(&mut self, cx: &mut UiContext, line: &Line, scope: &str) {
        let muted = Color::Literal(if self.dark { 0x7f8490 } else { 0x9699a2 });
        match line.kind {
            LineKind::File => {
                row(cx).height(CODE_ROW_HEIGHT).padding_x(12).gap(8).align(Align::Center).background(Color::Panel)
                + |cx|
                {
                    icon(cx, &icons::FILE_CODE).size(15., 15.).color(Color::Literal(0x698ccd));
                    text(cx, &line.text).font_size(13).weight(650).ellipsis(true).min_width(0.).grow(1.);
                    let key = format!("{scope}\0{}", line.file);
                    let mut viewed = self.viewed.contains(&key);
                    if checkbox(cx, "Viewed", &mut viewed).height(CODE_ROW_HEIGHT).min_height(0.).padding(0)
                        .font_size(12).changed()
                    {
                        if viewed {
                            self.viewed.insert(key);
                        } else {
                            self.viewed.remove(&key);
                        }
                    }
                };
            }
            LineKind::Hunk => {
                row(cx).height(CODE_ROW_HEIGHT).padding_x(14).gap(8).align(Align::Center).background(Color::Panel)
                + |cx|
                {
                    icon(cx, &icons::CHEVRON_DOWN).size(12., 12.).color(Color::Muted);
                    text(cx, &line.text).font_family(FontFamily::Monospace).font_size(12).color(Color::Muted)
                        .ellipsis(true).min_width(0.).grow(1.);
                };
            }
            LineKind::Note => {
                text(cx, &line.text).font_size(12).color(Color::Muted).padding_x(12).ellipsis(true)
                    .height(CODE_ROW_HEIGHT);
            }
            _ => {
                let bg = match line.kind {
                    LineKind::Added => {
                        if self.dark { 0x203b2c } else { 0xe9f7ec }
                    }
                    LineKind::Removed => {
                        if self.dark { 0x44282d } else { 0xffeeee }
                    }
                    _ => {
                        if self.dark { 0x1e1e20 } else { 0xffffff }
                    }
                };
                let spans: Vec<_> = line.spans.iter().map(|token| plico_core::TextSpan {
                    range: token.start..token.end,
                    weight: 400,
                    italic: false,
                    color: Some(Color::Literal(if self.dark { token.dark } else { token.light })),
                }).collect();
                row(cx).height(CODE_ROW_HEIGHT).gap(0).padding_x(4).align(Align::Center).background(Color::Literal(bg))
                + |cx|
                {
                    text!(
                        cx,
                        "{:>4}",
                        line.old.map(|n| n.to_string()).unwrap_or_default()
                    ).width(38.).font_family(FontFamily::Monospace).font_size(12).color(muted).ellipsis(true);
                    text!(
                        cx,
                        "{:>4}",
                        line.new.map(|n| n.to_string()).unwrap_or_default()
                    ).width(38.).font_family(FontFamily::Monospace).font_size(12).color(muted).ellipsis(true);
                    text(cx, &line.text).font_family(FontFamily::Monospace).font_size(CODE_FONT_SIZE).spans(&spans)
                        .ellipsis(true).min_width(0.).width(0.).grow(1.);
                };
            }
        }
    }
}
#[component]
fn tool(cx: &mut UiContext, asset: &SvgAsset, label: &str) -> bool {
    button(cx)
        .icon(asset)
        .aria_label(label)
        .width(32.)
        .height(32.)
        .padding(0)
        .radius(8.)
        .background(Color::Panel)
        .border(0., Color::Border)
        .hover_background(Color::ControlHover)
        .clicked()
}
fn avatar_color(author: &str) -> u32 {
    const COLORS: [u32; 6] = [0xb7a942, 0x6b94cf, 0x8aa15e, 0xb47d96, 0x8d7bc3, 0x699e9c];
    COLORS[identity_key(author) as usize % COLORS.len()]
}
pub fn identity_key(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
fn tree_rows(files: &[File], query: &str, collapsed: &BTreeSet<String>) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, file) in files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.path.to_lowercase().contains(query))
    {
        let parts: Vec<_> = file.path.split('/').collect();
        let mut path = String::new();
        let mut hidden = false;
        for (depth, part) in parts.iter().enumerate() {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            let leaf = depth + 1 == parts.len();
            if leaf || seen.insert(path.clone()) {
                rows.push(TreeRow {
                    path: path.clone(),
                    name: (*part).into(),
                    depth,
                    file: leaf.then_some(index),
                });
            }
            if !leaf && collapsed.contains(&path) {
                hidden = true;
                break;
            }
        }
        if hidden {
            continue;
        }
    }
    rows
}
