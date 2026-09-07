use crate::{
    components::icons::icon,
    theme::{Theme, ThemeMode},
};
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, FontWeight, KeyDownEvent, MouseButton, Pixels, Point, SharedString,
    TextRun, UTF16Selection, Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    ops::Range,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const FONT_SIZE: f32 = 12.0;
const CELL_WIDTH: f32 = 7.224;
const LINE_HEIGHT: f32 = 16.5098;
const SCROLLBACK: usize = 10_000;

gpui::actions!(
    terminal,
    [
        TerminalCopy,
        TerminalPaste,
        TerminalSelectAll,
        TerminalClear,
        TerminalEscape
    ]
);
pub fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-c", TerminalCopy, Some("Terminal")),
        gpui::KeyBinding::new("cmd-v", TerminalPaste, Some("Terminal")),
        gpui::KeyBinding::new("cmd-a", TerminalSelectAll, Some("Terminal")),
        gpui::KeyBinding::new("cmd-k", TerminalClear, Some("Terminal")),
        gpui::KeyBinding::new("escape", TerminalEscape, Some("Terminal")),
    ]);
}

#[derive(Default)]
struct TerminalCallbacks {
    replies: Vec<Vec<u8>>,
}
impl vt100::Callbacks for TerminalCallbacks {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let param = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        let response = match (i1, c, param) {
            (None, 'n', 5) => Some("\x1b[0n".to_owned()),
            (None, 'n', 6) => {
                let (r, c) = screen.cursor_position();
                Some(format!("\x1b[{};{}R", r + 1, c + 1))
            }
            (None, 'c', 0) => Some("\x1b[?1;2c".to_owned()),
            (Some(b'>'), 'c', 0) => Some("\x1b[>0;0;0c".to_owned()),
            _ => None,
        };
        if let Some(response) = response {
            self.replies.push(response.into_bytes());
        }
    }
}

