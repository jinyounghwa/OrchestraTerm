use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::core::{LayoutNode, SHORTCUTS, SessionCore, SplitAxis};
use crate::engine::EngineState;
use crate::keymap::{Action, Mode, map_key};
use crate::terminal::PaneTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderPreset {
    Balanced,
    Compact,
    Pixel,
}

impl RenderPreset {
    fn label(self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Compact => "Compact",
            Self::Pixel => "Pixel",
        }
    }

    fn all() -> [Self; 3] {
        [Self::Balanced, Self::Compact, Self::Pixel]
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderMetrics {
    cell_w: f32,
    cell_h: f32,
    font_size: f32,
}

impl RenderMetrics {
    fn for_preset(preset: RenderPreset) -> Self {
        match preset {
            RenderPreset::Balanced => Self {
                cell_w: 9.0,
                cell_h: 18.0,
                font_size: 14.0,
            },
            RenderPreset::Compact => Self {
                cell_w: 8.0,
                cell_h: 16.0,
                font_size: 12.5,
            },
            RenderPreset::Pixel => Self {
                cell_w: 10.0,
                cell_h: 20.0,
                font_size: 15.0,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Prefix,
    Copy,
    CopySearch,
}

struct PaneRuntime {
    terminal: PaneTerminal,
    parser: vt100::Parser,
    cols: u16,
    rows: u16,
}

pub struct OrchestraApp {
    core: SessionCore,
    input_mode: InputMode,
    runtimes: BTreeMap<usize, PaneRuntime>,
    workspace_dir: Option<PathBuf>,
    copy_cursor: (u16, u16),
    copy_anchor: Option<(u16, u16)>,
    copy_search_input: String,
    pending_copy_text: Option<String>,
    engine_state: EngineState,
    cursor_visible: bool,
    last_cursor_toggle: Instant,
    render_preset: RenderPreset,
}

impl OrchestraApp {
    pub fn new() -> Self {
        let engine_state = EngineState::load_or_default();
        let session_name = engine_state
            .active_session
            .as_deref()
            .unwrap_or("default")
            .to_string();
        let mut core = SessionCore::new(session_name);
        core.append_line_focused("Interactive shell attached");

        let mut app = Self {
            core,
            input_mode: InputMode::Normal,
            runtimes: BTreeMap::new(),
            workspace_dir: None,
            copy_cursor: (0, 0),
            copy_anchor: None,
            copy_search_input: String::new(),
            pending_copy_text: None,
            engine_state,
            cursor_visible: true,
            last_cursor_toggle: Instant::now(),
            render_preset: RenderPreset::Balanced,
        };
        app.sync_runtimes();
        app
    }

    fn spawn_runtime_for(&mut self, pane_id: usize) {
        if self.runtimes.contains_key(&pane_id) {
            return;
        }

        let start_dir = self
            .workspace_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        match PaneTerminal::spawn(start_dir.as_deref()) {
            Ok(terminal) => {
                self.runtimes.insert(
                    pane_id,
                    PaneRuntime {
                        terminal,
                        parser: vt100::Parser::new(48, 160, 10_000),
                        cols: 160,
                        rows: 48,
                    },
                );
            }
            Err(err) => {
                self.core
                    .append_line_to_pane(pane_id, format!("[error] failed to start shell: {err}"));
            }
        }
    }

    fn sync_runtimes(&mut self) {
        let ids = self.core.pane_ids();

        let stale = self
            .runtimes
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect::<Vec<_>>();
        for pane_id in stale {
            if let Some(mut runtime) = self.runtimes.remove(&pane_id) {
                runtime.terminal.kill();
            }
        }

        for pane_id in ids {
            self.spawn_runtime_for(pane_id);
        }
    }

    fn poll_runtime_output(&mut self) {
        for pane_id in self.core.pane_ids() {
            let Some(runtime) = self.runtimes.get_mut(&pane_id) else {
                continue;
            };

            while let Ok(chunk) = runtime.terminal.output_rx.try_recv() {
                runtime.parser.process(&chunk);
            }
        }
    }

    fn open_folder(&mut self) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            let folder_text = folder.to_string_lossy().to_string();
            self.workspace_dir = Some(folder.clone());

            for pane_id in self.core.pane_ids() {
                if let Some(runtime) = self.runtimes.get_mut(&pane_id) {
                    let _ = runtime
                        .terminal
                        .send_line(&format!("cd '{}'", folder_text.replace('\'', "'\"'\"'")));
                }
            }
        }
    }

    fn send_focused_bytes(&mut self, bytes: &[u8]) {
        let pane_id = self.core.focused_pane;
        if let Some(runtime) = self.runtimes.get_mut(&pane_id) {
            let _ = runtime.terminal.write_bytes(bytes);
        }
    }

    fn send_focused_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.send_focused_bytes(text.as_bytes());
    }

    fn handle_terminal_input(&mut self, ctx: &egui::Context) {
        if self.input_mode == InputMode::Prefix
            || self.input_mode == InputMode::Copy
            || self.input_mode == InputMode::CopySearch
        {
            return;
        }

        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Paste(text) => {
                    self.send_focused_bytes(b"\x1b[200~");
                    self.send_focused_text(&text);
                    self.send_focused_bytes(b"\x1b[201~");
                }
                egui::Event::Text(text) => {
                    self.send_focused_text(&text);
                }
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } if pressed => {
                    if let Some(action) = map_key(Mode::Normal, key, modifiers) {
                        self.apply_action(action);
                        continue;
                    }
                    if modifiers.command {
                        continue;
                    }
                    if modifiers.ctrl {
                        if let Some(byte) = ctrl_key_to_byte(key) {
                            self.send_focused_bytes(&[byte]);
                        }
                        continue;
                    }

                    if modifiers.alt {
                        match key {
                            egui::Key::ArrowLeft => self.send_focused_bytes(b"\x1bb"),
                            egui::Key::ArrowRight => self.send_focused_bytes(b"\x1bf"),
                            _ => {}
                        }
                        continue;
                    }

                    match key {
                        egui::Key::Enter => self.send_focused_bytes(b"\r"),
                        egui::Key::Backspace => self.send_focused_bytes(&[0x7f]),
                        egui::Key::Tab => {
                            if modifiers.shift {
                                self.send_focused_bytes(b"\x1b[Z");
                            } else {
                                self.send_focused_bytes(b"\t");
                            }
                        }
                        egui::Key::ArrowUp => self.send_focused_bytes(b"\x1b[A"),
                        egui::Key::ArrowDown => self.send_focused_bytes(b"\x1b[B"),
                        egui::Key::ArrowRight => self.send_focused_bytes(b"\x1b[C"),
                        egui::Key::ArrowLeft => self.send_focused_bytes(b"\x1b[D"),
                        egui::Key::Home => self.send_focused_bytes(b"\x1b[H"),
                        egui::Key::End => self.send_focused_bytes(b"\x1b[F"),
                        egui::Key::Insert => self.send_focused_bytes(b"\x1b[2~"),
                        egui::Key::Delete => self.send_focused_bytes(b"\x1b[3~"),
                        egui::Key::PageUp => self.send_focused_bytes(b"\x1b[5~"),
                        egui::Key::PageDown => self.send_focused_bytes(b"\x1b[6~"),
                        egui::Key::F1 => self.send_focused_bytes(b"\x1bOP"),
                        egui::Key::F2 => self.send_focused_bytes(b"\x1bOQ"),
                        egui::Key::F3 => self.send_focused_bytes(b"\x1bOR"),
                        egui::Key::F4 => self.send_focused_bytes(b"\x1bOS"),
                        egui::Key::F5 => self.send_focused_bytes(b"\x1b[15~"),
                        egui::Key::F6 => self.send_focused_bytes(b"\x1b[17~"),
                        egui::Key::F7 => self.send_focused_bytes(b"\x1b[18~"),
                        egui::Key::F8 => self.send_focused_bytes(b"\x1b[19~"),
                        egui::Key::F9 => self.send_focused_bytes(b"\x1b[20~"),
                        egui::Key::F10 => self.send_focused_bytes(b"\x1b[21~"),
                        egui::Key::F11 => self.send_focused_bytes(b"\x1b[23~"),
                        egui::Key::F12 => self.send_focused_bytes(b"\x1b[24~"),
                        egui::Key::Escape => self.send_focused_bytes(&[0x1b]),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mode = match self.input_mode {
            InputMode::Normal => Mode::Normal,
            InputMode::Prefix => Mode::Prefix,
            InputMode::Copy => Mode::Copy,
            InputMode::CopySearch => Mode::CopySearch,
        };

        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            if self.input_mode == InputMode::CopySearch {
                if let egui::Event::Text(text) = &ev {
                    self.copy_search_input.push_str(text);
                    continue;
                }
            }
            let egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } = ev
            else {
                continue;
            };
            if !pressed {
                continue;
            }
            if let Some(action) = map_key(mode, key, modifiers) {
                self.apply_action(action);
            }
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::OpenFolder => self.open_folder(),
            Action::SendEnter => self.send_focused_bytes(b"\r"),
            Action::EnterPrefix => self.input_mode = InputMode::Prefix,
            Action::EnterCopyMode => {
                self.input_mode = InputMode::Copy;
                self.copy_cursor = (0, 0);
                self.copy_anchor = None;
                self.copy_search_input.clear();
            }
            Action::ExitCopyMode => {
                self.input_mode = InputMode::Normal;
                self.copy_anchor = None;
                self.copy_search_input.clear();
            }
            Action::SplitHorizontal => {
                self.core.split_focused(SplitAxis::Horizontal);
                self.input_mode = InputMode::Normal;
            }
            Action::SplitVertical => {
                self.core.split_focused(SplitAxis::Vertical);
                self.input_mode = InputMode::Normal;
            }
            Action::ClosePane => {
                self.core.close_focused();
                self.input_mode = InputMode::Normal;
            }
            Action::ToggleZoom => {
                self.core.toggle_zoom();
                self.input_mode = InputMode::Normal;
            }
            Action::FocusPrev => {
                self.core.focus_prev();
                self.input_mode = InputMode::Normal;
            }
            Action::FocusNext => {
                self.core.focus_next();
                self.input_mode = InputMode::Normal;
            }
            Action::CopyMoveUp => self.copy_cursor.1 = self.copy_cursor.1.saturating_sub(1),
            Action::CopyMoveDown => self.copy_cursor.1 = self.copy_cursor.1.saturating_add(1),
            Action::CopyMoveLeft => self.copy_cursor.0 = self.copy_cursor.0.saturating_sub(1),
            Action::CopyMoveRight => self.copy_cursor.0 = self.copy_cursor.0.saturating_add(1),
            Action::CopyStartSelection => {
                self.copy_anchor = Some(self.copy_cursor);
            }
            Action::CopyCopySelection => {
                self.pending_copy_text = self.extract_copy_selection();
            }
            Action::CopySearchStart => {
                self.input_mode = InputMode::CopySearch;
                self.copy_search_input.clear();
            }
            Action::CopySearchApply => {
                self.apply_copy_search();
                self.input_mode = InputMode::Copy;
            }
        }
    }

    fn apply_copy_search(&mut self) {
        let query = self.copy_search_input.trim();
        if query.is_empty() {
            return;
        }
        let pane_id = self.core.focused_pane;
        let Some(runtime) = self.runtimes.get(&pane_id) else {
            return;
        };
        let screen = runtime.parser.screen();
        let contents = screen.contents();
        for (y, line) in contents.lines().enumerate() {
            if let Some(x) = line.find(query) {
                self.copy_cursor = (x as u16, y as u16);
                self.copy_anchor = Some(self.copy_cursor);
                break;
            }
        }
    }

    fn extract_copy_selection(&self) -> Option<String> {
        let pane_id = self.core.focused_pane;
        let runtime = self.runtimes.get(&pane_id)?;
        let anchor = self.copy_anchor?;
        let cursor = self.copy_cursor;

        let screen = runtime.parser.screen();
        let lines = screen.contents();
        let lines: Vec<&str> = lines.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let (sx, sy) = anchor;
        let (ex, ey) = cursor;
        let y0 = sy.min(ey) as usize;
        let y1 = sy.max(ey) as usize;
        let mut out = Vec::new();

        for y in y0..=y1.min(lines.len().saturating_sub(1)) {
            let line = &lines[y];
            let (x0, x1) = if y == y0 && y == y1 {
                (sx.min(ex) as usize, sx.max(ex) as usize)
            } else if y == y0 {
                (sx as usize, line.len().saturating_sub(1))
            } else if y == y1 {
                (0, ex as usize)
            } else {
                (0, line.len().saturating_sub(1))
            };
            if line.is_empty() {
                out.push(String::new());
                continue;
            }
            let start = x0.min(line.len().saturating_sub(1));
            let end = x1.min(line.len().saturating_sub(1));
            out.push(line[start..=end].to_string());
        }

        Some(out.join("\n"))
    }

    fn draw_node(&mut self, ui: &mut egui::Ui, rect: egui::Rect, node: &LayoutNode) {
        match node {
            LayoutNode::Leaf(id) => self.draw_leaf(ui, rect, *id),
            LayoutNode::Split {
                axis,
                first,
                second,
            } => {
                let spacing = 6.0;
                match axis {
                    SplitAxis::Horizontal => {
                        let half = (rect.height() - spacing) / 2.0;
                        let r1 =
                            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), half));
                        let r2 = egui::Rect::from_min_size(
                            egui::pos2(rect.left(), rect.top() + half + spacing),
                            egui::vec2(rect.width(), half),
                        );
                        self.draw_node(ui, r1, first);
                        self.draw_node(ui, r2, second);
                    }
                    SplitAxis::Vertical => {
                        let half = (rect.width() - spacing) / 2.0;
                        let r1 =
                            egui::Rect::from_min_size(rect.min, egui::vec2(half, rect.height()));
                        let r2 = egui::Rect::from_min_size(
                            egui::pos2(rect.left() + half + spacing, rect.top()),
                            egui::vec2(half, rect.height()),
                        );
                        self.draw_node(ui, r1, first);
                        self.draw_node(ui, r2, second);
                    }
                }
            }
        }
    }

    fn draw_leaf(&mut self, ui: &mut egui::Ui, rect: egui::Rect, pane_id: usize) {
        let is_focused = pane_id == self.core.focused_pane;
        let stroke = if is_focused {
            egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 200, 120))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(90))
        };

        ui.painter()
            .rect_filled(rect, 6.0, egui::Color32::from_rgb(22, 25, 29));
        ui.painter()
            .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);

        let response = ui.allocate_rect(rect, egui::Sense::click());
        if response.clicked() {
            self.core.focused_pane = pane_id;
        }
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(8.0, 8.0))),
            |ui| {
                if let Some(runtime) = self.runtimes.get_mut(&pane_id) {
                    let pane = self
                        .core
                        .panes
                        .iter()
                        .find(|p| p.id == pane_id)
                        .expect("pane must exist");

                    let metrics = RenderMetrics::for_preset(self.render_preset);
                    let cols = ((rect.width() - 22.0) / metrics.cell_w).max(20.0) as u16;
                    let rows = ((rect.height() - 42.0) / metrics.cell_h).max(8.0) as u16;

                    if cols != runtime.cols || rows != runtime.rows {
                        runtime.cols = cols;
                        runtime.rows = rows;
                        let _ = runtime.terminal.resize(cols, rows);
                        runtime.parser.set_size(rows, cols);
                    }

                    ui.horizontal(|ui| {
                        ui.strong(format!("{} (#{} )", pane.title, pane.id));
                    });
                    ui.separator();

                    let screen = runtime.parser.screen();
                    let mut origin = ui.cursor().min;
                    origin.x = origin.x.round();
                    origin.y = origin.y.round();
                    let cell_w = metrics.cell_w;
                    let cell_h = metrics.cell_h;
                    let font_regular = egui::FontId::monospace(metrics.font_size);
                    let font_bold =
                        egui::FontId::new(metrics.font_size, egui::FontFamily::Monospace);

                    let max_rows = runtime.rows.min(200);
                    let max_cols = runtime.cols.min(400);

                    for row in 0..max_rows {
                        let mut col = 0;
                        while col < max_cols {
                            let Some(cell) = screen.cell(row, col) else {
                                col += 1;
                                continue;
                            };
                            if cell.is_wide_continuation() {
                                col += 1;
                                continue;
                            }

                            let is_wide = col + 1 < max_cols
                                && screen
                                    .cell(row, col + 1)
                                    .map(|c| c.is_wide_continuation())
                                    .unwrap_or(false);
                            let span = if is_wide { 2.0 } else { 1.0 };

                            let mut fg = vt_fg_to_egui(cell.fgcolor());
                            let mut bg = vt_bg_to_egui(cell.bgcolor());
                            if cell.inverse() {
                                std::mem::swap(&mut fg, &mut bg);
                            }
                            if cell.bgcolor() != vt100::Color::Default || cell.inverse() {
                                let x = origin.x + f32::from(col) * cell_w;
                                let y = origin.y + f32::from(row) * cell_h;
                                let cell_rect = egui::Rect::from_min_size(
                                    egui::pos2(x, y),
                                    egui::vec2(cell_w * span, cell_h),
                                );
                                ui.painter().rect_filled(cell_rect, 0.0, bg);
                            }

                            let content = cell.contents();
                            if content.is_empty() {
                                col += if is_wide { 2 } else { 1 };
                                continue;
                            }
                            let x = origin.x + f32::from(col) * cell_w;
                            let y = origin.y + f32::from(row) * cell_h;
                            ui.painter().text(
                                egui::pos2(x, y),
                                egui::Align2::LEFT_TOP,
                                content,
                                if cell.bold() {
                                    font_bold.clone()
                                } else {
                                    font_regular.clone()
                                },
                                fg,
                            );
                            if cell.underline() {
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(x, y + cell_h - 2.0),
                                        egui::pos2(x + (cell_w * span), y + cell_h - 2.0),
                                    ],
                                    egui::Stroke::new(1.0, fg),
                                );
                            }
                            col += if is_wide { 2 } else { 1 };
                        }
                    }

                    if pane_id == self.core.focused_pane && self.cursor_visible {
                        let (crow, ccol) = screen.cursor_position();
                        if crow < max_rows && ccol < max_cols {
                            let mut draw_col = ccol;
                            if let Some(cur_cell) = screen.cell(crow, ccol) {
                                if cur_cell.is_wide_continuation() && ccol > 0 {
                                    draw_col = ccol - 1;
                                }
                            }
                            let cursor_span = if draw_col + 1 < max_cols
                                && screen
                                    .cell(crow, draw_col + 1)
                                    .map(|c| c.is_wide_continuation())
                                    .unwrap_or(false)
                            {
                                2.0
                            } else {
                                1.0
                            };
                            let x = origin.x + f32::from(draw_col) * cell_w;
                            let y = origin.y + f32::from(crow) * cell_h;
                            let cursor_rect = egui::Rect::from_min_size(
                                egui::pos2(x, y),
                                egui::vec2(cell_w * cursor_span, cell_h),
                            );
                            ui.painter().rect_filled(
                                cursor_rect,
                                0.0,
                                egui::Color32::from_rgba_unmultiplied(120, 220, 160, 28),
                            );
                            ui.painter().rect_stroke(
                                cursor_rect,
                                0.0,
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 220, 160)),
                                egui::StrokeKind::Inside,
                            );
                        }
                    }
                }
            },
        );
    }
}

