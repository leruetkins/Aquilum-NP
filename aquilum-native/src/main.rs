#![windows_subsystem = "windows"]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod icons;

use aquilum_core::{Core, CoreEvent, EventSink};
use iced::advanced::text::highlighter::{self, Format};
use iced::widget::{
    button, column, container, markdown, row, scrollable, text, text_editor, text_input,
    Space,
};
use iced::{
    border, keyboard, Alignment, Background, Border, Color, Element, Fill, Font,
    Length, Subscription, Task, Theme,
};
use std::ops::Range;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    LivePreview,
    SourceMode,
    ReadingView,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Normal,
    Heading,
    Quote,
    Fence,
    Code,
}

pub struct MdHighlighter {
    // states[i] = "строка i начинается внутри блока кода"
    states: Vec<bool>,
    current: usize,
}

impl highlighter::Highlighter for MdHighlighter {
    type Settings = ();
    type Highlight = Kind;
    type Iterator<'a> = std::iter::Once<(Range<usize>, Kind)>;

    fn new(_: &Self::Settings) -> Self {
        Self {
            states: vec![false],
            current: 0,
        }
    }

    fn update(&mut self, _: &Self::Settings) {}

    fn change_line(&mut self, line: usize) {
        self.states.truncate(line + 1);
        while self.states.len() <= line {
            self.states.push(false);
        }
        self.current = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let in_code = self.states.get(self.current).copied().unwrap_or(false);
        let t = line.trim_start();
        let is_fence = t.starts_with("```");

        let kind = if is_fence {
            Kind::Fence
        } else if in_code {
            Kind::Code
        } else if t.starts_with('#') {
            Kind::Heading
        } else if t.starts_with('>') {
            Kind::Quote
        } else {
            Kind::Normal
        };

        let next = if is_fence { !in_code } else { in_code };
        self.states.truncate(self.current + 1);
        self.states.push(next);
        self.current += 1;

        std::iter::once((0..line.len(), kind))
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

pub fn md_format(kind: &Kind, _theme: &Theme) -> Format<Font> {
    use iced::font::Weight;
    match kind {
        Kind::Heading => Format {
            color: Some(ACCENT_BLUE),
            font: Some(Font {
                weight: Weight::Bold,
                ..Font::DEFAULT
            }),
        },
        Kind::Quote => Format {
            color: Some(TEXT_MUTED),
            font: None,
        },
        Kind::Fence => Format {
            color: Some(TEXT_MUTED),
            font: Some(Font::MONOSPACE),
        },
        Kind::Code => Format {
            color: Some(Color::from_rgb(0.65, 0.89, 0.63)),
            font: Some(Font::MONOSPACE),
        },
        Kind::Normal => Format {
            color: None,
            font: None,
        },
    }
}

#[derive(Debug, Clone)]
pub enum FileNode {
    Folder {
        name: String,
        path: PathBuf,
        children: Vec<FileNode>,
    },
    File {
        name: String,
        path: PathBuf,
    },
}

impl FileNode {
    pub fn name(&self) -> &str {
        match self {
            Self::Folder { name, .. } => name,
            Self::File { name, .. } => name,
        }
    }
}

struct App {
    core: Arc<Core>,
    workspace_path: Option<PathBuf>,
    workspace_name: String,
    file_tree: Vec<FileNode>,
    expanded_folders: HashSet<PathBuf>,
    selected_note_path: Option<PathBuf>,
    search_query: String,

    // Единый монолитный буфер для всей заметки
    full_editor_content: text_editor::Content,

    // Скомпилированный Markdown для Reading View
    reading_view_items: Vec<markdown::Item>,

    view_mode: ViewMode,
    is_dirty: bool,
    status_message: String,
}

#[derive(Debug, Clone)]
enum Message {
    OpenWorkspaceDialog,
    WorkspaceOpened(Option<PathBuf>),
    SelectNote(PathBuf),
    ToggleFolder(PathBuf),
    NewNoteDialog,
    SaveCurrentNote,
    SetViewMode(ViewMode),
    CycleViewMode,

    FullEditorAction(text_editor::Action),

    MarkdownLinkClicked(markdown::Uri),
    SearchChanged(String),
    Event(iced::Event),
}

struct CoreSink;
impl EventSink for CoreSink {
    fn emit(&self, event: CoreEvent) {
        println!("[core event] {}", event.name());
    }
}

// Catppuccin Mocha / Obsidian Dark палитра цветов
const BG_BASE: Color = Color::from_rgb(0.118, 0.118, 0.180); // #1e1e2e
const BG_SIDEBAR: Color = Color::from_rgb(0.094, 0.094, 0.145); // #181825
const BG_ACTIVE_ITEM: Color = Color::from_rgba(0.537, 0.706, 0.980, 0.16); // #89b4fa29
const BG_HOVER_ITEM: Color = Color::from_rgba(0.537, 0.706, 0.980, 0.08);
const TEXT_PRIMARY: Color = Color::from_rgb(0.804, 0.839, 0.957); // #cdd6f4
const TEXT_MUTED: Color = Color::from_rgb(0.478, 0.514, 0.624); // #7f849c
const ACCENT_BLUE: Color = Color::from_rgb(0.537, 0.706, 0.980); // #89b4fa
const BORDER_COLOR: Color = Color::from_rgb(0.192, 0.196, 0.267); // #313244

const INITIAL_NOTE: &str = "# Welcome to Aquilum Native\n\nYour fast, distraction-free markdown knowledge base with **Live Preview**!\n\n## Inline Live Preview Demo\n- Click any code block to edit it inline with pixel-perfect alignment\n- Regular text can be freely selected, copied, and edited across multiple lines\n\n### Interactive Features\n- [x] Native Rust GUI with zero Chromium overhead (~45 MB RAM)\n- [x] Lucide SVG vector icons\n- [x] Unified text editing with multi-line selection\n- [x] Dedicated Obsidian-style code blocks\n\n```powershell\nping -t 5.180.82.115 | ForEach-Object {\n    \"$((Get-Date -Format 'HH:mm:ss'))  $_\"\n}\n```\n\n> Regular text and code blocks are cleanly organized without line-by-line boundaries.\n";

impl App {
    fn boot() -> (Self, Task<Message>) {
        let app_data_dir = dirs::data_dir()
            .map(|d| d.join("com.dmitriy.aquilum-app"))
            .unwrap_or_else(|| PathBuf::from("./data"));

        let _ = std::fs::create_dir_all(&app_data_dir);
        let sink: Arc<dyn EventSink> = Arc::new(CoreSink);
        let core = Core::open(&app_data_dir, sink);

        let mut app = Self {
            core,
            workspace_path: None,
            workspace_name: "No Vault".to_owned(),
            file_tree: Vec::new(),
            expanded_folders: HashSet::new(),
            selected_note_path: None,
            search_query: String::new(),
            full_editor_content: text_editor::Content::with_text(INITIAL_NOTE),
            reading_view_items: markdown::parse(INITIAL_NOTE).collect(),
            view_mode: ViewMode::LivePreview,
            is_dirty: false,
            status_message: "Obsidian Markdown Editor".to_owned(),
        };

        if let Ok(workspaces) = app.core.ui_state.list_workspaces() {
            if let Some(first) = workspaces.into_iter().next() {
                let path = PathBuf::from(&first.path);
                if path.exists() {
                    app.open_workspace(path);
                }
            }
        }

        (app, Task::none())
    }

    fn title(&self) -> String {
        let dirty_mark = if self.is_dirty { "● " } else { "" };
        let mode_mark = match self.view_mode {
            ViewMode::LivePreview => "[Live Preview]",
            ViewMode::SourceMode => "[Source Mode]",
            ViewMode::ReadingView => "[Reading View]",
        };
        let note_name = self
            .selected_note_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Aquilum".to_owned());

        format!("{}{} — {} {}", dirty_mark, note_name, self.workspace_name, mode_mark)
    }

    fn theme(&self) -> Theme {
        Theme::CatppuccinMocha
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen().map(Message::Event)
    }

    fn open_workspace(&mut self, path: PathBuf) {
        self.workspace_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Vault".to_owned());
        self.workspace_path = Some(path.clone());
        self.expanded_folders.insert(path.clone());
        self.reload_file_tree();

        if let Some(first_file) = self.first_file_in_tree(&self.file_tree) {
            self.open_note(&first_file);
        }
    }

    fn first_file_in_tree(&self, nodes: &[FileNode]) -> Option<PathBuf> {
        for node in nodes {
            match node {
                FileNode::File { path, .. } => return Some(path.clone()),
                FileNode::Folder { children, .. } => {
                    if let Some(p) = self.first_file_in_tree(children) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    fn reload_file_tree(&mut self) {
        let Some(ws) = &self.workspace_path else {
            self.file_tree.clear();
            return;
        };
        self.file_tree = Self::build_tree(ws);
        self.status_message = format!("Vault: {}", self.workspace_name);
    }

    fn build_tree(dir: &Path) -> Vec<FileNode> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut folders = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.starts_with('.') || name == "node_modules" || name == "target" || name == ".git" {
                continue;
            }

            if path.is_dir() {
                let children = Self::build_tree(&path);
                folders.push(FileNode::Folder {
                    name,
                    path,
                    children,
                });
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case("md")
                        || ext.eq_ignore_ascii_case("markdown")
                        || ext.eq_ignore_ascii_case("txt")
                    {
                        files.push(FileNode::File { name, path });
                    }
                }
            }
        }

        folders.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));
        files.sort_by(|a, b| a.name().to_lowercase().cmp(&b.name().to_lowercase()));

        let mut all = folders;
        all.extend(files);
        all
    }

    fn open_note(&mut self, path: &Path) {
        if let Ok(raw_content) = std::fs::read_to_string(path) {
            self.full_editor_content = text_editor::Content::with_text(&raw_content);
            self.reading_view_items = markdown::parse(&raw_content).collect();
            self.selected_note_path = Some(path.to_path_buf());
            self.is_dirty = false;
            let note_name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            self.status_message = format!("Opened {}", note_name);
        }
    }

    fn save_current_note(&mut self) {
        if let Some(path) = &self.selected_note_path {
            let text = self.full_editor_content.text();

            if let Ok(()) = std::fs::write(path, &text) {
                self.reading_view_items = markdown::parse(&text).collect();
                self.is_dirty = false;
                let note_name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                self.status_message = format!("Saved {} ({} bytes)", note_name, text.len());
            } else {
                self.status_message = "Failed to save file!".to_owned();
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenWorkspaceDialog => Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .set_title("Open Vault Folder")
                        .pick_folder()
                        .await
                        .map(|handle| handle.path().to_path_buf())
                },
                Message::WorkspaceOpened,
            ),
            Message::WorkspaceOpened(Some(path)) => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let _ = self.core.ui_state.resolve_workspace(&path.to_string_lossy(), now_ms);
                self.open_workspace(path);
                Task::none()
            }
            Message::WorkspaceOpened(None) => Task::none(),
            Message::ToggleFolder(path) => {
                if self.expanded_folders.contains(&path) {
                    self.expanded_folders.remove(&path);
                } else {
                    self.expanded_folders.insert(path);
                }
                Task::none()
            }
            Message::SelectNote(path) => {
                if self.is_dirty {
                    self.save_current_note();
                }
                self.open_note(&path);
                Task::none()
            }
            Message::NewNoteDialog => {
                if let Some(ws) = &self.workspace_path {
                    let mut i = 1;
                    let mut new_path;
                    loop {
                        let name = format!("Untitled {}.md", i);
                        new_path = ws.join(&name);
                        if !new_path.exists() {
                            break;
                        }
                        i += 1;
                    }
                    let default_text = format!(
                        "# {}\n\nStart writing here...",
                        new_path.file_stem().unwrap().to_string_lossy()
                    );
                    let _ = std::fs::write(&new_path, &default_text);
                    self.reload_file_tree();
                    self.open_note(&new_path);
                }
                Task::none()
            }
            Message::SaveCurrentNote => {
                self.save_current_note();
                Task::none()
            }
            Message::SetViewMode(mode) => {
                if self.view_mode != mode {
                    if mode == ViewMode::ReadingView {
                        let text = self.full_editor_content.text();
                        self.reading_view_items = markdown::parse(&text).collect();
                    }
                    self.view_mode = mode;
                }
                Task::none()
            }
            Message::CycleViewMode => {
                let next_mode = match self.view_mode {
                    ViewMode::LivePreview => ViewMode::SourceMode,
                    ViewMode::SourceMode => ViewMode::ReadingView,
                    ViewMode::ReadingView => ViewMode::LivePreview,
                };
                self.update(Message::SetViewMode(next_mode))
            }
            Message::FullEditorAction(action) => {
                if action.is_edit() {
                    self.is_dirty = true;
                }
                self.full_editor_content.perform(action);
                Task::none()
            }
            Message::MarkdownLinkClicked(url) => {
                self.status_message = format!("Link: {}", url);
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", &url])
                    .spawn();
                Task::none()
            }
            Message::SearchChanged(query) => {
                self.search_query = query;
                Task::none()
            }
            Message::Event(iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                ..
            })) => {

                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character("s")
                        | keyboard::Key::Character("S")
                        | keyboard::Key::Character("ы")
                        | keyboard::Key::Character("Ы") => {
                            self.save_current_note();
                            return Task::none();
                        }
                        keyboard::Key::Character("e")
                        | keyboard::Key::Character("E")
                        | keyboard::Key::Character("у")
                        | keyboard::Key::Character("У") => {
                            return Task::done(Message::CycleViewMode);
                        }
                        keyboard::Key::Character("o")
                        | keyboard::Key::Character("O")
                        | keyboard::Key::Character("щ")
                        | keyboard::Key::Character("Щ") => {
                            return Task::done(Message::OpenWorkspaceDialog);
                        }
                        keyboard::Key::Character("n")
                        | keyboard::Key::Character("N")
                        | keyboard::Key::Character("т")
                        | keyboard::Key::Character("Т") => {
                            return Task::done(Message::NewNoteDialog);
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::Event(_) => Task::none(),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let editor_pane = self.view_editor();

        row![sidebar, editor_pane]
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        let vault_header = row![
            icons::icon(icons::FOLDER, 16.0, ACCENT_BLUE),
            text(&self.workspace_name)
                .size(13)
                .color(TEXT_PRIMARY),
            Space::new().width(Fill),
            button(icons::icon(icons::PLUS, 14.0, TEXT_PRIMARY))
                .padding([4, 6])
                .style(clean_ghost_button)
                .on_press(Message::NewNoteDialog),
            button(icons::icon(icons::FOLDER_OPEN, 14.0, TEXT_PRIMARY))
                .padding([4, 6])
                .style(clean_ghost_button)
                .on_press(Message::OpenWorkspaceDialog),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .padding([4, 6]);

        let search_bar = text_input("Quick filter...", &self.search_query)
            .on_input(Message::SearchChanged)
            .size(12)
            .padding([6, 8])
            .style(obsidian_search_input);

        let mut elements: Vec<Element<'_, Message>> = Vec::new();
        self.render_tree_nodes(&self.file_tree, 0, &mut elements);

        let tree_scroll = scrollable(column(elements).spacing(1)).height(Fill);

        container(
            column![
                vault_header,
                search_bar,
                tree_scroll,
            ]
            .spacing(8)
            .padding(8),
        )
        .width(Length::Fixed(270.0))
        .height(Fill)
        .style(obsidian_sidebar_box)
        .into()
    }

    fn render_tree_nodes<'a>(
        &'a self,
        nodes: &'a [FileNode],
        depth: usize,
        acc: &mut Vec<Element<'a, Message>>,
    ) {
        for node in nodes {
            match node {
                FileNode::Folder {
                    name,
                    path,
                    children,
                } => {
                    let is_expanded = self.expanded_folders.contains(path);
                    let chevron = if is_expanded {
                        icons::icon(icons::CHEVRON_DOWN, 12.0, TEXT_MUTED)
                    } else {
                        icons::icon(icons::CHEVRON_RIGHT, 12.0, TEXT_MUTED)
                    };
                    let folder_svg = if is_expanded {
                        icons::icon(icons::FOLDER_OPEN, 14.0, ACCENT_BLUE)
                    } else {
                        icons::icon(icons::FOLDER, 14.0, ACCENT_BLUE)
                    };
                    let indent = (depth * 14) as f32;

                    let folder_row = row![
                        Space::new().width(Length::Fixed(indent)),
                        chevron,
                        folder_svg,
                        text(name)
                            .size(12)
                            .color(TEXT_PRIMARY),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center);

                    let folder_btn = button(folder_row)
                        .width(Fill)
                        .padding([4, 6])
                        .style(obsidian_tree_item(false))
                        .on_press(Message::ToggleFolder(path.clone()));

                    acc.push(folder_btn.into());

                    if is_expanded {
                        self.render_tree_nodes(children, depth + 1, acc);
                    }
                }
                FileNode::File { name, path } => {
                    if !self.search_query.is_empty()
                        && !name.to_lowercase().contains(&self.search_query.to_lowercase())
                    {
                        continue;
                    }

                    let is_selected = self
                        .selected_note_path
                        .as_ref()
                        .map(|p| p == path)
                        .unwrap_or(false);

                    let indent = (depth * 14 + 18) as f32;
                    let display_name = name.strip_suffix(".md").unwrap_or(name);

                    let file_svg = icons::icon(
                        icons::FILE_TEXT,
                        13.0,
                        if is_selected { ACCENT_BLUE } else { TEXT_MUTED },
                    );

                    let file_row = row![
                        Space::new().width(Length::Fixed(indent)),
                        file_svg,
                        text(display_name)
                            .size(12)
                            .color(if is_selected { ACCENT_BLUE } else { TEXT_PRIMARY }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center);

                    let file_btn = button(file_row)
                        .width(Fill)
                        .padding([4, 6])
                        .style(obsidian_tree_item(is_selected))
                        .on_press(Message::SelectNote(path.clone()));

                    acc.push(file_btn.into());
                }
            }
        }
    }

    fn view_editor(&self) -> Element<'_, Message> {
        let note_title = self
            .selected_note_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "No Note Selected".to_owned());

        let dirty_badge = if self.is_dirty {
            text("● Modified")
                .size(11)
                .color(Color::from_rgb(0.98, 0.70, 0.38))
        } else {
            text("Saved")
                .size(11)
                .color(TEXT_MUTED)
        };

        // Тройной переключатель режимов
        let live_btn = button(text("Live Preview").size(11))
            .padding([3, 8])
            .style(obsidian_mode_button(self.view_mode == ViewMode::LivePreview))
            .on_press(Message::SetViewMode(ViewMode::LivePreview));

        let source_btn = button(text("Source").size(11))
            .padding([3, 8])
            .style(obsidian_mode_button(self.view_mode == ViewMode::SourceMode))
            .on_press(Message::SetViewMode(ViewMode::SourceMode));

        let reading_btn = button(text("Reading").size(11))
            .padding([3, 8])
            .style(obsidian_mode_button(self.view_mode == ViewMode::ReadingView))
            .on_press(Message::SetViewMode(ViewMode::ReadingView));

        let mode_switcher = row![live_btn, source_btn, reading_btn]
            .spacing(4)
            .align_y(Alignment::Center);

        let save_btn = button(
            row![
                icons::icon(icons::SAVE, 13.0, Color::from_rgb(0.1, 0.1, 0.15)),
                text("Save").size(12),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .padding([4, 12])
        .style(obsidian_save_button)
        .on_press(Message::SaveCurrentNote);

        let header = row![
            icons::icon(icons::FILE_TEXT, 15.0, ACCENT_BLUE),
            text(note_title)
                .size(15)
                .color(TEXT_PRIMARY),
            Space::new().width(Fill),
            dirty_badge,
            mode_switcher,
            save_btn,
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding([8, 24]);

        let content_pane: Element<'_, Message> = match self.view_mode {
            ViewMode::LivePreview => self.view_live_preview(),
            ViewMode::SourceMode => self.view_source_mode(),
            ViewMode::ReadingView => self.view_reading_mode(),
        };

        let footer = row![
            text(&self.status_message)
                .size(11)
                .color(TEXT_MUTED),
            Space::new().width(Fill),
            text(format!("{} lines", self.full_editor_content.line_count()))
                .size(11)
                .color(TEXT_MUTED),
        ]
        .spacing(16)
        .padding([6, 24]);

        // Главный контейнер редактора: content_pane занимает height(Fill),
        // поэтому footer ВСЕГДА прижат к самому низу окна!
        container(
            column![
                header,
                container(content_pane).height(Fill).width(Fill),
                footer,
            ]
            .width(Fill)
            .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(obsidian_editor_box)
        .into()
    }

    fn view_live_preview(&self) -> Element<'_, Message> {
        text_editor(&self.full_editor_content)
            .placeholder("Write markdown here...")
            .font(Font::DEFAULT)
            .size(14.0)
            .highlight_with::<MdHighlighter>((), md_format)
            .on_action(Message::FullEditorAction)
            .style(obsidian_editor_style)
            .padding([16, 32])
            .height(Fill)
            .into()
    }

    fn view_source_mode(&self) -> Element<'_, Message> {
        text_editor(&self.full_editor_content)
            .placeholder("Write markdown here...")
            .font(Font::MONOSPACE)
            .size(13.0)
            .highlight_with::<MdHighlighter>((), md_format)
            .on_action(Message::FullEditorAction)
            .style(obsidian_editor_style)
            .padding([16, 32])
            .height(Fill)
            .into()
    }

    fn view_reading_mode(&self) -> Element<'_, Message> {
        let md_settings = markdown::Settings::with_text_size(
            14.0,
            markdown::Style::from(&Theme::CatppuccinMocha),
        );
        let rendered_md = markdown::view(&self.reading_view_items, md_settings)
            .map(Message::MarkdownLinkClicked);

        scrollable(
            container(rendered_md)
                .padding([16, 32])
                .width(Fill),
        )
        .height(Fill)
        .width(Fill)
        .into()
    }
}