enum Output {
    Bytes(Vec<u8>),
    Exit(String),
    Error(String),
}
struct Process {
    master: Box<dyn MasterPty + Send>,
    input: Option<async_channel::Sender<Vec<u8>>>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    exited: Arc<AtomicBool>,
}
impl Drop for Process {
    fn drop(&mut self) {
        self.input.take();
        if !self.exited.load(Ordering::Acquire) {
            let _ = self.killer.kill();
        }
    }
}
impl Process {
    fn start(cwd: &PathBuf) -> anyhow::Result<(Self, async_channel::Receiver<Output>)> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut command = CommandBuilder::new(shell);
        command.arg("-l");
        command.cwd(cwd);
        Self::spawn(command)
    }
    fn spawn(
        mut command: CommandBuilder,
    ) -> anyhow::Result<(Self, async_channel::Receiver<Output>)> {
        let pair = portable_pty::native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let mut child = pair.slave.spawn_command(command)?;
        let killer = child.clone_killer();
        let exited = Arc::new(AtomicBool::new(false));
        let reader_exited = exited.clone();
        drop(pair.slave);
        let (tx, rx) = async_channel::bounded(64);
        let errors = tx.clone();
        let (input, incoming) = async_channel::unbounded::<Vec<u8>>();
        std::thread::spawn(move || {
            while let Ok(bytes) = incoming.recv_blocking() {
                if let Err(error) = writer.write_all(&bytes).and_then(|_| writer.flush()) {
                    let _ = errors.send_blocking(Output::Error(error.to_string()));
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut buffer = [0; 16384];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx
                            .send_blocking(Output::Bytes(buffer[..n].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    // macOS reports EOF as EIO on a closed PTY.
                    Err(e) if e.raw_os_error() == Some(5) => break,
                    Err(e) => {
                        let _ = tx.send_blocking(Output::Error(e.to_string()));
                        break;
                    }
                }
            }
            let status = child
                .wait()
                .map(|s| format!("进程已退出（{}）", s.exit_code()))
                .unwrap_or_else(|e| e.to_string());
            reader_exited.store(true, Ordering::Release);
            let _ = tx.send_blocking(Output::Exit(status));
        });
        Ok((
            Self {
                master: pair.master,
                input: Some(input),
                killer,
                exited,
            },
            rx,
        ))
    }
}

pub struct TerminalView {
    parser: vt100::Parser<TerminalCallbacks>,
    process: Option<Process>,
    status: Option<String>,
    focus: FocusHandle,
    bounds: Option<Bounds<Pixels>>,
    selection: Option<((u16, u16), (u16, u16))>,
    selecting: bool,
    scroll_delta: f32,
    marked: String,
    mode: ThemeMode,
    focus_pending: bool,
    blink: bool,
}
impl TerminalView {
    fn new(cwd: PathBuf, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        Self::with_process(Process::start(&cwd), mode, cx)
    }
    fn with_process(
        process: anyhow::Result<(Process, async_channel::Receiver<Output>)>,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            parser: vt100::Parser::new_with_callbacks(
                24,
                80,
                SCROLLBACK,
                TerminalCallbacks::default(),
            ),
            process: None,
            status: None,
            focus: cx.focus_handle(),
            bounds: None,
            selection: None,
            selecting: false,
            scroll_delta: 0.0,
            marked: String::new(),
            mode,
            focus_pending: true,
            blink: true,
        };
        match process {
            Ok((process, output)) => {
                view.process = Some(process);
                cx.spawn(async move |this, cx| {
                    while let Ok(event) = output.recv().await {
                        if this
                            .update(cx, |this, cx| {
                                match event {
                                    Output::Bytes(bytes) => {
                                        this.parser.process(&bytes);
                                        for reply in
                                            std::mem::take(&mut this.parser.callbacks_mut().replies)
                                        {
                                            if let Some(input) =
                                                this.process.as_ref().and_then(|p| p.input.as_ref())
                                            {
                                                let _ = input.try_send(reply);
                                            }
                                        }
                                    }
                                    Output::Exit(status) => {
                                        this.status = Some(status);
                                        this.process = None;
                                    }
                                    Output::Error(error) => {
                                        this.status = Some(format!("终端错误：{error}"))
                                    }
                                }
                                this.blink = true;
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(error) => view.status = Some(format!("无法启动终端：{error}")),
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(600))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.blink = !this.blink;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view
    }
    fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if let Some(input) = self.process.as_ref().and_then(|p| p.input.as_ref()) {
            if input.try_send(bytes).is_err() {
                self.status = Some("终端连接已关闭".into());
            }
        }
        self.parser.screen_mut().set_scrollback(0);
        self.selection = None;
        self.blink = true;
        cx.notify();
    }
    fn resize(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = Some(bounds);
        let cols = ((f32::from(bounds.size.width) / CELL_WIDTH).floor() as u16).max(2);
        let rows = ((f32::from(bounds.size.height) / LINE_HEIGHT).floor() as u16).max(1);
        if self.parser.screen().size() != (rows, cols) {
            if let Some(process) = &self.process {
                if let Err(error) = process.master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }) {
                    self.status = Some(format!("终端尺寸更新失败：{error}"));
                }
            }
            self.parser.screen_mut().set_size(rows, cols);
            self.selection = None;
        }
    }
    fn cell_at(&self, position: Point<Pixels>) -> (u16, u16) {
        let bounds = self.bounds.unwrap_or_default();
        let (rows, cols) = self.parser.screen().size();
        (
            ((f32::from(position.y - bounds.top()) / LINE_HEIGHT).max(0.0) as u16).min(rows - 1),
            ((f32::from(position.x - bounds.left()) / CELL_WIDTH).max(0.0) as u16).min(cols),
        )
    }
    fn selected_text(&self) -> String {
        let Some((a, b)) = self.selection else {
            return String::new();
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let screen = self.parser.screen();
        let mut text = String::new();
        for row in start.0..=end.0 {
            let first = if row == start.0 { start.1 } else { 0 };
            let last = if row == end.0 { end.1 } else { screen.size().1 };
            let mut line = String::new();
            for col in first..last {
                if let Some(cell) = screen.cell(row, col) {
                    if !cell.is_wide_continuation() {
                        let s = cell.contents();
                        line.push_str(if s.is_empty() { " " } else { s });
                    }
                }
            }
            text.push_str(line.trim_end());
            if row < end.0 && !screen.row_wrapped(row) {
                text.push('\n');
            }
        }
        text
    }
    fn platform_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.key(
            &KeyDownEvent {
                keystroke: gpui::Keystroke::parse(&format!("cmd-{key}"))
                    .expect("terminal shortcut"),
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        );
    }
    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.modifiers.platform {
            match key.key.as_str() {
                "c" => cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text())),
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        let text = text.replace("\r\n", "\n").replace('\r', "\n");
                        let text = if self.parser.screen().bracketed_paste() {
                            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
                        } else {
                            text
                        };
                        self.send(text.into_bytes(), cx);
                    }
                }
                "a" => {
                    let (r, c) = self.parser.screen().size();
                    self.selection = Some(((0, 0), (r - 1, c)));
                }
                "k" => {
                    // Full-screen programs own the alternate grid. Ask them to
                    // redraw without discarding the saved shell screen or modes.
                    if !self.parser.screen().alternate_screen() {
                        let modes = self.parser.screen().input_mode_formatted();
                        let attrs = self.parser.screen().attributes_formatted();
                        self.parser = vt100::Parser::new_with_callbacks(
                            self.parser.screen().size().0,
                            self.parser.screen().size().1,
                            SCROLLBACK,
                            TerminalCallbacks::default(),
                        );
                        self.parser.process(&modes);
                        self.parser.process(&attrs);
                    }
                    self.send(vec![12], cx);
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if let Some(bytes) = key_bytes(key, self.parser.screen().application_cursor()) {
            self.send(bytes, cx);
            cx.stop_propagation();
        }
    }
}

fn key_bytes(key: &gpui::Keystroke, application_cursor: bool) -> Option<Vec<u8>> {
    if key.modifiers.control {
        let c = match key.key.as_str() {
            "space" | "@" => 0,
            "[" => 27,
            "\\" => 28,
            "]" => 29,
            "^" => 30,
            "_" => 31,
            s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphabetic() => {
                s.as_bytes()[0].to_ascii_lowercase() - b'a' + 1
            }
            _ => return None,
        };
        return Some(vec![c]);
    }
    let sequence = match key.key.as_str() {
        "enter" => "\r",
        "backspace" => "\x7f",
        "delete" => "\x1b[3~",
        "escape" => "\x1b",
        "tab" if key.modifiers.shift => "\x1b[Z",
        "tab" => "\t",
        "up" if application_cursor => "\x1bOA",
        "down" if application_cursor => "\x1bOB",
        "right" if application_cursor => "\x1bOC",
        "left" if application_cursor => "\x1bOD",
        "up" => "\x1b[A",
        "down" => "\x1b[B",
        "right" => "\x1b[C",
        "left" => "\x1b[D",
        "home" => "\x1b[H",
        "end" => "\x1b[F",
        "pageup" => "\x1b[5~",
        "pagedown" => "\x1b[6~",
        "f1" => "\x1bOP",
        "f2" => "\x1bOQ",
        "f3" => "\x1bOR",
        "f4" => "\x1bOS",
        _ => {
            if key.modifiers.alt {
                return key
                    .key_char
                    .as_ref()
                    .map(|s| format!("\x1b{s}").into_bytes());
            }
            return None;
        }
    };
    Some(sequence.as_bytes().to_vec())
}

fn ansi_color(color: vt100::Color, default: gpui::Hsla) -> gpui::Hsla {
    let value = match color {
        vt100::Color::Default => return default,
        vt100::Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        vt100::Color::Idx(i) => {
            const COLORS: [u32; 16] = [
                0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5,
                0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
            ];
            if i < 16 {
                COLORS[i as usize]
            } else if i >= 232 {
                let v = 8 + 10 * u32::from(i - 232);
                (v << 16) | (v << 8) | v
            } else {
                let i = u32::from(i - 16);
                let c = |v| if v == 0 { 0 } else { 55 + 40 * v };
                (c(i / 36) << 16) | (c(i / 6 % 6) << 8) | c(i % 6)
            }
        }
    };
    rgb(value).into()
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_pending {
            self.focus.focus(window, cx);
            self.focus_pending = false;
        }
        let theme = Theme::for_mode(self.mode);
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div()
            .id("terminal-session")
            .role(gpui::Role::Terminal)
            .aria_label("内置终端")
            .aria_value(self.parser.screen().contents())
            .size_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::key))
            .on_action(
                cx.listener(|this, _: &TerminalCopy, window, cx| {
                    this.platform_key("c", window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &TerminalPaste, window, cx| {
                    this.platform_key("v", window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &TerminalSelectAll, window, cx| {
                this.platform_key("a", window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &TerminalClear, window, cx| {
                    this.platform_key("k", window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &TerminalEscape, _, cx| {
                this.send(vec![27], cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("terminal-grid")
                    .flex_1()
                    .min_h(px(0.))
                    .cursor_text()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            this.focus.focus(window, cx);
                            let cell = this.cell_at(event.position);
                            this.selection = Some((cell, cell));
                            this.selecting = true;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if this.selecting {
                            let cell = this.cell_at(event.position);
                            if let Some((_, end)) = &mut this.selection {
                                *end = cell;
                            }
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.selecting = false;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.selecting = false;
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        let delta = f32::from(event.delta.pixel_delta(px(LINE_HEIGHT)).y);
                        this.scroll_delta += delta;
                        let rows = (this.scroll_delta / LINE_HEIGHT).trunc();
                        this.scroll_delta -= rows * LINE_HEIGHT;
                        let screen = this.parser.screen_mut();
                        let offset = (screen.scrollback() as isize + rows as isize).max(0) as usize;
                        screen.set_scrollback(offset);
                        this.selection = None;
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                entity.update(cx, |this, _| this.resize(bounds));
                            },
                            move |bounds, _, window, cx| {
                                paint_entity.update(cx, |this, cx| {
                                    window.handle_input(
                                        &this.focus,
                                        ElementInputHandler::new(bounds, paint_entity.clone()),
                                        cx,
                                    );
                                    let screen = this.parser.screen();
                                    let (rows, cols) = screen.size();
                                    let foreground: gpui::Hsla = match this.mode {
                                        ThemeMode::Dark => rgb(0xdfdfdf).into(),
                                        ThemeMode::Light => rgb(0x1a1c1f).into(),
                                    };
                                    let selection = this
                                        .selection
                                        .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
                                    for row in 0..rows {
                                        for col in 0..cols {
                                            let Some(cell) = screen.cell(row, col) else {
                                                continue;
                                            };
                                            if cell.is_wide_continuation() {
                                                continue;
                                            }
                                            let origin = bounds.origin
                                                + point(
                                                    px(col as f32 * CELL_WIDTH),
                                                    px(row as f32 * LINE_HEIGHT),
                                                );
                                            let width = if cell.is_wide() {
                                                CELL_WIDTH * 2.
                                            } else {
                                                CELL_WIDTH
                                            };
                                            let mut fg = ansi_color(cell.fgcolor(), foreground);
                                            let mut bg =
                                                ansi_color(cell.bgcolor(), theme.surface.into());
                                            if cell.inverse() {
                                                std::mem::swap(&mut fg, &mut bg);
                                            }
                                            if selection.is_some_and(|(a, b)| {
                                                (row, col) >= a && (row, col) < b
                                            }) {
                                                bg = theme.text.alpha(0.22).into();
                                            }
                                            if bg != theme.surface.into() {
                                                window.paint_quad(fill(
                                                    Bounds::new(
                                                        origin,
                                                        size(px(width), px(LINE_HEIGHT)),
                                                    ),
                                                    bg,
                                                ));
                                            }
                                            if cell.has_contents() {
                                                let mut font = gpui::font("Menlo");
                                                if cell.bold() {
                                                    font.weight = FontWeight::BOLD;
                                                }
                                                if cell.italic() {
                                                    font.style = gpui::FontStyle::Italic;
                                                }
                                                let text: SharedString =
                                                    cell.contents().to_owned().into();
                                                let line = window.text_system().shape_line(
                                                    text.clone(),
                                                    px(FONT_SIZE),
                                                    &[TextRun {
                                                        len: text.len(),
                                                        font,
                                                        color: fg.into(),
                                                        background_color: None,
                                                        underline: cell.underline().then_some(
                                                            gpui::UnderlineStyle {
                                                                thickness: px(1.),
                                                                color: Some(fg.into()),
                                                                wavy: false,
                                                            },
                                                        ),
                                                        strikethrough: None,
                                                    }],
                                                    None,
                                                );
                                                let _ = line.paint(
                                                    origin,
                                                    px(LINE_HEIGHT),
                                                    gpui::TextAlign::Left,
                                                    None,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        }
                                    }
                                    let (row, col) = screen.cursor_position();
                                    if screen.scrollback() == 0
                                        && !screen.hide_cursor()
                                        && this.focus.is_focused(window)
                                        && this.blink
                                        && this.process.is_some()
                                    {
                                        let origin = bounds.origin
                                            + point(
                                                px(col as f32 * CELL_WIDTH),
                                                px(row as f32 * LINE_HEIGHT),
                                            );
                                        window.paint_quad(fill(
                                            Bounds::new(origin, size(px(1.), px(LINE_HEIGHT))),
                                            foreground,
                                        ));
                                    }
                                    if !this.marked.is_empty() {
                                        let text: SharedString = this.marked.clone().into();
                                        let line = window.text_system().shape_line(
                                            text.clone(),
                                            px(FONT_SIZE),
                                            &[TextRun {
                                                len: text.len(),
                                                font: gpui::font("Menlo"),
                                                color: foreground.into(),
                                                background_color: Some(theme.surface.into()),
                                                underline: Some(gpui::UnderlineStyle {
                                                    thickness: px(1.),
                                                    color: None,
                                                    wavy: false,
                                                }),
                                                strikethrough: None,
                                            }],
                                            None,
                                        );
                                        let _ = line.paint(
                                            bounds.origin
                                                + point(
                                                    px(col as f32 * CELL_WIDTH),
                                                    px(row as f32 * LINE_HEIGHT),
                                                ),
                                            px(LINE_HEIGHT),
                                            gpui::TextAlign::Left,
                                            None,
                                            window,
                                            cx,
                                        );
                                    }
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
            .when_some(self.status.clone(), |body, status| {
                body.child(
                    div()
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme.text_secondary)
                        .child(status),
                )
            })
    }
}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(
            &self
                .marked
                .encode_utf16()
                .collect::<Vec<_>>()
                .get(range)?
                .to_vec(),
        ))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let n = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: n..n,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked.clear();
        self.send(text.as_bytes().to_vec(), cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = text.into();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (r, c) = self.parser.screen().cursor_position();
        Some(Bounds::new(
            bounds.origin + point(px(c as f32 * CELL_WIDTH), px(r as f32 * LINE_HEIGHT)),
            size(px(CELL_WIDTH), px(LINE_HEIGHT)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

pub struct TerminalPanel {
    tabs: Vec<(usize, Entity<TerminalView>)>,
    active: usize,
    next_id: usize,
    cwd: PathBuf,
    mode: ThemeMode,
}
impl TerminalPanel {
    pub fn new(cwd: PathBuf, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            cwd,
            mode,
        };
        panel.add(cx);
        panel
    }
    fn add(&mut self, cx: &mut Context<Self>) {
        let terminal = cx.new(|cx| TerminalView::new(self.cwd.clone(), self.mode, cx));
        self.tabs.push((self.next_id, terminal));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
        cx.notify();
    }
    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        for (_, tab) in &self.tabs {
            tab.update(cx, |t, cx| {
                t.mode = mode;
                cx.notify();
            });
        }
        cx.notify();
    }
    pub fn focus(&mut self, cx: &mut Context<Self>) {
        if let Some((_, tab)) = self.tabs.get(self.active) {
            tab.update(cx, |t, cx| {
                t.focus_pending = true;
                cx.notify();
            });
        }
    }
}
impl Render for TerminalPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let title = self
            .cwd
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "终端".into());
        div()
            .id("terminal-panel")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(px(46.))
                    .flex_none()
                    .px(px(8.))
                    .pr(px(82.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("terminal-tabs")
                            .flex()
                            .min_w(px(0.))
                            .overflow_x_scroll()
                            .gap(px(4.))
                            .children(self.tabs.iter().enumerate().map(|(index, (id, _))| {
                                div()
                                    .id(("terminal-tab", *id))
                                    .role(gpui::Role::Tab)
                                    .aria_selected(index == self.active)
                                    .aria_label(format!("终端 {}", index + 1))
                                    .h(px(28.))
                                    .w(px(156.))
                                    .min_w(px(80.))
                                    .px(px(8.))
                                    .rounded(px(10.))
                                    .when(index == self.active, |s| s.bg(theme.text.alpha(0.05)))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.active = index;
                                        this.focus(cx);
                                        cx.notify();
                                    }))
                                    .child(icon("panel-terminal", theme.text.into()).size(px(16.)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .overflow_hidden()
                                            .text_size(px(13.))
                                            .text_color(theme.text)
                                            .child(title.clone()),
                                    )
                                    .child(
                                        div()
                                            .id(("close-terminal", *id))
                                            .role(gpui::Role::Button)
                                            .aria_label(format!("关闭终端 {}", index + 1))
                                            .size(px(20.))
                                            .flex_none()
                                            .rounded(px(5.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(theme.sidebar_hover))
                                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                cx.stop_propagation()
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.tabs.remove(index);
                                                this.active = this
                                                    .active
                                                    .saturating_sub(usize::from(
                                                        index <= this.active,
                                                    ))
                                                    .min(this.tabs.len().saturating_sub(1));
                                                this.focus(cx);
                                                cx.notify();
                                            }))
                                            .child(
                                                icon("close-dialog", theme.text_tertiary.into())
                                                    .size(px(12.)),
                                            ),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .id("new-terminal")
                            .role(gpui::Role::Button)
                            .aria_label("新建终端")
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(theme.text_tertiary)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|this, _, _, cx| this.add(cx)))
                            .child("+"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .pl(px(16.))
                    .pr(px(18.))
                    .pt(px(8.))
                    .pb(px(12.))
                    .when_some(
                        self.tabs.get(self.active).map(|(_, t)| t.clone()),
                        |body, tab| body.child(tab),
                    )
                    .when(self.tabs.is_empty(), |body| {
                        body.flex().items_center().justify_center().child(
                            div()
                                .id("restart-terminal")
                                .p(px(10.))
                                .rounded(px(10.))
                                .bg(theme.text.alpha(0.05))
                                .cursor_pointer()
                                .text_color(theme.text)
                                .on_click(cx.listener(|this, _, _, cx| this.add(cx)))
                                .child("新建终端"),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn output_until(rx: &async_channel::Receiver<Output>, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut output = String::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(Output::Bytes(bytes)) => output.push_str(&String::from_utf8_lossy(&bytes)),
                Ok(Output::Error(error)) => panic!("PTY failed: {error}"),
                Ok(Output::Exit(status)) => {
                    output.push_str(&status);
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
            if output.contains(needle) {
                return output;
            }
        }
        panic!("timed out waiting for {needle:?}: {output:?}");
    }

    #[test]
    fn real_pty_preserves_cwd_input_resize_and_exit_status() {
        let cwd = std::env::temp_dir();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf 'READY\\n'; read value; printf 'RECEIVED:%s\\n' \"$value\"; pwd; stty size; exit 7"]);
        command.cwd(&cwd);
        let (process, output) = Process::spawn(command).unwrap();
        output_until(&output, "READY");
        process
            .master
            .resize(PtySize {
                rows: 37,
                cols: 93,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        process
            .input
            .as_ref()
            .unwrap()
            .send_blocking("中文 ✓\n".as_bytes().to_vec())
            .unwrap();
        let result = output_until(&output, "进程已退出（7）");
        assert!(result.contains("RECEIVED:中文 ✓"), "{result:?}");
        assert!(result.contains("37 93"), "{result:?}");
        assert!(
            result.contains(cwd.canonicalize().unwrap().to_str().unwrap()),
            "{result:?}"
        );
    }

    #[test]
    fn ctrl_c_interrupts_foreground_job_and_drop_closes_shell() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            "trap 'printf INTERRUPTED; exit 0' INT; printf READY; while :; do sleep 1; done",
        ]);
        let (process, output) = Process::spawn(command).unwrap();
        output_until(&output, "READY");
        process
            .input
            .as_ref()
            .unwrap()
            .send_blocking(vec![3])
            .unwrap();
        output_until(&output, "INTERRUPTED");
        drop(process);
    }

    #[test]
    fn fragmented_unicode_ansi_queries_and_alternate_screen() {
        let mut parser =
            vt100::Parser::new_with_callbacks(4, 20, 100, TerminalCallbacks::default());
        for byte in "\x1b[31m中文\x1b[0m ✓\x1b[6n".as_bytes() {
            parser.process(&[*byte]);
        }
        assert_eq!(parser.screen().contents(), "中文 ✓");
        assert_eq!(
            parser.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(1)
        );
        assert_eq!(parser.callbacks().replies, vec![b"\x1b[1;7R".to_vec()]);
        parser.process(b"\x1b[?1049hTUI\x1b[?1h\x1b[?2004h");
        assert!(parser.screen().alternate_screen());
        assert!(parser.screen().application_cursor());
        assert!(parser.screen().bracketed_paste());
        parser.process(b"\x1b[?1049l");
        assert_eq!(parser.screen().contents(), "中文 ✓");
    }

    #[test]
    fn native_input_clipboard_and_escape_reach_the_terminal_once() {
        let mut app = gpui::TestApp::new();
        app.update(|cx| {
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                crate::app::DismissPermissionUi,
                None,
            )]);
            init(cx);
        });
        let mut window = app.open_window(|_, cx| {
            TerminalView::with_process(
                Process::spawn(CommandBuilder::new("/bin/cat")),
                ThemeMode::Dark,
                cx,
            )
        });
        let (input, received) = async_channel::unbounded();
        let _real_input =
            window.update(|view, _, _| view.process.as_mut().unwrap().input.replace(input));
        window.draw();
        window.simulate_click(point(px(20.), px(20.)), MouseButton::Left);
        window.update(|view, window, _| assert!(view.focus.is_focused(window)));
        window.update(|view, _, cx| {
            view.parser.process(b"SELECTION_TEST");
            cx.notify();
        });
        window.draw();
        let bounds = window.read(|view, _| view.bounds.unwrap());
        window.simulate_mouse_down(bounds.origin + point(px(1.), px(5.)), MouseButton::Left);
        window.simulate_mouse_move(bounds.origin + point(px(CELL_WIDTH * 9.), px(5.)));
        window.simulate_mouse_up(
            bounds.origin + point(px(CELL_WIDTH * 9.), px(5.)),
            MouseButton::Left,
        );
        assert_eq!(
            window.read(|view, _| view.selected_text()),
            "SELECTION",
            "{:?}",
            window.read(|view, _| (view.bounds, view.selection, view.parser.screen().contents()))
        );
        window.simulate_keystroke("cmd-c");
        assert_eq!(
            app.read_from_clipboard().and_then(|c| c.text()),
            Some("SELECTION".into())
        );
        window.simulate_input("中文 ✓");
        assert_eq!(
            std::iter::from_fn(|| received.try_recv().ok())
                .flatten()
                .collect::<Vec<_>>(),
            "中文 ✓".as_bytes()
        );
        app.write_to_clipboard(ClipboardItem::new_string("粘贴内容".into()));
        window.simulate_keystroke("cmd-v");
        assert_eq!(received.try_recv().unwrap(), "粘贴内容".as_bytes());
        assert!(
            received.try_recv().is_err(),
            "paste must not be duplicated by action and key handlers"
        );
        window.simulate_keystroke("escape");
        assert_eq!(received.try_recv().unwrap(), vec![27]);
        window.update(|view, _, _| view.parser.process(b"\x1b[?1049h\x1b[?1h\x1b[?2004h"));
        window.simulate_keystroke("cmd-k");
        assert_eq!(received.try_recv().unwrap(), vec![12]);
        window.read(|view, _| {
            assert!(view.parser.screen().alternate_screen());
            assert!(view.parser.screen().application_cursor());
            assert!(view.parser.screen().bracketed_paste());
        });
    }

    #[test]
    fn terminal_keys_preserve_shell_and_application_modes() {
        let key = |s| gpui::Keystroke::parse(s).unwrap();
        assert_eq!(key_bytes(&key("ctrl-c"), false), Some(vec![3]));
        assert_eq!(key_bytes(&key("up"), true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(&key("up"), false), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            key_bytes(&key("shift-tab"), false),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(key_bytes(&key("ctrl-`"), false), None);
    }
}