impl eframe::App for OrchestraApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_cursor_toggle.elapsed() >= Duration::from_millis(530) {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_toggle = Instant::now();
        }

        self.sync_runtimes();
        self.poll_runtime_output();
        self.handle_shortcuts(ctx);
        self.handle_terminal_input(ctx);

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OrchestraTerm");
                ui.separator();
                ui.label(format!("Session: {}", self.core.name));
                ui.separator();
                ui.label(format!("Focused: {}", self.core.focused_pane));
                ui.separator();
                ui.label(format!("Panes: {}", self.core.panes.len()));
                ui.separator();
                if let Some(path) = &self.workspace_dir {
                    ui.label(format!("Workspace: {}", path.display()));
                } else {
                    ui.label("Workspace: (not selected)");
                }
                if ui.button("Open Folder").clicked() {
                    self.open_folder();
                }
                if self.input_mode == InputMode::Prefix {
                    ui.colored_label(egui::Color32::YELLOW, "PREFIX MODE (Ctrl+B)");
                } else if self.input_mode == InputMode::Copy {
                    ui.colored_label(
                        egui::Color32::LIGHT_BLUE,
                        format!(
                            "COPY MODE (cursor: {}, {})",
                            self.copy_cursor.0, self.copy_cursor.1
                        ),
                    );
                } else if self.input_mode == InputMode::CopySearch {
                    ui.colored_label(
                        egui::Color32::LIGHT_BLUE,
                        format!("COPY SEARCH: /{}", self.copy_search_input),
                    );
                }
            });
        });

        egui::SidePanel::right("shortcut_panel")
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("Shortcuts");
                ui.label("Right-side fixed panel");
                egui::ComboBox::from_label("Render Preset")
                    .selected_text(self.render_preset.label())
                    .show_ui(ui, |ui| {
                        for preset in RenderPreset::all() {
                            ui.selectable_value(&mut self.render_preset, preset, preset.label());
                        }
                    });
                ui.separator();
                for sc in SHORTCUTS {
                    ui.horizontal(|ui| {
                        ui.monospace(sc.key);
                        ui.label(sc.action);
                    });
                }
                ui.separator();
                ui.heading("Team Modes");
                if self.engine_state.teams.is_empty() {
                    ui.label("No teams");
                } else {
                    for team in self.engine_state.teams.values() {
                        ui.label(format!(
                            "{} mode={:?} delegation_only={}",
                            team.id, team.mode, team.delegation_only
                        ));
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect().shrink2(egui::vec2(6.0, 6.0));
            let layout = if self.core.zoomed {
                LayoutNode::Leaf(self.core.focused_pane)
            } else {
                self.core.layout.clone()
            };
            self.draw_node(ui, rect, &layout);
        });

        if let Some(text) = self.pending_copy_text.take() {
            ctx.copy_text(text);
        }

        if let Some(session) = self.engine_state.sessions.get_mut(&self.core.name) {
            if let Some(window) = session.windows.get_mut(0) {
                window.active_pane = self.core.focused_pane;
            }
        } else {
            self.engine_state.create_session(&self.core.name);
        }
        self.engine_state.active_session = Some(self.core.name.clone());
        let _ = self.engine_state.save();

        ctx.request_repaint();
    }
}

