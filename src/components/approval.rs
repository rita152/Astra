//! Native command and network approval surface.
//!
//! Geometry and colors are taken from the ChatGPT desktop CDP captures in
//! `artifacts/chatgpt-p0-ui-cdp-audit-2026-08-30/{06..10,17..21}-*.json`.
//! This module intentionally contains no app-server protocol types. The
//! caller supplies a presentation model and translates [`ApprovalCardEvent`]
//! back into its own state machine.

use std::rc::Rc;

use gpui::{
    App, BoxShadow, Div, FontWeight, Role, SharedString, Stateful, Window, div, prelude::*, px,
    rgba,
};

use crate::{
    components::icons::icon,
    theme::{Theme, ThemeMode, UI_MONOSPACE_FONT_FAMILY},
};

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
    Resolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalRequestPresentation {
    Command {
        command: String,
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
            Self::Network { .. } => "互联网访问",
        }
    }

    pub fn question(&self) -> String {
        let reason = match self {
            Self::Command { reason, .. } | Self::Network { reason, .. } => reason,
        };
        if let Some(reason) = reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        {
            return reason.to_owned();
        }

        match self {
            Self::Command { .. } => "是否允许 ChatGPT 运行此命令？".to_owned(),
            Self::Network { destination, .. } => {
                format!("是否允许 ChatGPT 连接到 {destination}？")
            }
        }
    }

    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Command { command, .. } => non_empty(command),
            Self::Network { command, .. } => command.as_deref().and_then(non_empty),
        }
    }

    pub fn default_scope(&self) -> ApprovalScope {
        match self {
            Self::Command { .. } => ApprovalScope::SimilarCommands,
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
}

impl ApprovalScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::SimilarCommands => "允许类似命令",
            Self::InternetAccess => "互联网访问",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    AllowScoped(ApprovalScope),
    Decline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalMenuItem {
    AllowOnce,
    Scoped(ApprovalScope),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalKeyboardFocus {
    Decline,
    AllowOnce,
    MenuToggle,
    MenuAllowOnce,
    MenuScoped(ApprovalScope),
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
    /// Whether the server listed `cancel` in `availableDecisions`. ChatGPT uses
    /// this to show Reject, while sending `decline` to keep the turn running.
    pub cancel: bool,
    /// `None` renders a single Allow button. A value renders ChatGPT's split
    /// button and its two-row menu.
    pub scoped_approval: Option<ApprovalScope>,
    /// Deterministic interaction state used by both the live UI and the pixel
    /// capture harness. Native hover styles remain active as well.
    pub visual_state: ApprovalVisualState,
    /// Logical focus within the blocking surface. The card owns the window
    /// focus; this value keeps Tab order deterministic without inventing a
    /// browser-only focus ring that was not captured by CDP.
    pub keyboard_focus: Option<ApprovalKeyboardFocus>,
}

impl ApprovalCardViewModel {
    pub fn pending(request_id: impl Into<String>, request: ApprovalRequestPresentation) -> Self {
        let scoped_approval = Some(request.default_scope());
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
        }
    }

    pub fn should_render(&self) -> bool {
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
        if self.decline || self.cancel {
            Some(ApprovalDecision::Decline)
        } else {
            None
        }
    }

    fn can_reject(&self) -> bool {
        self.rejection_decision().is_some()
    }

    pub fn geometry(&self) -> ApprovalCardGeometry {
        ApprovalCardGeometry::for_has_preview(self.request.preview().is_some())
    }

    pub fn keyboard_event(&self, key: &str, shift: bool) -> Option<ApprovalCardEvent> {
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
            APPROVAL_CARD_HEADER_HEIGHT + preview_height + APPROVAL_CARD_ACTIONS_HEIGHT;
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
}

/// Callback kept UI-framework aware so a parent entity can update itself and
/// notify GPUI without introducing protocol types into this module.
#[derive(Clone)]
pub struct ApprovalCardCallback(Rc<dyn Fn(ApprovalCardEvent, &mut Window, &mut App) + 'static>);

impl ApprovalCardCallback {
    pub fn new(callback: impl Fn(ApprovalCardEvent, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(callback))
    }

    fn emit(&self, event: ApprovalCardEvent, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}

#[derive(Clone, Copy)]
struct ApprovalPalette {
    mode: ThemeMode,
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
                mode: ThemeMode::Dark,
                // Chromium composites the captured translucent 45/96% token
                // to RGB 44 over the conversation surface. An opaque 44 keeps
                // GPUI's screenshot path on that same resolved value.
                card: rgba(0x2c2c2cff),
                menu_outline: rgba(0xffffff15),
                preview: rgba(0x282828ff),
                text: rgba(0xdfdfdfff),
                question_text: rgba(0xdfdfdfe3),
                secondary: rgba(0xdfdfdf80),
                icon: rgba(0xdfdfdfa6),
                decline_text: rgba(0xdfdfdfca),
                button_border: rgba(0xffffff15),
                decline: rgba(0xffffff08),
                decline_hover: rgba(0xffffff14),
                approve: rgba(0xdfdfdfff),
                approve_hover: rgba(0xdfdfdfcc),
                approve_text: rgba(0x2d2d2ddb),
                menu: rgba(0x2d2d2dff),
                menu_text: rgba(0xdfdfdfb3),
                menu_focus: rgba(0xffffff14),
            }
        } else {
            Self {
                mode: ThemeMode::Light,
                card: rgba(0xffffffff),
                menu_outline: rgba(0x1a1c1f14),
                preview: rgba(0xffffffff),
                text: rgba(0x1a1c1fff),
                question_text: rgba(0x1a1c1fff),
                secondary: rgba(0x1a1c1f74),
                icon: rgba(0x1a1c1fa6),
                decline_text: rgba(0x1a1c1fdb),
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

    fn card_shadows(self) -> Vec<BoxShadow> {
        let (short_shadow, ambient_shadow) = match self.mode {
            ThemeMode::Light => (rgba(0x0000000d), rgba(0x00000007)),
            ThemeMode::Dark => (rgba(0x0000000a), rgba(0x0000000d)),
        };
        let mut shadows = vec![
            BoxShadow::new(px(0.0), px(3.0), short_shadow.into()).blur_radius(px(7.5)),
            BoxShadow::new(px(0.0), px(0.0), ambient_shadow.into()).blur_radius(px(20.0)),
        ];
        let outline = match self.mode {
            ThemeMode::Light => rgba(0x1a1c1f10),
            // GPUI rasterizes a 0.5 px spread at full device-pixel coverage;
            // halving the captured alpha preserves Chromium's 41-valued edge.
            ThemeMode::Dark => rgba(0xffffff14),
        };
        shadows.insert(
            0,
            BoxShadow::new(px(0.0), px(0.0), outline.into()).spread_radius(px(0.5)),
        );
        shadows
    }
}

/// Renders a native GPUI approval card. `Resolved` deliberately returns
/// `None`, mirroring ChatGPT's `serverRequest/resolved` behavior.
pub fn render_approval_card(
    model: &ApprovalCardViewModel,
    theme: Theme,
    callback: ApprovalCardCallback,
) -> Option<Stateful<Div>> {
    if !model.should_render() {
        return None;
    }

    let palette = ApprovalPalette::for_theme(theme);
    let geometry = model.geometry();
    let request_id: SharedString = model.request_id.clone().into();
    let title = model.request.title();
    let question = model.request.question();
    let preview = model.request.preview().map(ToOwned::to_owned);
    let icon_name = match model.request {
        ApprovalRequestPresentation::Command { .. } => "panel-terminal",
        ApprovalRequestPresentation::Network { .. } => "permission-dialog-internet",
    };

    let header = div()
        .id(approval_element_id("approval-header", &model.request_id))
        .role(Role::Alert)
        .aria_label(format!("{title}，{question}"))
        .h(px(APPROVAL_CARD_HEADER_HEIGHT))
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
                .text_size(px(13.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.secondary)
                .child(
                    div().size(px(18.0)).flex_none().child(
                        icon(icon_name, palette.icon.into())
                            .relative()
                            .top(px(1.0))
                            .size(px(18.0)),
                    ),
                )
                .child(title),
        )
        .child(
            div()
                .h(px(20.0))
                .min_w(px(0.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                // Chromium resolves this mixed Chinese/Latin request line to
                // PingFang in the captured card; keep that same face here.
                .font_weight(FontWeight::NORMAL)
                .font_family("PingFang SC")
                .text_color(palette.question_text)
                .overflow_hidden()
                .child(question),
        );

    let decline_callback = callback.clone();
    let rejection_decision = model
        .rejection_decision()
        .unwrap_or(ApprovalDecision::Decline);
    let decline = div()
        .id(approval_element_id("approval-decline", &model.request_id))
        .role(Role::Button)
        .aria_label("拒绝审批")
        .h(px(APPROVAL_BUTTON_HEIGHT))
        .pl(px(8.0))
        .pr(px(6.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(9999.0))
        .border_1()
        .border_color(palette.button_border)
        .bg(
            if model.visual_state == ApprovalVisualState::DeclineHovered {
                palette.decline_hover
            } else {
                palette.decline
            },
        )
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(palette.decline_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.decline_hover))
        .on_click(move |_, window, cx| {
            decline_callback.emit(ApprovalCardEvent::Decision(rejection_decision), window, cx);
        })
        .child("拒绝")
        .child(keycap("Esc", palette.decline_text));

    let approve_callback = callback.clone();
    let split_approval = model.allow_once && model.scoped_approval.is_some();
    let primary_scope = (!model.allow_once)
        .then_some(model.scoped_approval)
        .flatten();
    let primary_label = primary_scope
        .map(ApprovalScope::label)
        .unwrap_or("允许一次");
    let approve_fill = if matches!(
        model.visual_state,
        ApprovalVisualState::ApproveHovered | ApprovalVisualState::SplitMenu { .. }
    ) {
        palette.approve_hover
    } else {
        palette.approve
    };
    let mut approve = div()
        .id(approval_element_id("approval-once", &model.request_id))
        .role(Role::Button)
        .aria_label(primary_label)
        .h(px(APPROVAL_BUTTON_HEIGHT))
        .pl(px(8.0))
        .pr(if split_approval { px(1.0) } else { px(8.0) })
        .flex()
        .items_center()
        .gap(px(4.0))
        .border_t_1()
        .border_b_1()
        .border_l_1()
        .border_color(palette.button_border)
        .bg(approve_fill)
        .text_size(px(13.0))
        .line_height(px(18.0))
        .text_color(palette.approve_text)
        .cursor_pointer()
        .hover(move |button| button.bg(palette.approve_hover))
        .on_click(move |_, window, cx| {
            let decision = primary_scope
                .map(ApprovalDecision::AllowScoped)
                .unwrap_or(ApprovalDecision::AllowOnce);
            approve_callback.emit(ApprovalCardEvent::Decision(decision), window, cx);
        })
        .child(primary_label)
        .child(keycap("⏎", palette.approve_text));
    approve = if split_approval {
        approve.rounded_l(px(9999.0))
    } else {
        approve
            .rounded(px(9999.0))
            .border_r_1()
            .border_color(palette.button_border)
    };

    let mut approve_group = div()
        .min_w(px(0.0))
        .flex()
        .items_stretch()
        .rounded(px(9999.0))
        .overflow_hidden()
        .child(approve);

    if split_approval {
        let menu_callback = callback.clone();
        approve_group = approve_group.child(
            div()
                .id(approval_element_id(
                    "approval-menu-toggle",
                    &model.request_id,
                ))
                .role(Role::Button)
                .aria_label("审批选项")
                .h(px(APPROVAL_BUTTON_HEIGHT))
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
                .text_color(palette.approve_text)
                .cursor_pointer()
                .hover(move |button| button.bg(palette.approve_hover))
                .on_click(move |_, window, cx| {
                    menu_callback.emit(ApprovalCardEvent::ToggleMenu, window, cx);
                })
                .child(
                    icon("chevron-down", palette.approve_text.alpha(0.50).into()).size(px(14.0)),
                ),
        );
    }

    let actions = div()
        .h(px(APPROVAL_CARD_ACTIONS_HEIGHT))
        .px(px(16.0))
        .pt(px(8.0))
        .pb(px(16.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(div().flex_1())
        .when(model.can_reject(), |actions| actions.child(decline))
        .when(
            model.allow_once || model.scoped_approval.is_some(),
            |actions| actions.child(approve_group),
        );

    let card = div()
        .h(px(geometry.card_height))
        .w_full()
        .overflow_hidden()
        .rounded(px(APPROVAL_CARD_RADIUS))
        .bg(palette.card)
        .shadow(palette.card_shadows())
        .child(header)
        .when_some(preview, |card, preview| {
            card.child(
                div()
                    .h(px(APPROVAL_CARD_PREVIEW_HEIGHT))
                    .px(px(12.0))
                    .child(
                        div()
                            .h(px(APPROVAL_CARD_PREVIEW_HEIGHT))
                            .max_h(px(320.0))
                            .overflow_hidden()
                            .rounded(px(8.0))
                            .bg(palette.preview)
                            .px(px(8.0))
                            .py(px(8.0))
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .font_family(UI_MONOSPACE_FONT_FAMILY)
                            .font_weight(FontWeight::NORMAL)
                            .text_color(palette.secondary)
                            .child(preview),
                    ),
            )
        })
        .child(actions);

    let mut result = div()
        .id(approval_element_id("approval-card", &model.request_id))
        .relative()
        // The real conversation column starts at a 0.671875 px fractional
        // coordinate in the 2560 px CDP fixture. Preserve that subpixel phase
        // so CoreText and the vector icons rasterize on the same device pixels.
        .left(px(0.671875))
        .h(px(geometry.card_height))
        .w_full()
        .child(card);

    if let (ApprovalVisualState::SplitMenu { focused }, Some(scope)) =
        (model.visual_state, model.scoped_approval)
        && model.allow_once
    {
        result = result.child(approval_menu(
            request_id, scope, focused, geometry, palette, callback,
        ));
    }

    Some(result)
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

fn approval_menu(
    request_id: SharedString,
    scope: ApprovalScope,
    focused: Option<ApprovalMenuItem>,
    geometry: ApprovalCardGeometry,
    palette: ApprovalPalette,
    callback: ApprovalCardCallback,
) -> Stateful<Div> {
    let menu_outline = palette.menu_outline;
    div()
        .id(approval_element_id("approval-menu", request_id.as_ref()))
        .role(Role::Menu)
        .aria_label("审批选项")
        .absolute()
        .top(px(geometry.menu_top))
        .right(px(16.0))
        .w(px(geometry.menu_width))
        .h(px(geometry.menu_height))
        .p(px(4.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .overflow_hidden()
        .rounded(px(15.0))
        .bg(palette.menu)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), menu_outline.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .child(approval_menu_row(
            approval_element_id("approval-menu-once", request_id.as_ref()),
            "允许一次",
            ApprovalMenuItem::AllowOnce,
            focused == Some(ApprovalMenuItem::AllowOnce),
            false,
            palette,
            callback.clone(),
        ))
        .child(approval_menu_row(
            approval_element_id("approval-menu-scoped", request_id.as_ref()),
            scope.label(),
            ApprovalMenuItem::Scoped(scope),
            focused == Some(ApprovalMenuItem::Scoped(scope)),
            true,
            palette,
            callback,
        ))
}

fn approval_menu_row(
    id: SharedString,
    label: &'static str,
    item: ApprovalMenuItem,
    focused: bool,
    shows_information: bool,
    palette: ApprovalPalette,
    callback: ApprovalCardCallback,
) -> Stateful<Div> {
    let hover_callback = callback.clone();
    let click_callback = callback;
    div()
        .id(id)
        .role(Role::MenuItem)
        .h(px(APPROVAL_MENU_ROW_HEIGHT))
        .w_full()
        .px(px(8.0))
        .py(px(5.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(12.5))
        .when(focused, |row| row.bg(palette.menu_focus))
        .text_size(px(13.0))
        .line_height(px(18.5714))
        .font_weight(FontWeight::NORMAL)
        .text_color(palette.menu_text)
        .cursor_pointer()
        .hover(move |row| row.bg(palette.menu_focus))
        .on_hover(move |hovered, window, cx| {
            hover_callback.emit(
                ApprovalCardEvent::MenuFocusChanged((*hovered).then_some(item)),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            let decision = match item {
                ApprovalMenuItem::AllowOnce => ApprovalDecision::AllowOnce,
                ApprovalMenuItem::Scoped(scope) => ApprovalDecision::AllowScoped(scope),
            };
            click_callback.emit(ApprovalCardEvent::Decision(decision), window, cx);
        })
        .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
        .when(shows_information, |row| {
            row.child(icon("file-approval-info", palette.text.alpha(0.65).into()).size(px(16.0)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command() -> ApprovalCardViewModel {
        ApprovalCardViewModel::pending(
            "request-1",
            ApprovalRequestPresentation::command("curl -I https://example.com", None),
        )
    }

    #[test]
    fn captured_command_geometry_is_exact() {
        let geometry = command().geometry();
        assert_eq!(geometry.card_height, 162.0);
        assert_eq!(geometry.menu_top, 49.0);
        assert_eq!(geometry.menu_width, 168.0);
        assert_eq!(geometry.menu_height, 67.125);
    }

    #[test]
    fn network_without_command_omits_preview_height() {
        let model = ApprovalCardViewModel::pending(
            "request-2",
            ApprovalRequestPresentation::network("example.com", None, None),
        );
        assert_eq!(model.geometry().card_height, 128.0);
        assert_eq!(model.geometry().menu_top, 15.0);
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
        assert_eq!(network.question(), "是否允许 ChatGPT 连接到 example.com？");
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
    fn cancel_advertisement_still_drives_chatgpt_style_decline() {
        let mut model = command();
        model.set_available_decisions(true, false, true, None);

        assert!(model.can_reject());
        assert_eq!(model.rejection_decision(), Some(ApprovalDecision::Decline));
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
            model.keyboard_event("enter", false),
            Some(ApprovalCardEvent::Decision(ApprovalDecision::Decline))
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
        assert_eq!(dark.card, rgba(0x2c2c2cff));
        assert_eq!(dark.preview, rgba(0x282828ff));
        assert_eq!(light.card, rgba(0xffffffff));
        assert_eq!(light.approve, rgba(0x1a1c1fff));
        assert_eq!(dark.decline_hover, rgba(0xffffff14));
        assert_eq!(light.decline_hover, rgba(0x1a1c1f0e));
    }
}
