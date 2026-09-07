//! Native presentation for `item/permissions/requestApproval`.
//!
//! This module intentionally contains no app-server protocol types. It is
//! populated through protocol-neutral domain data by the production Composer.
//! Geometry, copy, colors, shadows, menu dimensions, and interaction states come from
//! the real ChatGPT renderer captures in
//! `artifacts/chatgpt-permissions-request-cdp-audit-2026-08-30/R01..R23`.
//!
//! An approved or declined card remains in conversation state while awaiting
//! `serverRequest/resolved`, but is no longer interactive. Cancellation and
//! transport failures stay visible with an explicit terminal status.

use std::path::Path;

use gpui::{BoxShadow, Div, FontWeight, Role, SharedString, Stateful, div, prelude::*, px, rgba};

use crate::{
    components::{callback::UiCallback, icons::icon},
    theme::{Theme, ThemeMode},
};

pub const PERMISSIONS_CARD_RADIUS: f32 = 25.0;
pub const PERMISSIONS_HEADER_HEIGHT: f32 = 76.0;
pub const PERMISSIONS_HEADER_WITH_REASON_HEIGHT: f32 = 97.5;
pub const PERMISSIONS_ACTIONS_HEIGHT: f32 = 52.0;
pub const PERMISSIONS_BUTTON_HEIGHT: f32 = 28.0;
pub const PERMISSIONS_DECLINE_WIDTH: f32 = 80.156_25;
pub const PERMISSIONS_ALLOW_ONCE_WIDTH: f32 = 93.140_625;
pub const PERMISSIONS_MENU_TOGGLE_WIDTH: f32 = 23.0;
pub const PERMISSIONS_MENU_WIDTH: f32 = 168.0;
pub const PERMISSIONS_MENU_HEIGHT: f32 = 67.125;
pub const PERMISSIONS_MENU_ROW_HEIGHT: f32 = 28.5625;