impl OrchestraApp {}

fn ctrl_key_to_byte(key: egui::Key) -> Option<u8> {
    match key {
        egui::Key::A => Some(0x01),
        egui::Key::B => Some(0x02),
        egui::Key::C => Some(0x03),
        egui::Key::D => Some(0x04),
        egui::Key::E => Some(0x05),
        egui::Key::F => Some(0x06),
        egui::Key::G => Some(0x07),
        egui::Key::H => Some(0x08),
        egui::Key::I => Some(0x09),
        egui::Key::J => Some(0x0a),
        egui::Key::K => Some(0x0b),
        egui::Key::L => Some(0x0c),
        egui::Key::M => Some(0x0d),
        egui::Key::N => Some(0x0e),
        egui::Key::O => Some(0x0f),
        egui::Key::P => Some(0x10),
        egui::Key::Q => Some(0x11),
        egui::Key::R => Some(0x12),
        egui::Key::S => Some(0x13),
        egui::Key::T => Some(0x14),
        egui::Key::U => Some(0x15),
        egui::Key::V => Some(0x16),
        egui::Key::W => Some(0x17),
        egui::Key::X => Some(0x18),
        egui::Key::Y => Some(0x19),
        egui::Key::Z => Some(0x1a),
        _ => None,
    }
}

