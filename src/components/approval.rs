//! Native command and network approval surface.
//!
//! Geometry and colors are taken from the ChatGPT desktop CDP captures in
//! `artifacts/chatgpt-p0-ui-cdp-audit-2026-08-30/{06..10,17..21}-*.json`.
//! This module intentionally contains no app-server protocol types. The
//! caller supplies a presentation model and translates [`ApprovalCardEvent`]
//! back into its own state machine.

use gpui::{Div, FontWeight, SharedString, Stateful, div, prelude::*, px, rgba};

use crate::{components::callback::UiCallback, theme::Theme};

mod render;
pub use render::render_approval_card;

#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json)]
pub struct ApprovalShortcut(pub &'static str);

pub fn init(cx: &mut gpui::App) {
    for key in [
        "enter",
        "space",
        "tab",
        "shift-tab",
        "escape",
        "shift-escape",
        "up",
        "down",
    ] {
        cx.bind_keys([gpui::KeyBinding::new(
            key,
            ApprovalShortcut(key),
            Some("ApprovalCard"),
        )]);
    }
    for key in ["tab", "shift-tab", "escape", "shift-escape"] {
        cx.bind_keys([gpui::KeyBinding::new(
            key,
            ApprovalShortcut(key),
            Some("ApprovalText"),
        )]);
    }
}

pub const APPROVAL_CARD_RADIUS: f32 = 25.0;
pub const APPROVAL_CARD_HEADER_HEIGHT: f32 = 76.0;
pub const APPROVAL_CARD_PREVIEW_HEIGHT: f32 = 34.0;
pub const APPROVAL_CARD_ACTIONS_HEIGHT: f32 = 52.0;
pub const APPROVAL_BUTTON_HEIGHT: f32 = 28.0;
pub const APPROVAL_MENU_WIDTH: f32 = 168.0;
pub const APPROVAL_MENU_HEIGHT: f32 = 67.125;
pub const APPROVAL_MENU_ROW_HEIGHT: f32 = 28.5625;

/// The approval surface is removed as soon as the server resolves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApprovalCardStatus {
    #[default]
    Pending,
    Submitting,
    Failed,
    Resolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalRequestPresentation {
    Command {
        command: String,
        reason: Option<String>,
    },
    WriteStdin {
        input: String,
        reason: Option<String>,
    },
    Network {
        destination: String,
        /// Network approvals can be attached to a command execution. When it
        /// is present, ChatGPT shows the command in the monospace preview.
        command: Option<String>,
        reason: Option<String>,
    },
}

impl ApprovalRequestPresentation {
    pub fn command(command: impl Into<String>, reason: Option<String>) -> Self {
        Self::Command {
            command: command.into(),
            reason,
        }
    }

    pub fn network(
        destination: impl Into<String>,
        command: Option<String>,
        reason: Option<String>,
    ) -> Self {
        Self::Network {
            destination: destination.into(),
            command,
            reason,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Command { .. } => "终端",
            Self::WriteStdin { .. } => "终端",
            Self::Network { .. } => "互联网访问",
        }
    }

    pub fn question(&self) -> String {
        let reason = match self {
            Self::Command { reason, .. }
            | Self::WriteStdin { reason, .. }
            | Self::Network { reason, .. } => reason,
        };
        if let Some(reason) = reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            && !matches!(self, Self::Network { .. })
        {
            return reason.to_owned();
        }

        match self {
            Self::Command { .. } => "是否允许 ChatGPT 运行此命令？".to_owned(),
            Self::WriteStdin { .. } => "是否允许 ChatGPT 向正在运行的终端发送此输入？".to_owned(),
            Self::Network { destination, .. } => {
                format!("允许 ChatGPT 与 {destination} 建立连接？")
            }
        }
    }

    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Command { command, .. } => non_empty(command),
            Self::WriteStdin { input, .. } => non_empty(input),
            Self::Network { command, .. } => command.as_deref().and_then(non_empty),
        }
    }

    pub fn default_scope(&self) -> ApprovalScope {
        match self {
            Self::Command { .. } | Self::WriteStdin { .. } => ApprovalScope::SimilarCommands,
            Self::Network { .. } => ApprovalScope::InternetAccess,
        }
    }
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn approval_element_id(prefix: &str, request_id: &str) -> SharedString {
    format!("{prefix}-{request_id}").into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalScope {
    SimilarCommands,
    InternetAccess,
    Session,
}

impl ApprovalScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::SimilarCommands => "允许类似命令",
            Self::InternetAccess => "互联网访问",
            Self::Session => "允许此对话",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    AllowScoped(ApprovalScope),
    Decline,
    Cancel,
    /// Index into the request's ordered, schema-validated decision list.
    ServerChoice(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalMenuItem {
    AllowOnce,
    Scoped(ApprovalScope),
    Choice(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalKeyboardFocus {
    Decline,
    AllowOnce,
    MenuToggle,
    MenuAllowOnce,
    MenuScoped(ApprovalScope),
    Choice(usize),
    MenuChoice(usize),
    PreviewToggle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ApprovalVisualState {
    #[default]
    Default,
    ApproveHovered,
    DeclineHovered,
    SplitMenu {
        focused: Option<ApprovalMenuItem>,
    },
}

impl ApprovalVisualState {
    pub fn menu_open(self) -> bool {
        matches!(self, Self::SplitMenu { .. })
    }

    #[cfg(test)]
    fn focused_menu_item(self) -> Option<ApprovalMenuItem> {
        match self {
            Self::SplitMenu { focused } => focused,
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalCardViewModel {
    pub request_id: String,
    pub request: ApprovalRequestPresentation,
    pub status: ApprovalCardStatus,
    /// Whether the server listed `accept` in `availableDecisions`.
    pub allow_once: bool,
    /// Whether the server listed `decline` in `availableDecisions`.
    pub decline: bool,
    /// Whether `cancel` is available. It remains distinct from `decline` and
    /// interrupts the turn when selected.
    pub cancel: bool,
    /// Fallback scope used by presentation fixtures. Live requests use the
    /// complete ordered `server_choices` list.
    pub scoped_approval: Option<ApprovalScope>,
    /// Deterministic interaction state used by both the live UI and the pixel
    /// capture harness. Native hover styles remain active as well.
    pub visual_state: ApprovalVisualState,
    /// Logical focus within the blocking surface. The card owns the window
    /// focus; this value keeps Tab order deterministic without inventing a
    /// browser-only focus ring that was not captured by CDP.
    pub keyboard_focus: Option<ApprovalKeyboardFocus>,
    pub server_choices: Vec<ApprovalChoicePresentation>,
    pub failure_message: Option<String>,
    pub permission_details: Vec<String>,
    pub preview_expanded: bool,
    pub preview_line_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalChoicePresentation {
    pub decision: ApprovalDecision,
    pub label: String,
    pub description: Option<String>,
    pub is_rejection: bool,
    pub kind: ApprovalChoiceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalChoiceKind {
    Once,
    Session,
    ExecPolicy,
    NetworkAllow,
    NetworkDeny,
    Decline,
    Cancel,
}

impl ApprovalCardViewModel {
    pub fn pending(request_id: impl Into<String>, request: ApprovalRequestPresentation) -> Self {
        let scoped_approval = Some(request.default_scope());
        let preview_line_count = request
            .preview()
            .map(|text| text.lines().count().max(1))
            .unwrap_or(0);
        Self {
            request_id: request_id.into(),
            request,
            status: ApprovalCardStatus::Pending,
            allow_once: true,
            decline: true,
            cancel: false,
            scoped_approval,
            visual_state: ApprovalVisualState::Default,
            keyboard_focus: None,
            server_choices: Vec::new(),
            failure_message: None,
            permission_details: Vec::new(),
            preview_expanded: false,
            preview_line_count,
        }
    }

    pub fn should_render(&self) -> bool {
        matches!(
            self.status,
            ApprovalCardStatus::Pending | ApprovalCardStatus::Failed
        )
    }

    pub fn is_interactive(&self) -> bool {
        self.status == ApprovalCardStatus::Pending
    }

    pub fn set_available_decisions(
        &mut self,
        allow_once: bool,
        decline: bool,
        cancel: bool,
        scoped_approval: Option<ApprovalScope>,
    ) {
        self.allow_once = allow_once;
        self.decline = decline;
        self.cancel = cancel;
        self.scoped_approval = scoped_approval;
    }

    fn rejection_decision(&self) -> Option<ApprovalDecision> {
        if self.decline {
            Some(ApprovalDecision::Decline)
        } else if self.cancel {
            Some(ApprovalDecision::Cancel)
        } else {
            None
        }
    }

    fn can_reject(&self) -> bool {
        self.rejection_decision().is_some()
    }

    pub fn geometry(&self) -> ApprovalCardGeometry {
        let mut geometry = ApprovalCardGeometry::for_has_preview(self.request.preview().is_some());
        if self.request.preview().is_some() {
            geometry.card_height += self.preview_height() - APPROVAL_CARD_PREVIEW_HEIGHT;
        }
        if matches!(self.request, ApprovalRequestPresentation::Network { .. }) {
            geometry.card_height += 21.5;
        }
        geometry.menu_height =
            8.0 + self.menu_choices().len() as f32 * (APPROVAL_MENU_ROW_HEIGHT + 2.0) - 2.0;
        geometry.menu_top = geometry.card_height - 47.0 - geometry.menu_height;
        geometry
    }

    pub fn preview_height(&self) -> f32 {
        let lines = if self.preview_expanded {
            self.preview_line_count
        } else {
            self.preview_line_count.min(3)
        };
        (lines as f32 * 18.0
            + 16.0
            + if self.preview_line_count > 3 {
                32.0
            } else {
                0.0
            })
        .min(320.0)
    }

    fn choices(&self) -> Vec<ApprovalChoicePresentation> {
        if !self.server_choices.is_empty() {
            return self.server_choices.clone();
        }
        let mut choices = Vec::new();
        if self.allow_once {
            choices.push(ApprovalChoicePresentation {
                decision: ApprovalDecision::AllowOnce,
                label: "允许一次".into(),
                description: None,
                is_rejection: false,
                kind: ApprovalChoiceKind::Once,
            });
        }
        if let Some(scope) = self.scoped_approval {
            choices.push(ApprovalChoicePresentation {
                decision: ApprovalDecision::AllowScoped(scope),
                label: scope.label().into(),
                description: None,
                is_rejection: false,
                kind: ApprovalChoiceKind::ExecPolicy,
            });
        }
        if let Some(decision) = self.rejection_decision() {
            choices.push(ApprovalChoicePresentation {
                decision,
                label: "拒绝".into(),
                description: None,
                is_rejection: true,
                kind: if self.decline {
                    ApprovalChoiceKind::Decline
                } else {
                    ApprovalChoiceKind::Cancel
                },
            });
        }
        choices
    }

    fn primary_choice(&self) -> Option<(usize, ApprovalChoicePresentation)> {
        let choices = self.choices();
        choices
            .iter()
            .position(|choice| choice.kind == ApprovalChoiceKind::Once)
            .or_else(|| {
                choices.iter().position(|choice| {
                    !choice.is_rejection && choice.kind != ApprovalChoiceKind::NetworkAllow
                })
            })
            .or_else(|| {
                choices
                    .iter()
                    .position(|choice| matches!(choice.kind, ApprovalChoiceKind::NetworkDeny))
            })
            .map(|i| (i, choices[i].clone()))
    }

    fn reject_choice(&self) -> Option<(usize, ApprovalChoicePresentation)> {
        let choices = self.choices();
        choices
            .iter()
            .position(|choice| choice.kind == ApprovalChoiceKind::Decline)
            .or_else(|| {
                choices
                    .iter()
                    .position(|choice| choice.kind == ApprovalChoiceKind::Cancel)
            })
            .map(|i| (i, choices[i].clone()))
    }

    fn menu_choices(&self) -> Vec<(usize, ApprovalChoicePresentation)> {
        let reject = self.reject_choice().map(|(i, _)| i);
        self.choices()
            .into_iter()
            .enumerate()
            .filter(|(i, choice)| {
                Some(*i) != reject && choice.kind != ApprovalChoiceKind::NetworkAllow
            })
            .collect()
    }

    pub fn keyboard_event(&self, key: &str, shift: bool) -> Option<ApprovalCardEvent> {
        if self.status == ApprovalCardStatus::Failed && key == "escape" {
            return Some(ApprovalCardEvent::StopTurn);
        }
        if !self.is_interactive() {
            return None;
        }
        if !self.server_choices.is_empty() {
            return self.server_keyboard_event(key, shift);
        }
        match key {
            "tab" => {
                let next = if self.visual_state.menu_open() {
                    match (self.keyboard_focus, shift) {
                        (Some(ApprovalKeyboardFocus::MenuAllowOnce), false) => self
                            .scoped_approval
                            .map(ApprovalKeyboardFocus::MenuScoped)
                            .unwrap_or(ApprovalKeyboardFocus::MenuAllowOnce),
                        (Some(ApprovalKeyboardFocus::MenuScoped(_)), false) => {
                            ApprovalKeyboardFocus::MenuAllowOnce
                        }
                        (Some(ApprovalKeyboardFocus::MenuAllowOnce), true) => self
                            .scoped_approval
                            .map(ApprovalKeyboardFocus::MenuScoped)
                            .unwrap_or(ApprovalKeyboardFocus::MenuAllowOnce),
                        (Some(ApprovalKeyboardFocus::MenuScoped(_)), true) => {
                            ApprovalKeyboardFocus::MenuAllowOnce
                        }
                        (_, true) => self
                            .scoped_approval
                            .map(ApprovalKeyboardFocus::MenuScoped)
                            .unwrap_or(ApprovalKeyboardFocus::MenuAllowOnce),
                        _ => ApprovalKeyboardFocus::MenuAllowOnce,
                    }
                } else {
                    let mut focus_order = Vec::with_capacity(3);
                    if self.can_reject() {
                        focus_order.push(ApprovalKeyboardFocus::Decline);
                    }
                    if self.allow_once || self.scoped_approval.is_some() {
                        focus_order.push(ApprovalKeyboardFocus::AllowOnce);
                    }
                    if self.allow_once && self.scoped_approval.is_some() {
                        focus_order.push(ApprovalKeyboardFocus::MenuToggle);
                    }
                    if focus_order.is_empty() {
                        return None;
                    }
                    let current = self.keyboard_focus.and_then(|focus| {
                        focus_order.iter().position(|candidate| *candidate == focus)
                    });
                    let next = match (current, shift) {
                        (Some(index), false) => (index + 1) % focus_order.len(),
                        (Some(0), true) | (None, true) => focus_order.len() - 1,
                        (Some(index), true) => index - 1,
                        (None, false) => 0,
                    };
                    focus_order[next]
                };
                Some(ApprovalCardEvent::KeyboardFocusChanged(Some(next)))
            }
            "escape" if self.visual_state.menu_open() => Some(ApprovalCardEvent::ToggleMenu),
            "escape" if self.can_reject() => {
                self.rejection_decision().map(ApprovalCardEvent::Decision)
            }
            "enter" | "space" => match self.keyboard_focus {
                Some(ApprovalKeyboardFocus::Decline) if self.can_reject() => {
                    self.rejection_decision().map(ApprovalCardEvent::Decision)
                }
                Some(ApprovalKeyboardFocus::MenuToggle) => Some(ApprovalCardEvent::ToggleMenu),
                Some(ApprovalKeyboardFocus::MenuAllowOnce) => {
                    Some(ApprovalCardEvent::Decision(ApprovalDecision::AllowOnce))
                }
                Some(ApprovalKeyboardFocus::MenuScoped(scope)) => Some(
                    ApprovalCardEvent::Decision(ApprovalDecision::AllowScoped(scope)),
                ),
                Some(ApprovalKeyboardFocus::AllowOnce) | None
                    if !self.visual_state.menu_open() && self.allow_once =>
                {
                    Some(ApprovalCardEvent::Decision(ApprovalDecision::AllowOnce))
                }
                Some(ApprovalKeyboardFocus::AllowOnce) | None if !self.visual_state.menu_open() => {
                    self.scoped_approval.map(|scope| {
                        ApprovalCardEvent::Decision(ApprovalDecision::AllowScoped(scope))
                    })
                }
                _ => None,
            },
            "down" if self.visual_state.menu_open() => {
                let next = match self.keyboard_focus {
                    Some(ApprovalKeyboardFocus::MenuAllowOnce) => self
                        .scoped_approval
                        .map(ApprovalKeyboardFocus::MenuScoped)
                        .unwrap_or(ApprovalKeyboardFocus::MenuAllowOnce),
                    _ => ApprovalKeyboardFocus::MenuAllowOnce,
                };
                Some(ApprovalCardEvent::KeyboardFocusChanged(Some(next)))
            }
            "up" if self.visual_state.menu_open() => {
                let next = match self.keyboard_focus {
                    Some(ApprovalKeyboardFocus::MenuScoped(_)) => {
                        ApprovalKeyboardFocus::MenuAllowOnce
                    }
                    _ => self
                        .scoped_approval
                        .map(ApprovalKeyboardFocus::MenuScoped)
                        .unwrap_or(ApprovalKeyboardFocus::MenuAllowOnce),
                };
                Some(ApprovalCardEvent::KeyboardFocusChanged(Some(next)))
            }
            _ => None,
        }
    }

    fn server_keyboard_event(&self, key: &str, shift: bool) -> Option<ApprovalCardEvent> {
        let menu = self.visual_state.menu_open();
        let choices = self.choices();
        let menu_choices = self.menu_choices();
        let mut order = if menu {
            menu_choices
                .iter()
                .map(|(i, _)| ApprovalKeyboardFocus::MenuChoice(*i))
                .collect::<Vec<_>>()
        } else {
            let mut order = Vec::new();
            if self.preview_line_count > 3 {
                order.push(ApprovalKeyboardFocus::PreviewToggle);
            }
            order.extend(
                choices
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.kind == ApprovalChoiceKind::NetworkAllow)
                    .map(|(i, _)| ApprovalKeyboardFocus::Choice(i)),
            );
            if let Some((i, _)) = self.reject_choice() {
                order.push(ApprovalKeyboardFocus::Choice(i));
            }
            if let Some((i, _)) = self.primary_choice() {
                order.push(ApprovalKeyboardFocus::Choice(i));
            }
            if menu_choices.len() > 1 {
                order.push(ApprovalKeyboardFocus::MenuToggle);
            }
            order
        };
        match key {
            "escape" if menu => Some(ApprovalCardEvent::ToggleMenu),
            "escape" => self
                .reject_choice()
                .map(|(_, choice)| ApprovalCardEvent::Decision(choice.decision)),
            "tab" | "up" | "down" if !order.is_empty() && (key == "tab" || menu) => {
                let backwards = key == "up" || key == "tab" && shift;
                if backwards {
                    order.reverse();
                }
                let i = self
                    .keyboard_focus
                    .and_then(|focus| order.iter().position(|candidate| *candidate == focus))
                    .map(|i| (i + 1) % order.len())
                    .unwrap_or(0);
                Some(ApprovalCardEvent::KeyboardFocusChanged(Some(order[i])))
            }
            "enter" | "space" => match self.keyboard_focus {
                Some(ApprovalKeyboardFocus::MenuToggle) => Some(ApprovalCardEvent::ToggleMenu),
                Some(ApprovalKeyboardFocus::PreviewToggle) => {
                    Some(ApprovalCardEvent::TogglePreview)
                }
                Some(ApprovalKeyboardFocus::Choice(i) | ApprovalKeyboardFocus::MenuChoice(i)) => {
                    choices
                        .get(i)
                        .map(|choice| ApprovalCardEvent::Decision(choice.decision))
                }
                None if !menu && key == "enter" => self
                    .primary_choice()
                    .map(|(_, choice)| ApprovalCardEvent::Decision(choice.decision)),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ApprovalCardGeometry {
    pub card_height: f32,
    pub menu_top: f32,
    pub menu_width: f32,
    pub menu_height: f32,
}

impl ApprovalCardGeometry {
    pub const fn for_has_preview(has_preview: bool) -> Self {
        let preview_height = if has_preview {
            APPROVAL_CARD_PREVIEW_HEIGHT
        } else {
            0.0
        };
        let card_height =
            2.0 + APPROVAL_CARD_HEADER_HEIGHT + preview_height + APPROVAL_CARD_ACTIONS_HEIGHT;
        // The menu's bottom edge sits 1.875 px above the 28 px button. The
        // button itself starts 44 px above the card's bottom edge.
        let menu_top = card_height - 44.0 - APPROVAL_MENU_HEIGHT - 1.875;
        Self {
            card_height,
            menu_top,
            menu_width: APPROVAL_MENU_WIDTH,
            menu_height: APPROVAL_MENU_HEIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalCardEvent {
    Decision(ApprovalDecision),
    ToggleMenu,
    MenuFocusChanged(Option<ApprovalMenuItem>),
    KeyboardFocusChanged(Option<ApprovalKeyboardFocus>),
    TogglePreview,
    OpenNetworkDestination,
    StopTurn,
}

pub type ApprovalCardCallback = UiCallback<ApprovalCardEvent>;

#[derive(Clone, Copy)]
struct ApprovalPalette {
    card: gpui::Rgba,
    menu_outline: gpui::Rgba,
    preview: gpui::Rgba,
    text: gpui::Rgba,
    question_text: gpui::Rgba,
    secondary: gpui::Rgba,
    icon: gpui::Rgba,
    decline_text: gpui::Rgba,
    button_border: gpui::Rgba,
    decline: gpui::Rgba,
    decline_hover: gpui::Rgba,
    approve: gpui::Rgba,
    approve_hover: gpui::Rgba,
    approve_text: gpui::Rgba,
    menu: gpui::Rgba,
    menu_text: gpui::Rgba,
    menu_focus: gpui::Rgba,
}

impl ApprovalPalette {
    fn for_theme(theme: Theme) -> Self {
        if theme.surface == rgba(0x181818ff) {
            Self {
                // Chromium composites the captured translucent 45/96% token
                // to RGB 44 over the conversation surface. An opaque 44 keeps
                // GPUI's screenshot path on that same resolved value.
                card: rgba(0x2d2d2dff),
                menu_outline: rgba(0xffffff15),
                preview: rgba(0x282828ff),
                text: rgba(0xffffffff),
                question_text: rgba(0xffffffff),
                secondary: rgba(0xffffffa6),
                icon: rgba(0xffffffa6),
                decline_text: rgba(0xffffffff),
                button_border: rgba(0xffffff15),
                decline: rgba(0xffffff08),
                decline_hover: rgba(0xffffff14),
                approve: rgba(0xffffffff),
                approve_hover: rgba(0xffffffcc),
                approve_text: rgba(0x2d2d2dff),
                menu: rgba(0x2d2d2dff),
                menu_text: rgba(0xffffffff),
                menu_focus: rgba(0xffffff14),
            }
        } else {
            Self {
                card: rgba(0xffffffff),
                menu_outline: rgba(0x1a1c1f14),
                preview: rgba(0xffffffff),
                text: rgba(0x1a1c1fff),
                question_text: rgba(0x1a1c1fff),
                secondary: rgba(0x1a1c1fa6),
                icon: rgba(0x1a1c1fa6),
                decline_text: rgba(0x1a1c1fff),
                button_border: rgba(0x1a1c1f14),
                decline: rgba(0xffffffff),
                decline_hover: rgba(0x1a1c1f0e),
                approve: rgba(0x1a1c1fff),
                approve_hover: rgba(0x1a1c1fcc),
                approve_text: rgba(0xffffffb3),
                menu: rgba(0xffffffff),
                menu_text: rgba(0x1a1c1fc9),
                menu_focus: rgba(0x1a1c1f0e),
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    fn command() -> ApprovalCardViewModel {
        ApprovalCardViewModel::pending(
            "request-1",
            ApprovalRequestPresentation::command("curl -I https://example.com", None),
        )
    }

    #[test]
    fn captured_command_geometry_is_exact() {
        let geometry = command().geometry();
        assert_eq!(geometry.card_height, 164.0);
        assert_eq!(geometry.menu_top, 49.875);
        assert_eq!(geometry.menu_width, 168.0);
        assert_eq!(geometry.menu_height, 67.125);
    }

    #[test]
    fn network_without_command_omits_preview_height() {
        let model = ApprovalCardViewModel::pending(
            "request-2",
            ApprovalRequestPresentation::network("example.com", None, None),
        );
        assert_eq!(model.geometry().card_height, 151.5);
        assert_eq!(model.geometry().menu_top, 37.375);
        assert_eq!(model.request.preview(), None);
    }

    #[test]
    fn explicit_reason_wins_and_blank_reason_falls_back() {
        let command = ApprovalRequestPresentation::command(
            "pwd",
            Some("是否允许我仅运行命令 `pwd`？".to_owned()),
        );
        assert_eq!(command.question(), "是否允许我仅运行命令 `pwd`？");

        let network =
            ApprovalRequestPresentation::network("example.com", None, Some("  ".to_owned()));
        assert_eq!(network.question(), "允许 ChatGPT 与 example.com 建立连接？");
    }

    #[test]
    fn request_kind_selects_the_real_scoped_copy() {
        assert_eq!(
            command().scoped_approval,
            Some(ApprovalScope::SimilarCommands)
        );
        let network = ApprovalCardViewModel::pending(
            "request-2",
            ApprovalRequestPresentation::network("example.com", Some("curl".to_owned()), None),
        );
        assert_eq!(network.scoped_approval, Some(ApprovalScope::InternetAccess));
        assert_eq!(ApprovalScope::InternetAccess.label(), "互联网访问");
    }

    #[test]
    fn resolved_surface_is_not_renderable() {
        let mut model = command();
        assert!(model.should_render());
        model.status = ApprovalCardStatus::Resolved;
        assert!(!model.should_render());
    }

    #[test]
    fn split_menu_tracks_default_and_focus_separately() {
        let default = ApprovalVisualState::SplitMenu { focused: None };
        assert!(default.menu_open());
        assert_eq!(default.focused_menu_item(), None);

        let focused = ApprovalVisualState::SplitMenu {
            focused: Some(ApprovalMenuItem::AllowOnce),
        };
        assert!(focused.menu_open());
        assert_eq!(
            focused.focused_menu_item(),
            Some(ApprovalMenuItem::AllowOnce)
        );
    }

    #[test]
    fn keyboard_shortcuts_and_tab_order_cover_every_approval_action() {
        let mut model = command();
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::AllowOnce))
        );
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::Decline))
        );

        assert_eq!(
            model.keyboard_event("tab", false),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::Decline
            )))
        );
        model.keyboard_focus = Some(ApprovalKeyboardFocus::Decline);
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::AllowOnce
            )))
        );
        model.keyboard_focus = Some(ApprovalKeyboardFocus::AllowOnce);
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::MenuToggle
            )))
        );
        model.keyboard_focus = Some(ApprovalKeyboardFocus::MenuToggle);
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(ApprovalCardEvent::ToggleMenu)
        );
        assert_eq!(
            model.keyboard_event("tab", true),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::AllowOnce
            )))
        );
    }

    #[test]
    fn cancel_advertisement_preserves_its_distinct_decision() {
        let mut model = command();
        model.set_available_decisions(true, false, true, None);

        assert!(model.can_reject());
        assert_eq!(model.rejection_decision(), Some(ApprovalDecision::Cancel));
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::Cancel))
        );
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::Decline
            )))
        );
        model.keyboard_focus = Some(ApprovalKeyboardFocus::Decline);
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::Cancel))
        );
    }

    #[test]
    fn open_menu_traps_tab_and_escape_without_answering() {
        let mut model = command();
        model.visual_state = ApprovalVisualState::SplitMenu { focused: None };
        assert_eq!(
            model.keyboard_event("tab", false),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::MenuAllowOnce
            )))
        );
        assert_eq!(
            model.keyboard_event("tab", true),
            Some(ApprovalCardEvent::KeyboardFocusChanged(Some(
                ApprovalKeyboardFocus::MenuScoped(ApprovalScope::SimilarCommands)
            )))
        );
        assert_eq!(
            model.keyboard_event("escape", false),
            Some(ApprovalCardEvent::ToggleMenu)
        );

        model.keyboard_focus = Some(ApprovalKeyboardFocus::MenuScoped(
            ApprovalScope::SimilarCommands,
        ));
        assert_eq!(
            model.keyboard_event("enter", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::AllowScoped(
                ApprovalScope::SimilarCommands
            )))
        );
    }

    #[test]
    fn light_and_dark_palettes_use_captured_surface_values() {
        let dark = ApprovalPalette::for_theme(Theme::for_mode(ThemeMode::Dark));
        let light = ApprovalPalette::for_theme(Theme::for_mode(ThemeMode::Light));
        assert_eq!(dark.card, rgba(0x2d2d2dff));
        assert_eq!(dark.preview, rgba(0x282828ff));
        assert_eq!(light.card, rgba(0xffffffff));
        assert_eq!(light.approve, rgba(0x1a1c1fff));
        assert_eq!(dark.decline_hover, rgba(0xffffff14));
        assert_eq!(light.decline_hover, rgba(0x1a1c1f0e));
    }
}