fn element_id(prefix: &str, request_id: &str) -> SharedString {
    format!("{prefix}-{request_id}").into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionPathAccess {
    Read,
    Write,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionPathRequest {
    pub path: String,
    pub access: PermissionPathAccess,
}

impl PermissionPathRequest {
    pub fn new(path: impl Into<String>, access: PermissionPathAccess) -> Self {
        Self {
            path: path.into(),
            access,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionFileAccess {
    Read,
    Write,
    ReadWrite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionActionPresentation {
    Network,
    FileSystem {
        access: PermissionFileAccess,
        paths: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionApprovalStatus {
    #[default]
    Pending,
    Approved,
    Declined,
    Resolved,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionApprovalDecision {
    AllowOnce,
    AllowForConversation,
    Decline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionApprovalMenuItem {
    AllowOnce,
    AllowForConversation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionApprovalKeyboardFocus {
    Decline,
    AllowOnce,
    MenuToggle,
    MenuAllowOnce,
    MenuAllowForConversation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionApprovalHover {
    Decline,
    Allow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionApprovalVisualState {
    #[default]
    Default,
    AllowHovered,
    DeclineHovered,
    Menu {
        focused: Option<PermissionApprovalMenuItem>,
    },
}

impl PermissionApprovalVisualState {
    pub fn menu_open(self) -> bool {
        matches!(self, Self::Menu { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionQuestionPart {
    Text(String),
    Path { full_path: String, label: String },
}

impl PermissionQuestionPart {
    fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    fn path(value: impl Into<String>) -> Self {
        let full_path = value.into();
        let label = permission_path_label(&full_path);
        Self::Path { full_path, label }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionApprovalPresentation {
    pub request_id: String,
    pub network_enabled: bool,
    pub file_system: Vec<PermissionPathRequest>,
    pub cwd: Option<String>,
    pub reason: Option<String>,
    pub failure_message: Option<String>,
    pub status: PermissionApprovalStatus,
    pub visual_state: PermissionApprovalVisualState,
    pub keyboard_focus: Option<PermissionApprovalKeyboardFocus>,
}

impl PermissionApprovalPresentation {
    pub fn network(request_id: impl Into<String>, reason: Option<String>) -> Self {
        Self::pending(request_id, true, Vec::new(), reason)
    }

    pub fn file_system(
        request_id: impl Into<String>,
        file_system: Vec<PermissionPathRequest>,
        reason: Option<String>,
    ) -> Self {
        Self::pending(request_id, false, file_system, reason)
    }

    pub fn combined(
        request_id: impl Into<String>,
        file_system: Vec<PermissionPathRequest>,
        reason: Option<String>,
    ) -> Self {
        Self::pending(request_id, true, file_system, reason)
    }

    pub fn pending(
        request_id: impl Into<String>,
        network_enabled: bool,
        file_system: Vec<PermissionPathRequest>,
        reason: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            network_enabled,
            file_system,
            cwd: None,
            reason,
            failure_message: None,
            status: PermissionApprovalStatus::Pending,
            visual_state: PermissionApprovalVisualState::Default,
            keyboard_focus: None,
        }
    }

    pub fn should_render(&self) -> bool {
        (self.status == PermissionApprovalStatus::Pending && !self.actions().is_empty())
            || matches!(
                self.status,
                PermissionApprovalStatus::Cancelled | PermissionApprovalStatus::Failed
            )
    }

    pub fn is_interactive(&self) -> bool {
        self.status == PermissionApprovalStatus::Pending && !self.actions().is_empty()
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn title(&self) -> &'static str {
        if self.network_enabled && self.visible_file_paths().is_empty() {
            "互联网访问"
        } else {
            "权限"
        }
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd
            .as_deref()
            .map(str::trim)
            .filter(|cwd| !cwd.is_empty())
    }

    pub fn header_height(&self) -> f32 {
        let base = if self.reason().is_some() {
            PERMISSIONS_HEADER_WITH_REASON_HEIGHT
        } else {
            PERMISSIONS_HEADER_HEIGHT
        };
        base + if self.cwd().is_some() { 19.5 } else { 0.0 }
    }

    pub fn card_height(&self) -> f32 {
        self.header_height() + PERMISSIONS_ACTIONS_HEIGHT
    }

    pub fn menu_top(&self) -> f32 {
        self.card_height() - 44.0 - PERMISSIONS_MENU_HEIGHT - 1.875
    }

    /// Mirrors the installed renderer's `Pvs` conversion: deny entries are
    /// omitted, identical read+write paths become one readWrite action, and
    /// action order is network, readWrite, read, write.
    pub fn actions(&self) -> Vec<PermissionActionPresentation> {
        let mut actions = Vec::new();
        if self.network_enabled {
            actions.push(PermissionActionPresentation::Network);
        }

        let mut reads = Vec::new();
        let mut writes = Vec::new();
        for entry in &self.file_system {
            match entry.access {
                PermissionPathAccess::Read => push_unique(&mut reads, &entry.path),
                PermissionPathAccess::Write => push_unique(&mut writes, &entry.path),
                PermissionPathAccess::Deny => {}
            }
        }

        let read_write = reads
            .iter()
            .filter(|path| writes.contains(path))
            .cloned()
            .collect::<Vec<_>>();
        let read = reads
            .into_iter()
            .filter(|path| !read_write.contains(path))
            .collect::<Vec<_>>();
        let write = writes
            .into_iter()
            .filter(|path| !read_write.contains(path))
            .collect::<Vec<_>>();

        push_file_action(&mut actions, PermissionFileAccess::ReadWrite, read_write);
        push_file_action(&mut actions, PermissionFileAccess::Read, read);
        push_file_action(&mut actions, PermissionFileAccess::Write, write);
        actions
    }

    pub fn question_parts(&self) -> Vec<PermissionQuestionPart> {
        let actions = self.actions();
        if actions.is_empty() {
            return vec![PermissionQuestionPart::text(
                "Codex 未请求额外文件或网络权限，是否继续？",
            )];
        }
        if actions.len() == 1 {
            return single_action_question(&actions[0]);
        }

        let mut parts = vec![PermissionQuestionPart::text("允许 ChatGPT ")];
        for (index, action) in actions.iter().enumerate() {
            if index > 0 {
                parts.push(PermissionQuestionPart::text(
                    if index + 1 == actions.len() {
                        "和"
                    } else {
                        "、"
                    },
                ));
            }
            append_action_phrase(&mut parts, action);
        }
        parts.push(PermissionQuestionPart::text("？"));
        parts
    }

    pub fn question_text(&self) -> String {
        self.question_parts()
            .into_iter()
            .map(|part| match part {
                PermissionQuestionPart::Text(text) => text,
                PermissionQuestionPart::Path { label, .. } => label,
            })
            .collect()
    }

    pub fn keyboard_event(&self, key: &str, shift: bool) -> Option<PermissionApprovalEvent> {
        match key {
            "tab" => Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                if self.visual_state.menu_open() {
                    match (self.keyboard_focus, shift) {
                        (Some(PermissionApprovalKeyboardFocus::MenuAllowOnce), false) => {
                            PermissionApprovalKeyboardFocus::MenuAllowForConversation
                        }
                        (
                            Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation),
                            false,
                        ) => PermissionApprovalKeyboardFocus::MenuAllowOnce,
                        (Some(PermissionApprovalKeyboardFocus::MenuAllowOnce), true) => {
                            PermissionApprovalKeyboardFocus::MenuAllowForConversation
                        }
                        (Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation), true) => {
                            PermissionApprovalKeyboardFocus::MenuAllowOnce
                        }
                        (_, true) => PermissionApprovalKeyboardFocus::MenuAllowForConversation,
                        _ => PermissionApprovalKeyboardFocus::MenuAllowOnce,
                    }
                } else {
                    match (self.keyboard_focus, shift) {
                        (Some(PermissionApprovalKeyboardFocus::Decline), false) => {
                            PermissionApprovalKeyboardFocus::AllowOnce
                        }
                        (Some(PermissionApprovalKeyboardFocus::AllowOnce), false) => {
                            PermissionApprovalKeyboardFocus::MenuToggle
                        }
                        (Some(PermissionApprovalKeyboardFocus::MenuToggle), false) => {
                            PermissionApprovalKeyboardFocus::Decline
                        }
                        (Some(PermissionApprovalKeyboardFocus::Decline), true) => {
                            PermissionApprovalKeyboardFocus::MenuToggle
                        }
                        (Some(PermissionApprovalKeyboardFocus::AllowOnce), true) => {
                            PermissionApprovalKeyboardFocus::Decline
                        }
                        (Some(PermissionApprovalKeyboardFocus::MenuToggle), true) => {
                            PermissionApprovalKeyboardFocus::AllowOnce
                        }
                        (_, true) => PermissionApprovalKeyboardFocus::MenuToggle,
                        _ => PermissionApprovalKeyboardFocus::Decline,
                    }
                },
            ))),
            "escape" if self.visual_state.menu_open() => Some(PermissionApprovalEvent::ToggleMenu),
            "escape" => Some(PermissionApprovalEvent::Decision(
                PermissionApprovalDecision::Decline,
            )),
            "enter" | "space" => match self.keyboard_focus {
                Some(PermissionApprovalKeyboardFocus::Decline) => Some(
                    PermissionApprovalEvent::Decision(PermissionApprovalDecision::Decline),
                ),
                Some(PermissionApprovalKeyboardFocus::MenuToggle) => {
                    Some(PermissionApprovalEvent::ToggleMenu)
                }
                Some(PermissionApprovalKeyboardFocus::MenuAllowOnce) => Some(
                    PermissionApprovalEvent::Decision(PermissionApprovalDecision::AllowOnce),
                ),
                Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation) => {
                    Some(PermissionApprovalEvent::Decision(
                        PermissionApprovalDecision::AllowForConversation,
                    ))
                }
                Some(PermissionApprovalKeyboardFocus::AllowOnce) | None
                    if !self.visual_state.menu_open() =>
                {
                    Some(PermissionApprovalEvent::Decision(
                        PermissionApprovalDecision::AllowOnce,
                    ))
                }
                _ => None,
            },
            "down" if self.visual_state.menu_open() => Some(
                PermissionApprovalEvent::KeyboardFocusChanged(Some(match self.keyboard_focus {
                    Some(PermissionApprovalKeyboardFocus::MenuAllowOnce) => {
                        PermissionApprovalKeyboardFocus::MenuAllowForConversation
                    }
                    _ => PermissionApprovalKeyboardFocus::MenuAllowOnce,
                })),
            ),
            "up" if self.visual_state.menu_open() => Some(
                PermissionApprovalEvent::KeyboardFocusChanged(Some(match self.keyboard_focus {
                    Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation) => {
                        PermissionApprovalKeyboardFocus::MenuAllowOnce
                    }
                    _ => PermissionApprovalKeyboardFocus::MenuAllowForConversation,
                })),
            ),
            _ => None,
        }
    }

    fn visible_file_paths(&self) -> Vec<&str> {
        self.file_system
            .iter()
            .filter(|entry| entry.access != PermissionPathAccess::Deny)
            .map(|entry| entry.path.as_str())
            .collect()
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|candidate| candidate == value) {
        values.push(value.to_owned());
    }
}

fn push_file_action(
    actions: &mut Vec<PermissionActionPresentation>,
    access: PermissionFileAccess,
    paths: Vec<String>,
) {
    if !paths.is_empty() {
        actions.push(PermissionActionPresentation::FileSystem { access, paths });
    }
}

fn permission_path_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_owned()
}

fn single_action_question(action: &PermissionActionPresentation) -> Vec<PermissionQuestionPart> {
    match action {
        PermissionActionPresentation::Network => {
            vec![PermissionQuestionPart::text("允许 ChatGPT 连接互联网？")]
        }
        PermissionActionPresentation::FileSystem { access, paths } => {
            let mut parts = vec![PermissionQuestionPart::text(match access {
                PermissionFileAccess::Read => "允许 ChatGPT 查看 ",
                PermissionFileAccess::Write => "允许 ChatGPT 编辑 ",
                PermissionFileAccess::ReadWrite => "允许 ChatGPT 查看和编辑 ",
            })];
            append_paths(&mut parts, paths);
            parts.push(PermissionQuestionPart::text(" 的内容吗？"));
            parts
        }
    }
}

fn append_action_phrase(
    parts: &mut Vec<PermissionQuestionPart>,
    action: &PermissionActionPresentation,
) {
    match action {
        PermissionActionPresentation::Network => {
            parts.push(PermissionQuestionPart::text("连接到互联网"));
        }
        PermissionActionPresentation::FileSystem { access, paths } => {
            parts.push(PermissionQuestionPart::text(match access {
                PermissionFileAccess::Read => "查看 ",
                PermissionFileAccess::Write => "编辑 ",
                PermissionFileAccess::ReadWrite => "查看和编辑 ",
            }));
            append_paths(parts, paths);
            parts.push(PermissionQuestionPart::text(" 的内容"));
        }
    }
}

fn append_paths(parts: &mut Vec<PermissionQuestionPart>, paths: &[String]) {
    for (index, path) in paths.iter().enumerate() {
        if index > 0 {
            parts.push(PermissionQuestionPart::text(if index + 1 == paths.len() {
                " 和 "
            } else {
                "、"
            }));
        }
        parts.push(PermissionQuestionPart::path(path));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionApprovalEvent {
    Decision(PermissionApprovalDecision),
    ToggleMenu,
    HoverChanged(Option<PermissionApprovalHover>),
    MenuFocusChanged(Option<PermissionApprovalMenuItem>),
    KeyboardFocusChanged(Option<PermissionApprovalKeyboardFocus>),
}

pub type PermissionApprovalCallback = UiCallback<PermissionApprovalEvent>;

#[derive(Clone, Copy)]
struct PermissionPalette {
    mode: ThemeMode,
    card: gpui::Rgba,
    card_outline: gpui::Rgba,
    text: gpui::Rgba,
    secondary: gpui::Rgba,
    secondary_icon: gpui::Rgba,
    description: gpui::Rgba,
    mention: gpui::Rgba,
    mention_icon: gpui::Rgba,
    button_border: gpui::Rgba,
    decline: gpui::Rgba,
    decline_hover: gpui::Rgba,
    approve: gpui::Rgba,
    approve_hover: gpui::Rgba,
    approve_text: gpui::Rgba,
    menu: gpui::Rgba,
    menu_focus: gpui::Rgba,
    focus_ring: gpui::Rgba,
}

impl PermissionPalette {
    fn for_theme(theme: Theme) -> Self {
        if theme.surface == rgba(0x181818ff) {
            Self {
                mode: ThemeMode::Dark,
                // The captured 96% surface composites to #2c2c2c over the
                // app's #181818 conversation background.
                card: rgba(0x2c2c2cff),
                // A CSS 0.5 px spread only covers half of its edge pixel.
                // GPUI's shadow primitive covers that device pixel fully, so
                // halve the captured alpha to preserve the same result.
                card_outline: rgba(0xffffff14),
                text: rgba(0xdfdfdfff),
                // CoreText's glyph coverage is denser than Chromium's. These
                // are the calibrated text alphas; vector icons retain the
                // computed CSS alpha below.
                secondary: rgba(0xdfdfdf80),
                secondary_icon: rgba(0xdfdfdfa6),
                description: rgba(0xffffff63),
                mention: rgba(0x95c9f9ff),
                mention_icon: rgba(0xffffff7f),
                button_border: rgba(0xffffff15),
                decline: rgba(0xffffff08),
                decline_hover: rgba(0xffffff14),
                approve: rgba(0xdfdfdfff),
                approve_hover: rgba(0xdfdfdfcc),
                approve_text: rgba(0x2d2d2ddb),
                menu: rgba(0x2d2d2dff),
                menu_focus: rgba(0xffffff14),
                focus_ring: rgba(0x83c3ffc2),
            }
        } else {
            Self {
                mode: ThemeMode::Light,
                // The captured 96% surface composites to white over the
                // light conversation background.
                card: rgba(0xffffffff),
                card_outline: rgba(0x1a1c1f10),
                text: rgba(0x1a1c1fff),
                secondary: rgba(0x1a1c1f8a),
                secondary_icon: rgba(0x1a1c1fa6),
                description: rgba(0x1a1c1f67),
                mention: rgba(0x2e82d2ff),
                mention_icon: rgba(0x1a1c1f7e),
                button_border: rgba(0x1a1c1f14),
                decline: rgba(0xffffffff),
                decline_hover: rgba(0x1a1c1f0e),
                approve: rgba(0x1a1c1fff),
                approve_hover: rgba(0x1a1c1fcc),
                approve_text: rgba(0xffffffb3),
                menu: rgba(0xffffffff),
                menu_focus: rgba(0x1a1c1f0e),
                focus_ring: rgba(0x339cffff),
            }
        }
    }

    fn card_shadows(self) -> Vec<BoxShadow> {
        let (short_shadow, ambient_shadow) = match self.mode {
            ThemeMode::Light => (rgba(0x0000000f), rgba(0x00000005)),
            ThemeMode::Dark => (rgba(0x0000000a), rgba(0x0000000d)),
        };
        vec![
            BoxShadow::new(px(1.5), px(0.0), self.card_outline.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(3.0), short_shadow.into()).blur_radius(px(7.5)),
            BoxShadow::new(px(0.0), px(0.0), ambient_shadow.into()).blur_radius(px(20.0)),
        ]
    }

    fn menu_shadows(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow::new(px(0.0), px(0.0), self.button_border.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ]
    }

    fn focus_shadow(self, inset: bool) -> Vec<BoxShadow> {
        let shadow =
            BoxShadow::new(px(0.0), px(0.0), self.focus_ring.into()).spread_radius(px(2.0));
        vec![if inset { shadow.inset() } else { shadow }]
    }
}

/// Renders the interactive pending surface plus explicit cancellation/failure
/// outcomes. Approved, declined, and server-resolved presentations stay in
/// conversation state but return `None`.
pub fn render_permissions_approval(
    model: &PermissionApprovalPresentation,
    theme: Theme,
    callback: PermissionApprovalCallback,
) -> Option<Stateful<Div>> {
    if !model.should_render() {
        return None;
    }
    if !model.is_interactive() {
        return Some(render_permissions_status(model, theme));
    }

    let palette = PermissionPalette::for_theme(theme);
    let title = model.title();
    let question_text = model.question_text();
    let header = div()
        .id(element_id("permissions-header", &model.request_id))
        .role(Role::Alert)
        .aria_label(format!("{title}，{question_text}"))
        .h(px(model.header_height()))
        .min_w(px(0.0))
        .px(px(16.0))
        .pt(px(16.0))
        .pb(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .h(px(20.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.95))
                .line_height(px(20.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.secondary)
                .child(
                    icon("permission-request", palette.secondary_icon.into())
                        .size(px(18.0))
                        .relative()
                        .left(px(1.0)),
                )
                .child(title),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(render_question(model, palette))
                .when_some(model.reason().map(ToOwned::to_owned), |content, reason| {
                    content.child(
                        div()
                            .h(px(19.5))
                            .text_size(px(13.0))
                            .line_height(px(19.5))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(palette.description)
                            .child(reason),
                    )
                })
                .when_some(model.cwd().map(ToOwned::to_owned), |content, cwd| {
                    content.child(
                        div()
                            .h(px(19.5))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(palette.description)
                            .child(format!("工作目录：{cwd}")),
                    )
                }),
        );

    let decline_callback = callback.clone();
    let decline_hover_callback = callback.clone();
    let decline_focused = model.keyboard_focus == Some(PermissionApprovalKeyboardFocus::Decline);
    let decline = div()
        .id(element_id("permissions-decline", &model.request_id))
        .role(Role::Button)
        .aria_label("拒绝")
        .h(px(PERMISSIONS_BUTTON_HEIGHT))
        .w(px(PERMISSIONS_DECLINE_WIDTH))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(9999.0))
        .border_1()
        .border_color(palette.button_border)
        .bg(
            if model.visual_state == PermissionApprovalVisualState::DeclineHovered {
                palette.decline_hover
            } else {
                palette.decline
            },
        )
        .when(decline_focused, |button| {
            button.shadow(palette.focus_shadow(false))
        })
        .text_size(px(12.95))
        .line_height(px(18.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(if palette.mode == ThemeMode::Dark {
            rgba(0xdfdfdfca)
        } else {
            rgba(0x1a1c1fdb)
        })
        .cursor_pointer()
        .hover(move |button| button.bg(palette.decline_hover))
        .on_hover(move |hovered, window, cx| {
            decline_hover_callback.emit(
                PermissionApprovalEvent::HoverChanged(
                    (*hovered).then_some(PermissionApprovalHover::Decline),
                ),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            decline_callback.emit(
                PermissionApprovalEvent::Decision(PermissionApprovalDecision::Decline),
                window,
                cx,
            );
        })
        .child("拒绝")
        .child(keycap(
            "Esc",
            if palette.mode == ThemeMode::Dark {
                rgba(0xdfdfdfca)
            } else {
                rgba(0x1a1c1fdb)
            },
        ));

    let allow_focused = model.keyboard_focus == Some(PermissionApprovalKeyboardFocus::AllowOnce);
    let approve_fill = if matches!(
        model.visual_state,
        PermissionApprovalVisualState::AllowHovered | PermissionApprovalVisualState::Menu { .. }
    ) {
        palette.approve_hover
    } else {
        palette.approve
    };
    let approve_callback = callback.clone();
    let approve_hover_callback = callback.clone();
    let approve = div()
        .id(element_id("permissions-allow-once", &model.request_id))
        .role(Role::Button)
        .aria_label("允许一次")
        .h(px(PERMISSIONS_BUTTON_HEIGHT))
        .w(px(PERMISSIONS_ALLOW_ONCE_WIDTH))
        .pl(px(8.0))
        .pr(px(4.0))
        .min_w(px(0.0))
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded_l(px(9999.0))
        .border_t_1()
        .border_b_1()
        .border_l_1()
        .border_color(palette.button_border)
        .bg(approve_fill)
        .when(allow_focused, |button| {
            button.shadow(palette.focus_shadow(true))
        })
        .text_size(px(12.95))
        .line_height(px(18.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(palette.approve_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.approve_hover))
        .on_hover(move |hovered, window, cx| {
            approve_hover_callback.emit(
                PermissionApprovalEvent::HoverChanged(
                    (*hovered).then_some(PermissionApprovalHover::Allow),
                ),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            approve_callback.emit(
                PermissionApprovalEvent::Decision(PermissionApprovalDecision::AllowOnce),
                window,
                cx,
            );
        })
        .child("允许一次")
        .child(keycap("⏎", palette.approve_text));

    let toggle_focused = model.keyboard_focus == Some(PermissionApprovalKeyboardFocus::MenuToggle);
    let toggle_callback = callback.clone();
    let toggle = div()
        .id(element_id("permissions-menu-toggle", &model.request_id))
        .role(Role::Button)
        .aria_label("审批选项")
        .h(px(PERMISSIONS_BUTTON_HEIGHT))
        .w(px(PERMISSIONS_MENU_TOGGLE_WIDTH))
        .pl(px(2.0))
        .pr(px(6.0))
        .flex()
        .items_center()
        .rounded_r(px(9999.0))
        .border_t_1()
        .border_r_1()
        .border_b_1()
        .border_color(palette.button_border)
        .bg(approve_fill)
        .when(toggle_focused, |button| {
            button.shadow(palette.focus_shadow(true))
        })
        .text_color(palette.approve_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.approve_hover))
        .on_click(move |_, window, cx| {
            toggle_callback.emit(PermissionApprovalEvent::ToggleMenu, window, cx);
        })
        .child(
            icon("chevron-down", palette.approve_text.alpha(0.50).into())
                .size(px(14.0))
                .relative()
                .left(px(1.0))
                .top(px(-1.0)),
        );

    let actions = div()
        .h(px(PERMISSIONS_ACTIONS_HEIGHT))
        .px(px(16.0))
        .pt(px(8.0))
        .pb(px(16.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().flex_1())
        .child(decline)
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .items_stretch()
                .overflow_hidden()
                .rounded(px(9999.0))
                .child(approve)
                .child(toggle),
        );

    let card = div()
        .h(px(model.card_height()))
        .w_full()
        .overflow_hidden()
        .rounded(px(PERMISSIONS_CARD_RADIUS))
        .bg(palette.card)
        .shadow(palette.card_shadows())
        .child(header)
        .child(actions);

    let mut result = div()
        .id(element_id("permissions-card", &model.request_id))
        .relative()
        // Home's capture overlay already preserves the renderer's fractional
        // conversation-column phase. Do not add that offset a second time.
        .left(px(0.0))
        .h(px(model.card_height()))
        .w_full()
        .child(card);

    if let PermissionApprovalVisualState::Menu { focused } = model.visual_state {
        result = result.child(render_menu(
            &model.request_id,
            model.menu_top(),
            focused,
            palette,
            callback,
        ));
    }

    Some(result)
}

fn render_permissions_status(
    model: &PermissionApprovalPresentation,
    theme: Theme,
) -> Stateful<Div> {
    let palette = PermissionPalette::for_theme(theme);
    let status = match model.status {
        PermissionApprovalStatus::Cancelled => "权限请求已取消",
        PermissionApprovalStatus::Failed => "权限请求失败",
        PermissionApprovalStatus::Pending => "等待审批",
        PermissionApprovalStatus::Approved => "已允许",
        PermissionApprovalStatus::Declined => "已拒绝",
        PermissionApprovalStatus::Resolved => "已完成",
    };
    div()
        .id(element_id("permissions-card", &model.request_id))
        .role(Role::Alert)
        .aria_label(status)
        .min_h(px(104.0))
        .w_full()
        .px(px(16.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .rounded(px(PERMISSIONS_CARD_RADIUS))
        .bg(palette.card)
        .shadow(palette.card_shadows())
        .text_color(palette.text)
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .child(status),
        )
        .when_some(model.failure_message.clone(), |card, message| {
            card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.description)
                    .child(message),
            )
        })
}

fn render_question(model: &PermissionApprovalPresentation, palette: PermissionPalette) -> Div {
    let mut line = div()
        .h(px(20.0))
        .min_w(px(0.0))
        .flex()
        .items_center()
        .overflow_hidden()
        .text_size(px(14.0))
        .line_height(px(20.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(palette.text);
    for part in model.question_parts() {
        line = match part {
            PermissionQuestionPart::Text(text) => line.child(text),
            PermissionQuestionPart::Path { full_path, label } => line.child(
                div()
                    .id(element_id("permissions-path", &full_path))
                    .aria_label(full_path)
                    .h(px(20.0))
                    .px(px(2.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .text_color(palette.mention)
                    .child(
                        icon("settings-folder-reference", palette.mention_icon.into())
                            .size(px(16.0)),
                    )
                    .child(label),
            ),
        };
    }
    line
}

fn keycap(label: &'static str, color: gpui::Rgba) -> Div {
    div()
        .h(px(16.0))
        .min_w(px(16.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(color.alpha(0.10))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(color)
        .child(label)
}

fn render_menu(
    request_id: &str,
    top: f32,
    focused: Option<PermissionApprovalMenuItem>,
    palette: PermissionPalette,
    callback: PermissionApprovalCallback,
) -> Stateful<Div> {
    div()
        .id(element_id("permissions-menu", request_id))
        .role(Role::Menu)
        .aria_label("审批选项")
        .absolute()
        .top(px(top))
        .right(px(16.0))
        .w(px(PERMISSIONS_MENU_WIDTH))
        .h(px(PERMISSIONS_MENU_HEIGHT))
        .p(px(4.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .overflow_hidden()
        .rounded(px(15.0))
        .bg(palette.menu)
        .shadow(palette.menu_shadows())
        .child(menu_row(
            element_id("permissions-menu-once", request_id),
            "允许一次",
            PermissionApprovalMenuItem::AllowOnce,
            focused == Some(PermissionApprovalMenuItem::AllowOnce),
            palette,
            callback.clone(),
        ))
        .child(menu_row(
            element_id("permissions-menu-conversation", request_id),
            "允许此对话",
            PermissionApprovalMenuItem::AllowForConversation,
            focused == Some(PermissionApprovalMenuItem::AllowForConversation),
            palette,
            callback,
        ))
}

fn menu_row(
    id: SharedString,
    label: &'static str,
    item: PermissionApprovalMenuItem,
    focused: bool,
    palette: PermissionPalette,
    callback: PermissionApprovalCallback,
) -> Stateful<Div> {
    let hover_callback = callback.clone();
    let click_callback = callback;
    div()
        .id(id)
        .role(Role::MenuItem)
        .h(px(PERMISSIONS_MENU_ROW_HEIGHT))
        .w_full()
        .px(px(8.0))
        .py(px(5.0))
        .flex()
        .items_center()
        .rounded(px(12.5))
        .when(focused, |row| row.bg(palette.menu_focus))
        .text_size(px(13.0))
        .line_height(px(18.5714))
        .font_weight(FontWeight::NORMAL)
        .text_color(palette.text)
        .cursor_pointer()
        .hover(move |row| row.bg(palette.menu_focus))
        .on_hover(move |hovered, window, cx| {
            hover_callback.emit(
                PermissionApprovalEvent::MenuFocusChanged((*hovered).then_some(item)),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            click_callback.emit(
                PermissionApprovalEvent::Decision(match item {
                    PermissionApprovalMenuItem::AllowOnce => PermissionApprovalDecision::AllowOnce,
                    PermissionApprovalMenuItem::AllowForConversation => {
                        PermissionApprovalDecision::AllowForConversation
                    }
                }),
                window,
                cx,
            );
        })
        .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &str) -> PermissionPathRequest {
        PermissionPathRequest::new(path, PermissionPathAccess::Read)
    }

    fn write(path: &str) -> PermissionPathRequest {
        PermissionPathRequest::new(path, PermissionPathAccess::Write)
    }

    #[test]
    fn network_fixture_matches_r01_copy_and_geometry() {
        let model = PermissionApprovalPresentation::network(
            "network-1",
            Some("Connect to example.com to verify the integration.".to_owned()),
        );
        assert_eq!(model.title(), "互联网访问");
        assert_eq!(model.question_text(), "允许 ChatGPT 连接互联网？");
        assert_eq!(model.header_height(), 97.5);
        assert_eq!(model.card_height(), 149.5);
        assert_eq!(model.menu_top(), 36.5);
    }

    #[test]
    fn filesystem_fixture_matches_r10_copy() {
        let model = PermissionApprovalPresentation::file_system(
            "filesystem-1",
            vec![read("/Users/zp/Downloads")],
            Some("Inspect downloaded fixtures needed by this task.".to_owned()),
        );
        assert_eq!(model.title(), "权限");
        assert_eq!(
            model.question_text(),
            "允许 ChatGPT 查看 Downloads 的内容吗？"
        );
    }

    #[test]
    fn live_permission_context_keeps_cwd_visible_without_changing_capture_fixtures() {
        let model = PermissionApprovalPresentation::network("network-cwd", None)
            .with_cwd("/workspace/project");
        assert_eq!(model.cwd(), Some("/workspace/project"));
        assert_eq!(model.header_height(), PERMISSIONS_HEADER_HEIGHT + 19.5);
        assert!(model.should_render());
    }

    #[test]
    fn combined_fixture_matches_r11_copy_and_action_order() {
        let model = PermissionApprovalPresentation::combined(
            "combined-1",
            vec![read("/Users/zp/Downloads"), write("/Users/zp/Desktop/GPUI")],
            Some("Download a fixture and store the generated result.".to_owned()),
        );
        assert_eq!(
            model.question_text(),
            "允许 ChatGPT 连接到互联网、查看 Downloads 的内容和编辑 GPUI 的内容？"
        );
        assert_eq!(
            model.actions(),
            vec![
                PermissionActionPresentation::Network,
                PermissionActionPresentation::FileSystem {
                    access: PermissionFileAccess::Read,
                    paths: vec!["/Users/zp/Downloads".to_owned()],
                },
                PermissionActionPresentation::FileSystem {
                    access: PermissionFileAccess::Write,
                    paths: vec!["/Users/zp/Desktop/GPUI".to_owned()],
                },
            ]
        );
    }

    #[test]
    fn duplicate_read_write_paths_merge_and_deny_is_hidden() {
        let model = PermissionApprovalPresentation::file_system(
            "filesystem-2",
            vec![
                read("/tmp/a"),
                write("/tmp/a"),
                read("/tmp/b"),
                PermissionPathRequest::new("/tmp/secret", PermissionPathAccess::Deny),
            ],
            None,
        );
        assert_eq!(
            model.actions(),
            vec![
                PermissionActionPresentation::FileSystem {
                    access: PermissionFileAccess::ReadWrite,
                    paths: vec!["/tmp/a".to_owned()],
                },
                PermissionActionPresentation::FileSystem {
                    access: PermissionFileAccess::Read,
                    paths: vec!["/tmp/b".to_owned()],
                },
            ]
        );
        assert_eq!(
            model.question_text(),
            "允许 ChatGPT 查看和编辑 a 的内容和查看 b 的内容？"
        );
    }

    #[test]
    fn terminal_outcomes_all_unmount() {
        let mut model = PermissionApprovalPresentation::network("network-2", None);
        let callback = PermissionApprovalCallback::new(|_, _, _| {});
        assert!(model.should_render());
        for status in [
            PermissionApprovalStatus::Approved,
            PermissionApprovalStatus::Declined,
            PermissionApprovalStatus::Resolved,
        ] {
            model.status = status;
            assert!(!model.should_render(), "{status:?}");
            assert!(
                render_permissions_approval(
                    &model,
                    Theme::for_mode(ThemeMode::Light),
                    callback.clone(),
                )
                .is_none(),
                "{status:?} must unmount rather than invent terminal chrome",
            );
        }
    }

    #[test]
    fn pending_hover_focus_and_menu_presentations_all_render() {
        let callback = PermissionApprovalCallback::new(|_, _, _| {});
        let mut model = PermissionApprovalPresentation::network("network-render", None);
        let states = [
            (
                PermissionApprovalVisualState::Default,
                Some(PermissionApprovalKeyboardFocus::Decline),
            ),
            (PermissionApprovalVisualState::AllowHovered, None),
            (PermissionApprovalVisualState::DeclineHovered, None),
            (
                PermissionApprovalVisualState::Menu { focused: None },
                Some(PermissionApprovalKeyboardFocus::MenuToggle),
            ),
            (
                PermissionApprovalVisualState::Menu {
                    focused: Some(PermissionApprovalMenuItem::AllowOnce),
                },
                Some(PermissionApprovalKeyboardFocus::MenuAllowOnce),
            ),
            (
                PermissionApprovalVisualState::Menu {
                    focused: Some(PermissionApprovalMenuItem::AllowForConversation),
                },
                Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation),
            ),
        ];

        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for (visual_state, keyboard_focus) in states {
                model.visual_state = visual_state;
                model.keyboard_focus = keyboard_focus;
                assert!(
                    render_permissions_approval(&model, Theme::for_mode(mode), callback.clone(),)
                        .is_some(),
                    "{mode:?} {visual_state:?} {keyboard_focus:?}",
                );
            }
        }
    }

    #[test]
    fn empty_or_deny_only_request_does_not_create_a_blank_card() {
        let empty = PermissionApprovalPresentation::file_system("empty", Vec::new(), None);
        let denied = PermissionApprovalPresentation::file_system(
            "deny-only",
            vec![PermissionPathRequest::new(
                "/tmp/hidden",
                PermissionPathAccess::Deny,
            )],
            None,
        );
        assert!(!empty.should_render());
        assert!(!denied.should_render());
    }

    #[test]
    fn keyboard_paths_cover_default_menu_and_terminal_decisions() {
        let mut model = PermissionApprovalPresentation::network("network-3", None);
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(PermissionApprovalEvent::Decision(
                PermissionApprovalDecision::AllowOnce
            ))
        );
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(PermissionApprovalEvent::Decision(
                PermissionApprovalDecision::Decline
            ))
        );
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                PermissionApprovalKeyboardFocus::Decline
            )))
        );
        model.keyboard_focus = Some(PermissionApprovalKeyboardFocus::Decline);
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                PermissionApprovalKeyboardFocus::AllowOnce
            )))
        );
        model.keyboard_focus = Some(PermissionApprovalKeyboardFocus::AllowOnce);
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                PermissionApprovalKeyboardFocus::MenuToggle
            )))
        );

        model.visual_state = PermissionApprovalVisualState::Menu { focused: None };
        model.keyboard_focus = None;
        assert_eq!(
            model.keyboard_event("down", false),
            Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                PermissionApprovalKeyboardFocus::MenuAllowOnce
            )))
        );
        model.keyboard_focus = Some(PermissionApprovalKeyboardFocus::MenuAllowOnce);
        assert_eq!(
            model.keyboard_event("down", false),
            Some(PermissionApprovalEvent::KeyboardFocusChanged(Some(
                PermissionApprovalKeyboardFocus::MenuAllowForConversation
            )))
        );
        model.keyboard_focus = Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation);
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(PermissionApprovalEvent::ToggleMenu)
        );
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(PermissionApprovalEvent::Decision(
                PermissionApprovalDecision::AllowForConversation
            ))
        );
    }

    #[test]
    fn light_and_dark_palettes_match_captured_computed_values() {
        let light = PermissionPalette::for_theme(Theme::for_mode(ThemeMode::Light));
        let dark = PermissionPalette::for_theme(Theme::for_mode(ThemeMode::Dark));
        assert_eq!(light.card, rgba(0xffffffff));
        assert_eq!(dark.card, rgba(0x2c2c2cff));
        assert_eq!(light.text, rgba(0x1a1c1fff));
        assert_eq!(dark.text, rgba(0xdfdfdfff));
        assert_eq!(light.decline_hover, rgba(0x1a1c1f0e));
        assert_eq!(dark.decline_hover, rgba(0xffffff14));
        assert_eq!(light.mention, rgba(0x2e82d2ff));
        assert_eq!(dark.mention, rgba(0x95c9f9ff));
        assert_eq!(light.mode, ThemeMode::Light);
        assert_eq!(dark.mode, ThemeMode::Dark);
    }
}