fn vt_fg_to_egui(color: vt100::Color) -> egui::Color32 {
    match color {
        vt100::Color::Default => egui::Color32::from_rgb(210, 220, 230),
        vt100::Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
        vt100::Color::Idx(idx) => ansi256_to_egui(idx),
    }
}

fn vt_bg_to_egui(color: vt100::Color) -> egui::Color32 {
    match color {
        vt100::Color::Default => egui::Color32::from_rgb(22, 25, 29),
        vt100::Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
        vt100::Color::Idx(idx) => ansi256_to_egui(idx),
    }
}

fn ansi256_to_egui(i: u8) -> egui::Color32 {
    const BASE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 49, 49),
        (13, 188, 121),
        (229, 229, 16),
        (36, 114, 200),
        (188, 63, 188),
        (17, 168, 205),
        (229, 229, 229),
        (102, 102, 102),
        (241, 76, 76),
        (35, 209, 139),
        (245, 245, 67),
        (59, 142, 234),
        (214, 112, 214),
        (41, 184, 219),
        (255, 255, 255),
    ];
    if i < 16 {
        let (r, g, b) = BASE[usize::from(i)];
        return egui::Color32::from_rgb(r, g, b);
    }
    if (16..=231).contains(&i) {
        let idx = i - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let map = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        return egui::Color32::from_rgb(map(r), map(g), map(b));
    }
    let gray = 8 + (i - 232) * 10;
    egui::Color32::from_rgb(gray, gray, gray)
}