// ---- Стилизация в духе Obsidian ----

fn obsidian_sidebar_box(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SIDEBAR)),
        border: Border {
            color: BORDER_COLOR,
            width: 1.0,
            radius: border::Radius::default(),
        },
        text_color: Some(TEXT_PRIMARY),
        ..container::Style::default()
    }
}

fn obsidian_editor_box(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_BASE)),
        border: Border::default(),
        text_color: Some(TEXT_PRIMARY),
        ..container::Style::default()
    }
}

fn obsidian_editor_style(_theme: &Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: BG_ACTIVE_ITEM,
    }
}

fn obsidian_tree_item(is_selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, border_color) = if is_selected {
            (BG_ACTIVE_ITEM, ACCENT_BLUE)
        } else {
            match status {
                button::Status::Hovered => (BG_HOVER_ITEM, Color::TRANSPARENT),
                button::Status::Pressed => (BG_ACTIVE_ITEM, Color::TRANSPARENT),
                _ => (Color::TRANSPARENT, Color::TRANSPARENT),
            }
        };

        button::Style {
            background: Some(Background::Color(bg)),
            text_color: TEXT_PRIMARY,
            border: Border {
                color: border_color,
                width: if is_selected { 1.5 } else { 0.0 },
                radius: border::Radius::from(4.0),
            },
            ..button::Style::default()
        }
    }
}

