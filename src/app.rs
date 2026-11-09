//! A small, menu-less text editor.

use std::ops::Range;
use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Id, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    RichText, Stroke, TextEdit, Ui, UiBuilder, ViewportCommand, pos2,
    text::CCursor,
    text::CCursorRange,
    vec2,
    widgets::text_edit::{TextEditOutput, TextEditState},
};

use crate::text::{self, Encoding, Eol, TextStats, char_to_byte};
use crate::theme;

const APP_NAME: &str = "mrs";
const EDITOR_ID: &str = "mrs.editor";
const GUIDE_COLUMN: f32 = 80.0;
const BASE_FONT_SIZE: f32 = 15.0;

fn editor_id() -> Id {
    Id::unique(EDITOR_ID)
}

/// What to do after the "unsaved changes" question has been answered.
#[derive(Clone, Debug)]
enum Action {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Save,
    Discard,
    Cancel,
}

struct PendingCursor {
    start: usize,
    end: usize,
    focus: bool,
    scroll: bool,
}

struct Notice {
    title: Option<String>,
    text: String,
    until: f64,
}

struct FindState {
    open: bool,
    query: String,
    case_sensitive: bool,
    matches: Vec<Range<usize>>,
    current: usize,
    /// Scroll the editor to the current match on the next frame.
    scroll: bool,
    /// Put the caret into the search field on the next frame.
    focus: bool,
    /// Byte offset where the current search should start.
    anchor: usize,
    computed_revision: u64,
    computed_query: String,
    computed_case: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            current: 0,
            scroll: false,
            focus: false,
            anchor: 0,
            computed_revision: u64::MAX,
            computed_query: String::new(),
            computed_case: false,
        }
    }
}

impl FindState {
    fn ensure_matches(&mut self, text: &str, revision: u64) {
        if self.computed_revision == revision
            && self.computed_query == self.query
            && self.computed_case == self.case_sensitive
        {
            return;
        }
        self.matches = text::find_matches(text, &self.query, self.case_sensitive);
        self.computed_revision = revision;
        self.computed_query.clone_from(&self.query);
        self.computed_case = self.case_sensitive;
        if self.matches.is_empty() {
            self.current = 0;
        } else {
            self.current = self.current.min(self.matches.len() - 1);
        }
    }

    /// Index of the first match starting at or after `byte`.
    fn index_at_or_after(&self, byte: usize) -> usize {
        self.matches
            .partition_point(|m| m.start < byte)
            .min(self.matches.len().saturating_sub(1))
    }
}

struct GotoState {
    input: String,
    focus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteCommand {
    New,
    Quit,
    Save,
    SaveAs,
    Open,
    Find,
    Goto,
    ToggleGuide,
    ToggleStatusBar,
}

struct PaletteEntry {
    command: PaletteCommand,
    label: &'static str,
    hint: &'static str,
}

const PALETTE_COMMANDS: &[PaletteEntry] = &[
    PaletteEntry {
        command: PaletteCommand::New,
        label: "New File",
        hint: "Ctrl+N",
    },
    PaletteEntry {
        command: PaletteCommand::Open,
        label: "Open File…",
        hint: "Ctrl+O",
    },
    PaletteEntry {
        command: PaletteCommand::Save,
        label: "Save",
        hint: "Ctrl+S",
    },
    PaletteEntry {
        command: PaletteCommand::SaveAs,
        label: "Save As…",
        hint: "Ctrl+Shift+S",
    },
    PaletteEntry {
        command: PaletteCommand::Find,
        label: "Find…",
        hint: "Ctrl+F",
    },
    PaletteEntry {
        command: PaletteCommand::Goto,
        label: "Go to Line…",
        hint: "Ctrl+L",
    },
    PaletteEntry {
        command: PaletteCommand::ToggleGuide,
        label: "Toggle Vertical Line",
        hint: "",
    },
    PaletteEntry {
        command: PaletteCommand::ToggleStatusBar,
        label: "Toggle Status Bar",
        hint: "",
    },
    PaletteEntry {
        command: PaletteCommand::Quit,
        label: "Quit",
        hint: "Ctrl+Q",
    },
];

#[derive(Default)]
struct PaletteState {
    open: bool,
    query: String,
    selected: usize,
    focus: bool,
}

pub struct EditorApp {
    text: String,
    saved_text: String,
    path: Option<PathBuf>,
    encoding: Encoding,
    eol: Eol,

    stats: TextStats,
    stats_revision: u64,
    /// Bumped every time `text` changes.
    revision: u64,

    font_size: f32,

    cursor_char: usize,
    cursor_line: usize,
    cursor_col: usize,
    selection_len: usize,

    find: FindState,
    goto: Option<GotoState>,
    palette: PaletteState,
    show_guide: bool,
    show_status_bar: bool,
    show_help: bool,
    pending: Option<Action>,
    error: Option<String>,
    pending_cursor: Option<PendingCursor>,
    notice: Option<Notice>,

    focus_editor: bool,
    scroll_to_top: bool,
    allow_close: bool,
    modal_was_open: bool,
    restore_pending: bool,
    title: String,
    last_dir: Option<PathBuf>,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx
            .options_mut(|options| options.zoom_with_keyboard = false);

        let mut app = Self {
            text: String::new(),
            saved_text: String::new(),
            path: None,
            encoding: Encoding::Utf8,
            eol: Eol::Lf,
            stats: TextStats::compute(""),
            stats_revision: 0,
            revision: 0,
            font_size: BASE_FONT_SIZE,
            cursor_char: 0,
            cursor_line: 1,
            cursor_col: 1,
            selection_len: 0,
            find: FindState::default(),
            goto: None,
            palette: PaletteState::default(),
            show_guide: true,
            show_status_bar: true,
            show_help: false,
            pending: None,
            error: None,
            pending_cursor: None,
            notice: None,
            focus_editor: true,
            scroll_to_top: true,
            allow_close: false,
            modal_was_open: false,
            restore_pending: false,
            title: String::new(),
            last_dir: std::env::current_dir().ok(),
        };

        if let Some(path) = initial {
            app.load_path(path, &cc.egui_ctx);
        }
        app
    }

    fn is_modified(&self) -> bool {
        self.text != self.saved_text
    }

    fn file_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned())
    }

    fn window_title(&self) -> String {
        let marker = if self.is_modified() { "• " } else { "" };
        format!("{marker}{} — {APP_NAME}", self.file_name())
    }

    fn dialog_open(&self) -> bool {
        self.pending.is_some() || self.error.is_some() || self.show_help || self.palette.open
    }

    /// Give keyboard focus back to whatever was being edited before a dialog
    /// was opened.
    fn restore_focus(&mut self, ctx: &egui::Context) {
        if self.find.open {
            self.find.focus = true;
            ctx.memory_mut(|memory| memory.request_focus(Id::unique("find_input")));
        } else if let Some(goto) = &mut self.goto {
            goto.focus = true;
            ctx.memory_mut(|memory| memory.request_focus(Id::unique("goto_input")));
        } else {
            self.focus_editor = true;
            ctx.memory_mut(|memory| memory.request_focus(editor_id()));
        }
        ctx.request_repaint();
    }

    // ----------------------------------------------------------------- files

    fn request(&mut self, ctx: &egui::Context, action: Action) {
        if self.is_modified() {
            self.pending = Some(action);
        } else {
            self.run_action(ctx, action);
        }
    }

    fn run_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New => self.new_file(ctx),
            Action::Open => {
                if let Some(path) = self.pick_open() {
                    self.load_path(path, ctx);
                }
            }
            Action::OpenPath(path) => self.load_path(path, ctx),
            Action::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn new_file(&mut self, ctx: &egui::Context) {
        self.text.clear();
        self.saved_text.clear();
        self.path = None;
        self.encoding = Encoding::Utf8;
        self.eol = Eol::Lf;
        self.revision = self.revision.wrapping_add(1);
        self.stats = TextStats::compute("");
        self.stats_revision = self.revision;
        self.find = FindState::default();
        self.goto = None;
        self.pending_cursor = None;
        self.scroll_to_top = true;
        self.focus_editor = true;
        self.cursor_char = 0;
        self.cursor_line = 1;
        self.cursor_col = 1;
        self.selection_len = 0;
        self.reset_editor_state(ctx);
    }

    fn load_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        let path = absolute(&path);
        let result = std::fs::read(&path)
            .map_err(|err| format!("Cannot open “{}”.\n{err}", path.display()))
            .and_then(|bytes| {
                Encoding::decode(&bytes)
                    .map_err(|err| format!("Cannot open “{}”.\n{err}", path.display()))
            });

        match result {
            Ok((raw, encoding)) => {
                let (text, eol) = Eol::normalize(&raw);
                self.text = text;
                self.saved_text = self.text.clone();
                self.encoding = encoding;
                self.eol = eol;
                self.path = Some(path.clone());
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.revision = self.revision.wrapping_add(1);
                self.stats = TextStats::compute(&self.text);
                self.stats_revision = self.revision;
                self.find = FindState::default();
                self.goto = None;
                self.pending_cursor = None;
                self.scroll_to_top = true;
                self.focus_editor = true;
                self.cursor_char = 0;
                self.cursor_line = 1;
                self.cursor_col = 1;
                self.selection_len = 0;
                self.reset_editor_state(ctx);
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// Forget the caret position and the undo history, which belong to the
    /// previous document.
    fn reset_editor_state(&mut self, ctx: &egui::Context) {
        let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(0))));
        state.clear_undoer();
        state.store(ctx, editor_id());
    }

    fn save(&mut self, _ctx: &egui::Context) -> bool {
        let path = match self.path.clone() {
            Some(path) => path,
            None => match self.pick_save() {
                Some(path) => path,
                None => return false,
            },
        };
        let path = absolute(&path);

        let contents = self.eol.encode(&self.text);
        let bytes = self.encoding.encode(&contents);
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.last_dir = path.parent().map(Path::to_path_buf);
                self.path = Some(path);
                self.saved_text.clone_from(&self.text);
                true
            }
            Err(err) => {
                self.error = Some(format!("Cannot save “{}”.\n{err}", path.display()));
                false
            }
        }
    }

    fn pick_open(&mut self) -> Option<PathBuf> {
        let mut dialog = text_file_dialog();
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }
        let path = dialog.pick_file();
        if let Some(path) = &path {
            self.last_dir = path.parent().map(Path::to_path_buf);
        }
        path
    }

    fn pick_save(&mut self) -> Option<PathBuf> {
        let suggested = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled.txt".to_owned());

        let mut dialog = text_file_dialog().set_file_name(suggested);
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }
        let path = dialog.save_file();
        if let Some(path) = &path {
            self.last_dir = path.parent().map(Path::to_path_buf);
        }
        path
    }

    fn reset_zoom(&mut self) {
        self.font_size = BASE_FONT_SIZE;
    }

    fn zoom(&mut self, delta: f32) {
        self.font_size = (self.font_size + delta).clamp(8.0, 40.0);
    }

    // ------------------------------------------------------------- shortcuts

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested())
            && !self.allow_close
            && self.is_modified()
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(Action::Quit);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        #[derive(Clone, Copy)]
        enum Cmd {
            New,
            Open,
            Save,
            SaveAs,
            CopyPath,
            DeleteLine,
            Quit,
            Find,
            FindNext,
            FindPrev,
            Goto,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            Help,
        }

        let mut command = None;
        let mut wheel = 0.0;

        ctx.input_mut(|input| {
            let cmd = Modifiers::COMMAND;
            let shift = Modifiers::SHIFT;
            let mut hit = |modifiers: Modifiers, key: Key| {
                input.consume_shortcut(&KeyboardShortcut::new(modifiers, key))
            };

            if hit(cmd | shift, Key::S) {
                command = Some(Cmd::SaveAs);
            } else if hit(cmd | shift, Key::C) {
                command = Some(Cmd::CopyPath);
            } else if hit(cmd | shift, Key::K) {
                command = Some(Cmd::DeleteLine);
            } else if hit(cmd, Key::S) {
                command = Some(Cmd::Save);
            } else if hit(cmd, Key::N) {
                command = Some(Cmd::New);
            } else if hit(cmd, Key::O) {
                command = Some(Cmd::Open);
            } else if hit(cmd, Key::Q) {
                command = Some(Cmd::Quit);
            } else if hit(cmd, Key::F) {
                command = Some(Cmd::Find);
            } else if hit(cmd, Key::L) {
                command = Some(Cmd::Goto);
            } else if hit(cmd | shift, Key::F3) {
                command = Some(Cmd::FindPrev);
            } else if hit(cmd, Key::F3) {
                command = Some(Cmd::FindNext);
            } else if hit(cmd | shift, Key::Plus) || hit(cmd, Key::Plus) || hit(cmd, Key::Equals) {
                command = Some(Cmd::ZoomIn);
            } else if hit(cmd, Key::Minus) {
                command = Some(Cmd::ZoomOut);
            } else if hit(cmd, Key::Num0) {
                command = Some(Cmd::ZoomReset);
            } else if hit(Modifiers::NONE, Key::F1) {
                command = Some(Cmd::Help);
            } else if hit(Modifiers::NONE, Key::F3) {
                command = Some(Cmd::FindNext);
            } else if hit(shift, Key::F3) {
                command = Some(Cmd::FindPrev);
            } else if input.modifiers.command && input.smooth_scroll_delta.y != 0.0 {
                wheel = std::mem::take(&mut input.smooth_scroll_delta.y);
            }
        });

        if wheel != 0.0 {
            self.zoom(wheel.signum());
        }

        let Some(command) = command else { return };
        match command {
            Cmd::New => self.request(ctx, Action::New),
            Cmd::Open => self.request(ctx, Action::Open),
            Cmd::Save => {
                self.save(ctx);
            }
            Cmd::SaveAs => {
                if let Some(path) = self.pick_save() {
                    self.path = Some(path);
                    self.save(ctx);
                }
            }
            Cmd::CopyPath => self.copy_path(ctx),
            Cmd::DeleteLine => self.delete_line(ctx),
            Cmd::Quit => self.request(ctx, Action::Quit),
            Cmd::Find => self.open_find(ctx),
            Cmd::FindNext => self.find_step(1),
            Cmd::FindPrev => self.find_step(-1),
            Cmd::Goto => self.open_goto(),
            Cmd::ZoomIn => self.zoom(1.0),
            Cmd::ZoomOut => self.zoom(-1.0),
            Cmd::ZoomReset => self.reset_zoom(),
            Cmd::Help => self.show_help = !self.show_help,
        }
    }

    fn copy_path(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.path else {
            self.flash(
                ctx,
                "The document has not been saved yet — no path to copy.",
            );
            return;
        };
        let absolute = path.display().to_string();
        ctx.copy_text(absolute.clone());
        self.notice = Some(Notice {
            title: Some("Path copied to clipboard".to_owned()),
            text: absolute,
            until: ctx.input(|input| input.time) + 3.0,
        });
        ctx.request_repaint();
    }

    /// Delete the lines covered by the selection, or the line with the caret.
    fn delete_line(&mut self, ctx: &egui::Context) {
        if self.text.is_empty() {
            return;
        }

        let (sel_start, sel_end) = TextEditState::load(ctx, editor_id())
            .and_then(|state| state.cursor.char_range())
            .map(|range| {
                let a = range.primary.index.0;
                let b = range.secondary.index.0;
                (a.min(b), a.max(b))
            })
            .unwrap_or((self.cursor_char, self.cursor_char));

        let start_byte = char_to_byte(&self.text, sel_start);
        let end_byte = char_to_byte(&self.text, sel_end);

        let mut del_start = self.text[..start_byte]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let ends_at_line_boundary = sel_end > sel_start
            && (end_byte == self.text.len()
                || self.text.as_bytes().get(end_byte - 1) == Some(&b'\n'));

        let del_end = if ends_at_line_boundary {
            end_byte
        } else if let Some(offset) = self.text[end_byte..].find('\n') {
            end_byte + offset + 1
        } else {
            self.text.len()
        };

        // Removing the last line also removes the newline before it, so the
        // document does not keep a trailing empty line.
        if del_end == self.text.len() && del_start > 0 {
            del_start -= 1;
        }

        if del_start >= del_end {
            return;
        }

        let start = self.text[..del_start].chars().count();
        let end = start + self.text[del_start..del_end].chars().count();

        set_editor_selection(ctx, start, end);
        ctx.memory_mut(|memory| memory.request_focus(editor_id()));
        ctx.input_mut(|input| {
            input.events.push(egui::Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        });
        ctx.request_repaint();
    }

    fn flash(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.notice = Some(Notice {
            title: None,
            text: text.into(),
            until: ctx.input(|input| input.time) + 3.0,
        });
        ctx.request_repaint();
    }

    fn selected_char_range(&self, ctx: &egui::Context) -> Option<(usize, usize)> {
        let state = TextEditState::load(ctx, editor_id())?;
        let range = state.cursor.char_range()?;
        let start = range.primary.index.0.min(range.secondary.index.0);
        let end = range.primary.index.0.max(range.secondary.index.0);
        (start != end).then_some((start, end))
    }

    fn open_find(&mut self, ctx: &egui::Context) {
        let mut anchor = self.cursor_char;
        if !self.find.open {
            self.find.open = true;
            self.goto = None;

            // Pre-fill the search with a short, single-line selection.
            if let Some((start, end)) = self.selected_char_range(ctx)
                && end - start <= 200
            {
                let start_byte = char_to_byte(&self.text, start);
                let end_byte = char_to_byte(&self.text, end);
                if let Some(selection) = self.text.get(start_byte..end_byte) {
                    let selection = selection.trim_end_matches(['\n', '\r']);
                    if !selection.is_empty() && !selection.contains('\n') {
                        self.find.query = selection.to_owned();
                        anchor = start;
                    }
                }
            }

            self.find.computed_revision = u64::MAX;
        }
        self.find.ensure_matches(&self.text, self.revision);
        self.find.anchor = char_to_byte(&self.text, anchor);
        self.find.current = self.find.index_at_or_after(self.find.anchor);
        self.find.scroll = false;
        self.find.focus = true;
    }

    fn find_step(&mut self, direction: isize) {
        self.goto = None;
        self.find.ensure_matches(&self.text, self.revision);
        if self.find.matches.is_empty() {
            if !self.find.open {
                self.find.open = true;
                self.find.focus = true;
            }
            return;
        }
        let len = self.find.matches.len() as isize;
        self.find.current = (self.find.current as isize + direction).rem_euclid(len) as usize;
        self.find.anchor = self.find.matches[self.find.current].start;
        self.find.scroll = true;
    }

    fn open_goto(&mut self) {
        self.find.open = false;
        self.find.scroll = false;
        self.goto = Some(GotoState {
            input: self.cursor_line.to_string(),
            focus: true,
        });
    }

    // ------------------------------------------------------ command palette

    fn handle_palette_shortcut(&mut self, ctx: &egui::Context) {
        let hit = ctx.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::P,
            ))
        });
        if !hit {
            return;
        }
        if self.palette.open {
            self.palette.open = false;
        } else if !self.dialog_open() {
            self.open_palette(ctx);
        }
    }

    fn open_palette(&mut self, ctx: &egui::Context) {
        self.palette.open = true;
        self.palette.query.clear();
        self.palette.selected = 0;
        self.palette.focus = true;
        ctx.request_repaint();
    }

    fn run_palette_command(&mut self, ctx: &egui::Context, command: PaletteCommand) {
        self.palette.open = false;
        match command {
            PaletteCommand::New => self.request(ctx, Action::New),
            PaletteCommand::Quit => self.request(ctx, Action::Quit),
            PaletteCommand::Save => {
                self.save(ctx);
            }
            PaletteCommand::SaveAs => {
                if let Some(path) = self.pick_save() {
                    self.path = Some(path);
                    self.save(ctx);
                }
            }
            PaletteCommand::Open => self.request(ctx, Action::Open),
            PaletteCommand::Find => self.open_find(ctx),
            PaletteCommand::Goto => self.open_goto(),
            PaletteCommand::ToggleGuide => self.show_guide = !self.show_guide,
            PaletteCommand::ToggleStatusBar => self.show_status_bar = !self.show_status_bar,
        }
        ctx.request_repaint();
    }

    fn palette_ui(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }

        let id = Id::unique("command_palette");
        let input_id = Id::unique("palette_input");
        let width = (ctx.content_rect().width() - 80.0).clamp(320.0, 560.0);
        let needle = self.palette.query.trim().to_lowercase();
        let matches: Vec<usize> = PALETTE_COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, entry)| needle.is_empty() || entry.label.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();

        if matches.is_empty() {
            self.palette.selected = 0;
        } else {
            self.palette.selected = self.palette.selected.min(matches.len() - 1);
        }

        let mut chosen = None;
        let modal = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(Align2::CENTER_TOP, vec2(0.0, 72.0)))
            .backdrop_color(Color32::from_black_alpha(80))
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL_BG)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(10))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 12],
                        blur: 36,
                        spread: 0,
                        color: Color32::from_black_alpha(150),
                    })
                    .inner_margin(Margin::same(6)),
            )
            .show(ctx, |ui| {
                ui.set_width(width);

                let up = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp));
                let down = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown));
                let enter = ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter));

                if up {
                    self.palette.selected = self.palette.selected.saturating_sub(1);
                } else if down && !matches.is_empty() {
                    self.palette.selected = (self.palette.selected + 1).min(matches.len() - 1);
                }

                let response = ui.add(
                    TextEdit::singleline(&mut self.palette.query)
                        .id(input_id)
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(10, 9))
                        .font(FontId::proportional(14.5))
                        .hint_text("Type the name of a command"),
                );
                if response.changed() {
                    self.palette.selected = 0;
                }
                if self.palette.focus {
                    self.palette.focus = false;
                    response.request_focus();
                    select_all_text(ui.ctx(), input_id, &self.palette.query);
                } else if !response.has_focus() && ui.memory(|memory| memory.focused().is_none()) {
                    response.request_focus();
                }

                ui.add_space(2.0);
                let line = ui.available_rect_before_wrap();
                ui.painter()
                    .hline(line.x_range(), line.top(), Stroke::new(1.0, theme::BORDER));
                ui.add_space(5.0);

                if matches.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("No matching commands")
                            .size(12.0)
                            .color(theme::FAINT),
                    );
                    ui.add_space(8.0);
                } else {
                    for (position, &index) in matches.iter().enumerate() {
                        let entry = &PALETTE_COMMANDS[index];
                        let (rect, row) = ui.allocate_exact_size(
                            vec2(ui.available_width(), 30.0),
                            egui::Sense::click(),
                        );
                        if row.hovered() {
                            self.palette.selected = position;
                        }
                        if position == self.palette.selected {
                            ui.painter().rect_filled(
                                rect.shrink2(vec2(0.0, 1.0)),
                                egui::CornerRadius::same(6),
                                theme::HOVER_BG,
                            );
                        }
                        let painter = ui.painter();
                        painter.text(
                            rect.left_center() + vec2(10.0, 0.0),
                            Align2::LEFT_CENTER,
                            entry.label,
                            FontId::proportional(13.5),
                            theme::TEXT,
                        );
                        if !entry.hint.is_empty() {
                            painter.text(
                                rect.right_center() - vec2(10.0, 0.0),
                                Align2::RIGHT_CENTER,
                                entry.hint,
                                FontId::monospace(11.0),
                                theme::FAINT,
                            );
                        }
                        if row.clicked() {
                            chosen = Some(entry.command);
                        }
                    }
                }

                ui.add_space(4.0);
                ui.label(
                    RichText::new("Up/Down navigate     Enter run     Esc close")
                        .size(10.5)
                        .color(theme::FAINT),
                );

                if enter && !matches.is_empty() {
                    chosen = Some(PALETTE_COMMANDS[matches[self.palette.selected]].command);
                }
            });

        if let Some(command) = chosen {
            self.run_palette_command(ctx, command);
        } else if modal.should_close() {
            self.palette.open = false;
        }
    }

    // ------------------------------------------------------------------- ui

    fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(28.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::STATUS_BG)
                    .inner_margin(Margin::symmetric(14, 0)),
            )
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::BORDER));

                let wide = ui.ctx().content_rect().width() >= 760.0;
                ui.horizontal_centered(|ui| {
                    let (dot_rect, _) =
                        ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                    let dot_color = if self.is_modified() {
                        theme::MODIFIED
                    } else {
                        theme::FAINT
                    };
                    ui.painter()
                        .circle_filled(dot_rect.center(), 3.5, dot_color);

                    let name = self.file_name();
                    let label = egui::Label::new(RichText::new(name).size(11.5).color(
                        if self.is_modified() {
                            theme::TEXT
                        } else {
                            theme::MUTED
                        },
                    ))
                    .truncate()
                    .sense(egui::Sense::click());
                    let tooltip = self
                        .path
                        .as_ref()
                        .map(|path| {
                            format!(
                                "{}\n\nClick to copy the path (Ctrl+Shift+C)",
                                path.display()
                            )
                        })
                        .unwrap_or_else(|| "Unsaved document — save it to get a path".to_owned());
                    let response = ui
                        .add(label)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(tooltip);
                    if response.clicked() {
                        self.copy_path(ui.ctx());
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        status_item(
                            ui,
                            &format!("{:.0}%", zoom_percent(self.font_size)),
                            theme::FAINT,
                        );
                        separator(ui);
                        status_item(ui, self.encoding.label(), theme::MUTED);
                        separator(ui);
                        status_item(ui, self.eol.label(), theme::MUTED);
                        separator(ui);
                        if wide {
                            status_item(
                                ui,
                                &format!("{} chars", text::thousands(self.stats.chars)),
                                theme::MUTED,
                            );
                            separator(ui);
                            status_item(
                                ui,
                                &format!("{} words", text::thousands(self.stats.words)),
                                theme::MUTED,
                            );
                            separator(ui);
                        }
                        status_item(
                            ui,
                            &format!("{} lines", text::thousands(self.stats.lines)),
                            theme::MUTED,
                        );
                        if self.selection_len > 0 {
                            separator(ui);
                            status_item(
                                ui,
                                &format!("sel {}", text::thousands(self.selection_len)),
                                theme::ACCENT,
                            );
                        }
                        separator(ui);
                        status_item(
                            ui,
                            &format!("Ln {}, Col {}", self.cursor_line, self.cursor_col),
                            theme::TEXT,
                        );
                    });
                });
            });
    }

    fn prompt_bar(&mut self, ui: &mut Ui) {
        if !self.find.open && self.goto.is_none() {
            return;
        }

        if !self.dialog_open()
            && ui
                .ctx()
                .input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            if self.find.open {
                self.find.open = false;
                self.find.scroll = false;
            }
            self.goto = None;
            self.focus_editor = true;
            return;
        }

        egui::Panel::bottom("prompt_bar")
            .exact_size(38.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL_BG)
                    .inner_margin(Margin::symmetric(14, 0)),
            )
            .show(ui, |ui| {
                let rect = ui.max_rect();
                ui.painter()
                    .hline(rect.x_range(), rect.top(), Stroke::new(1.0, theme::BORDER));
                ui.horizontal_centered(|ui| {
                    if self.find.open {
                        self.find_bar_ui(ui);
                    } else {
                        self.goto_bar_ui(ui);
                    }
                });
            });
    }

    fn find_bar_ui(&mut self, ui: &mut Ui) {
        let input_id = Id::unique("find_input");
        let modal = self.dialog_open();
        let shift_enter =
            !modal && ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Enter));
        let enter = !modal && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter));

        ui.label(RichText::new("Find").size(12.0).color(theme::MUTED));

        let input_width = (ui.available_width() * 0.34).clamp(140.0, 320.0);
        let response = ui.add(
            TextEdit::singleline(&mut self.find.query)
                .id(input_id)
                .desired_width(input_width)
                .interactive(!modal)
                .margin(Margin::symmetric(8, 3))
                .font(FontId::proportional(13.0))
                .hint_text("type to search"),
        );

        if response.changed() {
            self.find.ensure_matches(&self.text, self.revision);
            self.find.current = self.find.index_at_or_after(self.find.anchor);
            self.find.scroll = !self.find.matches.is_empty();
        }

        if shift_enter {
            self.find_step(-1);
        } else if enter {
            self.find_step(1);
        }

        if self.find.focus {
            self.find.focus = false;
            response.request_focus();
            select_all_text(ui.ctx(), input_id, &self.find.query);
        }

        let (count, color) = if self.find.query.is_empty() {
            (String::new(), theme::FAINT)
        } else if self.find.matches.is_empty() {
            ("no matches".to_owned(), theme::ERROR)
        } else {
            (
                format!("{} of {}", self.find.current + 1, self.find.matches.len()),
                theme::MUTED,
            )
        };
        ui.label(RichText::new(count).size(11.5).color(color));

        let has_matches = !self.find.matches.is_empty();
        if ui
            .add_enabled(has_matches, egui::Button::new("Prev").small())
            .clicked()
        {
            self.find_step(-1);
        }
        if ui
            .add_enabled(has_matches, egui::Button::new("Next").small())
            .clicked()
        {
            self.find_step(1);
        }

        let case = ui
            .selectable_label(
                self.find.case_sensitive,
                RichText::new("Aa")
                    .size(12.0)
                    .color(if self.find.case_sensitive {
                        theme::ACCENT
                    } else {
                        theme::MUTED
                    }),
            )
            .on_hover_text("Match case");
        if case.clicked() {
            self.find.case_sensitive = !self.find.case_sensitive;
            self.find.ensure_matches(&self.text, self.revision);
            self.find.current = self.find.index_at_or_after(self.find.anchor);
            self.find.scroll = !self.find.matches.is_empty();
        }

        if ui.available_width() > 250.0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new("Enter next   Shift+Enter previous   Esc close")
                        .size(10.5)
                        .color(theme::FAINT),
                );
            });
        }
    }

    fn goto_bar_ui(&mut self, ui: &mut Ui) {
        let Some(mut goto) = self.goto.take() else {
            return;
        };
        let input_id = Id::unique("goto_input");
        let modal = self.dialog_open();
        let enter = !modal && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter));

        ui.label(RichText::new("Go to line").size(12.0).color(theme::MUTED));
        let response = ui.add(
            TextEdit::singleline(&mut goto.input)
                .id(input_id)
                .desired_width(72.0)
                .interactive(!modal)
                .margin(Margin::symmetric(8, 3))
                .font(FontId::proportional(13.0)),
        );
        ui.label(
            RichText::new(format!("of {}", text::thousands(self.stats.lines)))
                .size(11.5)
                .color(theme::FAINT),
        );

        if goto.focus {
            goto.focus = false;
            response.request_focus();
            select_all_text(ui.ctx(), input_id, &goto.input);
        }

        let mut close = false;
        if enter && let Ok(line) = goto.input.trim().parse::<usize>() {
            let start = self.stats.line_start(line);
            self.pending_cursor = Some(PendingCursor {
                start,
                end: start,
                focus: true,
                scroll: true,
            });
            close = true;
        }

        if close {
            self.focus_editor = true;
        } else {
            self.goto = Some(goto);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("Enter jump   Esc close")
                    .size(10.5)
                    .color(theme::FAINT),
            );
        });
    }

    fn editor_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ui, |ui| {
                let available = ui.available_rect_before_wrap();
                let margin = 20.0;
                let width = (available.width() - 2.0 * margin).clamp(240.0, 920.0);
                let height = (available.height() - 2.0 * margin).max(60.0);
                let panel_rect = egui::Rect::from_min_size(
                    pos2(available.center().x - width / 2.0, available.top() + margin),
                    vec2(width, height),
                );

                let painter = ui.painter();
                let shadow = egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 28,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                };
                painter.add(shadow.as_shape(panel_rect, egui::CornerRadius::same(10)));
                painter.rect_stroke(
                    panel_rect,
                    egui::CornerRadius::same(10),
                    Stroke::new(1.0, theme::BORDER),
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(panel_rect, egui::CornerRadius::same(10), theme::EDITOR_BG);

                let inner = panel_rect.shrink2(vec2(18.0, 12.0));
                let mut editor_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(inner)
                        .layout(Layout::top_down(Align::Min)),
                );
                editor_ui.set_clip_rect(inner.intersect(ui.clip_rect()));

                let modal = self.dialog_open();
                let visuals = &mut editor_ui.style_mut().visuals;
                visuals.widgets.inactive.fg_stroke.color = theme::SCROLL_HANDLE;
                visuals.widgets.hovered.fg_stroke.color = theme::SCROLL_HANDLE_HOVER;
                visuals.widgets.active.fg_stroke.color = theme::SCROLL_HANDLE_HOVER;

                let font_id = FontId::monospace(self.font_size);
                let row_height = ctx.fonts_mut(|fonts| fonts.row_height(&font_id));
                let rows = ((inner.height() / row_height).floor() as usize).max(3);

                if self.focus_editor {
                    self.focus_editor = false;
                    ctx.memory_mut(|memory| memory.request_focus(editor_id()));
                }

                let matches = if self.find.open {
                    self.find.matches.clone()
                } else {
                    Vec::new()
                };
                let current_match = self.find.current;
                let text_color = theme::TEXT;
                let layouter_color = text_color;
                let layouter_font = font_id.clone();
                let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                    ui.fonts_mut(|fonts| {
                        if matches.is_empty() {
                            fonts.layout(
                                buffer.as_str().to_owned(),
                                layouter_font.clone(),
                                layouter_color,
                                wrap_width,
                            )
                        } else {
                            fonts.layout_job(build_job(
                                buffer.as_str(),
                                &matches,
                                current_match,
                                &layouter_font,
                                layouter_color,
                                wrap_width,
                            ))
                        }
                    })
                };

                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .auto_shrink([false, false]);
                if self.scroll_to_top {
                    self.scroll_to_top = false;
                    scroll = scroll.vertical_scroll_offset(0.0);
                }

                let output = scroll
                    .show(&mut editor_ui, |ui| {
                        let output = TextEdit::multiline(&mut self.text)
                            .id(editor_id())
                            .font(font_id.clone())
                            .text_color(text_color)
                            .frame(egui::Frame::NONE)
                            .margin(Margin::ZERO)
                            .desired_width(f32::INFINITY)
                            .desired_rows(rows)
                            .lock_focus(true)
                            .interactive(!modal)
                            .layouter(&mut layouter)
                            .show(ui);
                        self.post_edit(ui, &output);
                        output
                    })
                    .inner;

                let guide_x = output.galley_pos.x - output.galley.rect.left()
                    + GUIDE_COLUMN * ctx.fonts_mut(|fonts| fonts.glyph_width(&font_id, 'M'));
                if self.show_guide && guide_x > inner.left() && guide_x < inner.right() {
                    ui.painter().with_clip_rect(inner).vline(
                        guide_x,
                        inner.y_range(),
                        Stroke::new(1.0, theme::GUIDE),
                    );
                }
            });
    }

    fn post_edit(&mut self, ui: &mut Ui, output: &TextEditOutput) {
        let ctx = ui.ctx().clone();

        if output.response.changed() {
            self.revision = self.revision.wrapping_add(1);
        }

        if let Some(pending) = self.pending_cursor.take() {
            set_editor_selection(&ctx, pending.start, pending.end);
            if pending.focus {
                ui.memory_mut(|memory| memory.request_focus(editor_id()));
            }
            if pending.scroll {
                let rect = cursor_rect(&output.galley, output.galley_pos, pending.end);
                ui.scroll_to_rect(rect.expand2(vec2(0.0, 32.0)), Some(Align::Center));
            }
        }

        if self.find.open && self.find.scroll && !self.find.matches.is_empty() {
            self.find.scroll = false;
            let found = self.find.matches[self.find.current].clone();
            let start = text::byte_to_char(&self.text, found.start);
            let end = text::byte_to_char(&self.text, found.end);
            set_editor_selection(&ctx, start, end);
            let rect = cursor_rect(&output.galley, output.galley_pos, end);
            ui.scroll_to_rect(rect.expand2(vec2(0.0, 40.0)), Some(Align::Center));
        }

        if let Some(state) = TextEditState::load(&ctx, editor_id())
            && let Some(range) = state.cursor.char_range()
        {
            self.cursor_char = range.primary.index.0;
            let (line, column) = self.stats.line_col(self.cursor_char);
            self.cursor_line = line;
            self.cursor_col = column;
            self.selection_len = range.primary.index.0.abs_diff(range.secondary.index.0);
        }
    }

    fn notice_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let Some(notice) = self.notice.as_ref() else {
            return;
        };
        let text = notice.text.clone();
        let title = notice.title.clone();
        let until = notice.until;
        if now >= until {
            self.notice = None;
            return;
        }

        let prompt_bar = if self.find.open || self.goto.is_some() {
            38.0
        } else {
            0.0
        };
        let status_bar = if self.show_status_bar { 28.0 } else { 0.0 };
        let max_width = (ctx.content_rect().width() - 80.0).max(240.0);

        egui::Area::new(Id::unique("notice"))
            .anchor(
                Align2::CENTER_BOTTOM,
                vec2(0.0, -(status_bar + prompt_bar + 14.0)),
            )
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme::PANEL_BG)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(max_width);
                        if let Some(title) = title {
                            ui.label(RichText::new(title).size(10.5).color(theme::MUTED));
                            ui.add_space(3.0);
                            let mut job = egui::text::LayoutJob::simple(
                                text,
                                FontId::monospace(11.5),
                                theme::TEXT,
                                max_width,
                            );
                            job.wrap.break_anywhere = true;
                            ui.label(job);
                        } else {
                            ui.label(RichText::new(text).size(12.0).color(theme::TEXT));
                        }
                    });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.error.is_some()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.error = None;
        }
        if let Some(message) = self.error.clone() {
            let modal = egui::Modal::new(Id::unique("error_modal")).show(ctx, |ui| {
                ui.set_width(430.0);
                ui.heading("Something went wrong");
                ui.add_space(8.0);
                ui.label(RichText::new(message).color(theme::TEXT));
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    primary_button(ui, "OK").clicked()
                })
                .inner
            });
            if modal.inner
                || modal.backdrop_response.clicked()
                || ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
            {
                self.error = None;
            }
        }

        if self.pending.is_some()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.pending = None;
        }

        if let Some(action) = self.pending.clone() {
            let mut choice = None;
            let modal = egui::Modal::new(Id::unique("confirm_modal")).show(ctx, |ui| {
                ui.set_width(400.0);
                ui.heading("Save changes?");
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("“{}” has unsaved changes.", self.file_name()))
                        .color(theme::MUTED),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, "Save").clicked() {
                        choice = Some(Choice::Save);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(Choice::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(Choice::Cancel);
                    }
                });
            });
            if modal.backdrop_response.clicked() {
                choice = Some(Choice::Cancel);
            }
            if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
                choice = Some(Choice::Save);
            }

            match choice {
                Some(Choice::Save) => {
                    if self.save(ctx) {
                        let action = self.pending.take().unwrap_or(action);
                        self.run_action(ctx, action);
                    } else {
                        self.pending = None;
                    }
                }
                Some(Choice::Discard) => {
                    self.pending = None;
                    self.run_action(ctx, action);
                }
                Some(Choice::Cancel) => self.pending = None,
                None => {}
            }
        }

        if self.show_help {
            let modal_width = (ctx.content_rect().width() - 100.0).clamp(300.0, 700.0);
            let modal = egui::Modal::new(Id::unique("help_modal")).show(ctx, |ui| {
                ui.set_width(modal_width);
                ui.heading("Keyboard shortcuts");
                ui.add_space(10.0);
                if modal_width >= 560.0 {
                    let (left, right) = SHORTCUTS.split_at(11);
                    ui.horizontal_top(|ui| {
                        shortcut_grid(ui, "shortcuts_left", left);
                        ui.add_space(22.0);
                        ui.vertical(|ui| shortcut_grid(ui, "shortcuts_right", right));
                    });
                } else {
                    shortcut_grid(ui, "shortcuts_all", SHORTCUTS);
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Ctrl+scroll zooms the text as well.")
                        .size(11.0)
                        .color(theme::FAINT),
                );
            });
            if modal.backdrop_response.clicked()
                || ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
                || ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::F1))
            {
                self.show_help = false;
            }
        }
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|input| input.raw.hovered_files.is_empty()) {
            return;
        }
        let rect = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::unique("drop_overlay"),
        ));
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(170));
        let card = egui::Rect::from_center_size(rect.center(), vec2(360.0, 110.0));
        painter.rect_filled(card, egui::CornerRadius::same(10), theme::PANEL_BG);
        painter.rect_stroke(
            card,
            egui::CornerRadius::same(10),
            Stroke::new(1.5, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
        painter.text(
            card.center() - vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            "Drop a file to open it",
            FontId::proportional(16.0),
            theme::TEXT,
        );
        painter.text(
            card.center() + vec2(0.0, 16.0),
            Align2::CENTER_CENTER,
            "Release to open the file",
            FontId::proportional(11.0),
            theme::FAINT,
        );
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_close_request(&ctx);

        let dropped: Option<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.request(&ctx, Action::OpenPath(path));
        }

        self.find.ensure_matches(&self.text, self.revision);

        self.handle_palette_shortcut(&ctx);
        if !self.dialog_open() {
            self.handle_shortcuts(&ctx);
        }

        if self.stats_revision != self.revision {
            self.stats = TextStats::compute(&self.text);
            self.stats_revision = self.revision;
        }

        let title = self.window_title();
        if title != self.title {
            self.title.clone_from(&title);
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
        }

        if self.show_status_bar {
            self.status_bar(ui);
        }
        self.prompt_bar(ui);
        self.editor_ui(ui);
        self.dialogs(&ctx);
        self.palette_ui(&ctx);
        self.drop_overlay(&ctx);
        self.notice_ui(&ctx);

        let modal_now = self.dialog_open();
        if self.modal_was_open && !modal_now {
            // The modal layer is still in effect for one more pass, so wait a
            // frame before handing focus back.
            self.restore_pending = true;
            ctx.request_repaint();
        } else if self.restore_pending {
            self.restore_pending = false;
            self.restore_focus(&ctx);
        }
        self.modal_was_open = modal_now;
    }
}