fn clean_ghost_button(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => BG_HOVER_ITEM,
        button::Status::Pressed => BG_ACTIVE_ITEM,
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT_PRIMARY,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: border::Radius::from(4.0),
        },
        ..button::Style::default()
    }
}

fn obsidian_mode_button(is_active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let bg = if is_active {
            Color::from_rgba(0.537, 0.706, 0.980, 0.22)
        } else {
            match status {
                button::Status::Hovered => BG_HOVER_ITEM,
                _ => Color::TRANSPARENT,
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if is_active { ACCENT_BLUE } else { TEXT_MUTED },
            border: Border {
                color: if is_active { ACCENT_BLUE } else { Color::TRANSPARENT },
                width: if is_active { 1.0 } else { 0.0 },
                radius: border::Radius::from(4.0),
            },
            ..button::Style::default()
        }
    }
}

fn obsidian_save_button(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.6, 0.75, 1.0),
        button::Status::Pressed => Color::from_rgb(0.45, 0.65, 0.9),
        _ => ACCENT_BLUE,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.1, 0.1, 0.15),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: border::Radius::from(4.0),
        },
        ..button::Style::default()
    }
}

fn obsidian_search_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => ACCENT_BLUE,
        text_input::Status::Hovered => Color::from_rgb(0.3, 0.35, 0.45),
        _ => BORDER_COLOR,
    };
    text_input::Style {
        background: Background::Color(Color::from_rgb(0.12, 0.12, 0.18)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: border::Radius::from(4.0),
        },
        icon: TEXT_MUTED,
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: BG_ACTIVE_ITEM,
    }
}