// ---------------------------------------------------------------- helpers

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn zoom_percent(font_size: f32) -> f32 {
    (font_size / BASE_FONT_SIZE * 100.0).round()
}

fn text_file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter(
            "Text files",
            &[
                "txt", "md", "markdown", "rs", "toml", "json", "yaml", "yml", "log", "csv", "ini",
                "conf", "cfg", "sh", "py", "c", "h", "cpp", "html", "css", "js", "xml",
            ],
        )
        .add_filter("All files", &["*"])
}

fn status_item(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(RichText::new(text).size(11.5).color(color));
}

fn separator(ui: &mut Ui) {
    ui.label(RichText::new("·").size(10.0).color(theme::FAINT));
}

fn primary_button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .color(Color32::from_rgb(0x10, 0x12, 0x18))
                .strong(),
        )
        .fill(theme::ACCENT)
        .min_size(vec2(78.0, 26.0)),
    )
}

fn keys_label(keys: &[&str]) -> RichText {
    RichText::new(keys.join(" "))
        .monospace()
        .size(11.5)
        .color(theme::TEXT)
        .background_color(theme::INPUT_BG)
}

fn select_all_text(ctx: &egui::Context, id: Id, text: &str) {
    if let Some(mut state) = TextEditState::load(ctx, id) {
        let count = text.chars().count();
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(count),
        )));
        state.store(ctx, id);
    }
}

fn set_editor_selection(ctx: &egui::Context, start: usize, end: usize) {
    let mut state = TextEditState::load(ctx, editor_id()).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
    state.store(ctx, editor_id());
}

fn cursor_rect(galley: &egui::Galley, galley_pos: egui::Pos2, char_index: usize) -> egui::Rect {
    let cursor = galley.pos_from_cursor(CCursor::new(char_index));
    egui::Rect::from_min_size(
        galley_pos + cursor.min.to_vec2() - vec2(galley.rect.left(), 0.0),
        cursor.size(),
    )
}

fn build_job(
    text: &str,
    matches: &[Range<usize>],
    current: usize,
    font_id: &FontId,
    color: Color32,
    wrap_width: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_width;

    let base = egui::text::TextFormat {
        font_id: font_id.clone(),
        color,
        ..Default::default()
    };

    let mut position = 0;
    for (index, found) in matches.iter().enumerate() {
        if found.start > text.len() || found.end > text.len() || found.start < position {
            continue;
        }
        if found.start > position {
            job.append(&text[position..found.start], 0.0, base.clone());
        }
        let mut format = base.clone();
        format.background = if index == current {
            theme::MATCH_CURRENT_BG
        } else {
            theme::MATCH_BG
        };
        job.append(&text[found.start..found.end], 0.0, format);
        position = found.end;
    }
    if position < text.len() {
        job.append(&text[position..], 0.0, base);
    }
    job
}

const SHORTCUTS: &[(&[&str], &str)] = &[
    (&["Ctrl+N"], "New document"),
    (&["Ctrl+O"], "Open file"),
    (&["Ctrl+S"], "Save"),
    (&["Ctrl+Shift+S"], "Save as"),
    (&["Ctrl+Shift+C"], "Copy file path"),
    (&["Ctrl+Q"], "Quit"),
    (&["Ctrl+Shift+P"], "Command palette"),
    (&["Ctrl+F"], "Find"),
    (&["F3"], "Find next"),
    (&["Shift+F3"], "Find previous"),
    (&["Ctrl+L"], "Go to line"),
    (&["F1"], "This help"),
    (&["Esc"], "Close find / go to line"),
    (&["Ctrl+Z"], "Undo"),
    (&["Ctrl+Y"], "Redo"),
    (&["Ctrl+X"], "Cut"),
    (&["Ctrl+C"], "Copy"),
    (&["Ctrl+V"], "Paste"),
    (&["Ctrl+A"], "Select all"),
    (&["Ctrl+Shift+K"], "Delete line"),
    (&["Ctrl++"], "Larger text"),
    (&["Ctrl+-"], "Smaller text"),
    (&["Ctrl+0"], "Reset text size"),
];

fn shortcut_grid(ui: &mut Ui, id: &'static str, rows: &[(&[&str], &str)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([28.0, 7.0])
        .show(ui, |ui| {
            for (keys, action) in rows {
                ui.label(keys_label(keys));
                ui.label(RichText::new(*action).color(theme::MUTED));
                ui.end_row();
            }
        });
}
