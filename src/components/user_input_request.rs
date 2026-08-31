//! Native `item/tool/requestUserInput` form surface.
//!
//! This module is deliberately presentation-only: it does not depend on the
//! app-server protocol or mutate conversation state. Geometry, copy, colors,
//! and interaction behavior come from the ChatGPT desktop CDP captures in
//! `artifacts/chatgpt-p0-ui-cdp-audit-2026-08-30/{47..54,56..58}-*.{json,png}`.
//! Two-question navigation and per-question answer persistence come from the
//! natural request and controlled renderer captures in
//! `artifacts/chatgpt-user-input-multi-cdp-audit-2026-08-30/`.
//! The production integration keeps a submitted request mounted and disabled
//! until `serverRequest/resolved` arrives. Cancellation and transport failures
//! are also represented explicitly so pending protocol state never disappears
//! without an observable outcome.

use std::{fmt, rc::Rc};

use gpui::{
    App, BoxShadow, Div, Entity, FontWeight, Role, SharedString, Stateful, Transformation, Window,
    div, prelude::*, px, radians, rgba,
};

use crate::{
    components::{icons::icon, prompt_input::PromptInput},
    theme::{Theme, ThemeMode},
};

pub const USER_INPUT_CARD_RADIUS: f32 = 25.0;
pub const USER_INPUT_HEADER_HEIGHT: f32 = 44.0;
pub const USER_INPUT_CONTENT_TOP_PADDING: f32 = 10.0;
pub const USER_INPUT_RECOMMENDED_OPTION_HEIGHT: f32 = 52.5625;
pub const USER_INPUT_OPTION_HEIGHT: f32 = 51.125;
pub const USER_INPUT_OPTION_GAP: f32 = 4.0;
pub const USER_INPUT_OTHER_ROW_HEIGHT: f32 = 40.0;
pub const USER_INPUT_CONTENT_BOTTOM_PADDING: f32 = 8.0;
pub const USER_INPUT_CONTROL_SIZE: f32 = 24.0;
pub const USER_INPUT_SKIP_HEIGHT: f32 = 28.0;
#[cfg(test)]
pub const USER_INPUT_CAPTURED_TWO_OPTION_HEIGHT: f32 = 213.6875;

fn element_id(prefix: &str, request_id: &str, suffix: impl std::fmt::Display) -> SharedString {
    format!("{prefix}-{request_id}-{suffix}").into()
}

/// Lifecycle states for the protocol-backed request card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UserInputRequestStatus {
    #[default]
    Pending,
    Submitting,
    Resolved,
    Cancelled,
    Failed,
}

/// One selectable answer in a request question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserInputOptionPresentation {
    pub label: String,
    pub description: Option<String>,
    pub recommended: bool,
}

impl UserInputOptionPresentation {
    pub fn new(label: impl Into<String>, description: Option<String>) -> Self {
        Self {
            label: label.into(),
            description,
            recommended: false,
        }
    }

    pub fn recommended(label: impl Into<String>, description: Option<String>) -> Self {
        Self {
            label: label.into(),
            description,
            recommended: true,
        }
    }

    pub fn accessible_label(&self) -> String {
        if self.recommended {
            format!("{} (Recommended)", self.label)
        } else {
            self.label.clone()
        }
    }
}

/// A single question supplied by `item/tool/requestUserInput`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserInputQuestionPresentation {
    pub id: String,
    pub header: Option<String>,
    pub question: String,
    pub options: Vec<UserInputOptionPresentation>,
    pub allows_other: bool,
    pub other_placeholder: String,
    pub is_secret: bool,
}

impl UserInputQuestionPresentation {
    pub fn single_choice(
        id: impl Into<String>,
        question: impl Into<String>,
        options: Vec<UserInputOptionPresentation>,
    ) -> Self {
        Self {
            id: id.into(),
            header: None,
            question: question.into(),
            options,
            allows_other: true,
            other_placeholder: "否，并告诉 ChatGPT 应该如何做得不同".to_owned(),
            is_secret: false,
        }
    }

    pub fn display_question(&self) -> &str {
        non_empty(self.question.as_str())
            .or_else(|| self.header.as_deref().and_then(non_empty))
            .unwrap_or("请选择一个选项。")
    }

    pub fn recommended_option_index(&self) -> Option<usize> {
        self.options.iter().position(|option| option.recommended)
    }
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

/// Visual navigation state is independent from the submitted/checked answer.
/// Capture 52 demonstrates this explicitly: blue keeps its activity fill while
/// red receives the keyboard focus ring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserInputVisualState {
    pub active_option_index: Option<usize>,
    pub focused_option_index: Option<usize>,
    pub skip_hovered: bool,
    /// Some real transitions retain the checked row's fill while another row
    /// is active (single-question dark captures); D02 does not.
    pub retain_checked_fill: bool,
}

impl UserInputVisualState {
    pub const fn option_active(index: usize) -> Self {
        Self {
            active_option_index: Some(index),
            focused_option_index: None,
            skip_hovered: false,
            retain_checked_fill: true,
        }
    }

    pub const fn option_active_exclusive(index: usize) -> Self {
        Self {
            active_option_index: Some(index),
            focused_option_index: None,
            skip_hovered: false,
            retain_checked_fill: false,
        }
    }

    pub const fn option_focused(active_index: usize, focused_index: usize) -> Self {
        Self {
            active_option_index: Some(active_index),
            focused_option_index: Some(focused_index),
            skip_hovered: false,
            retain_checked_fill: true,
        }
    }

    pub const fn skip_hovered() -> Self {
        Self {
            active_option_index: None,
            focused_option_index: None,
            skip_hovered: true,
            retain_checked_fill: false,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct UserInputRequestPresentation {
    pub request_id: String,
    pub questions: Vec<UserInputQuestionPresentation>,
    pub current_question_index: usize,
    pub selected_option_index: Option<usize>,
    pub status: UserInputRequestStatus,
    pub is_blocking: bool,
    pub auto_resolution_ms: Option<u64>,
    pub failure_message: Option<String>,
    pub visual_state: UserInputVisualState,
    /// Logical focus inside the blocking form. The enclosing GPUI surface
    /// owns native window focus; this preserves the CDP-observed Tab order.
    pub keyboard_focus: Option<UserInputKeyboardFocus>,
    /// Draft entered through the CDP-observed free-form `Other` path.
    pub other_answer: String,
    /// Per-question drafts survive forward/back navigation and are serialized
    /// into the app-server response only after the final step.
    pub answers: Vec<UserInputQuestionAnswer>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct UserInputQuestionAnswer {
    pub selected_option_index: Option<usize>,
    pub selected_label: Option<String>,
    pub selected_labels: Vec<String>,
    pub other_answer: String,
    pub skipped: bool,
}

impl fmt::Debug for UserInputQuestionAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserInputQuestionAnswer")
            .field("selected_option_index", &self.selected_option_index)
            .field(
                "answer_count",
                &self
                    .selected_labels
                    .len()
                    .max(usize::from(self.selected_label.is_some())),
            )
            .field("answers", &"<redacted>")
            .field("skipped", &self.skipped)
            .finish()
    }
}

impl fmt::Debug for UserInputRequestPresentation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserInputRequestPresentation")
            .field("request_id", &self.request_id)
            .field("questions", &self.questions)
            .field("current_question_index", &self.current_question_index)
            .field("selected_option_index", &self.selected_option_index)
            .field("status", &self.status)
            .field("is_blocking", &self.is_blocking)
            .field("auto_resolution_ms", &self.auto_resolution_ms)
            .field("visual_state", &self.visual_state)
            .field("keyboard_focus", &self.keyboard_focus)
            .field("answers", &self.answers)
            .field("other_answer", &"<redacted>")
            .field("failure_message", &self.failure_message)
            .finish()
    }
}

impl UserInputRequestPresentation {
    pub fn pending(
        request_id: impl Into<String>,
        questions: Vec<UserInputQuestionPresentation>,
    ) -> Self {
        let question_count = questions.len();
        let selected_option_index = questions
            .first()
            .and_then(UserInputQuestionPresentation::recommended_option_index)
            .or_else(|| {
                questions
                    .first()
                    .filter(|question| !question.options.is_empty())
                    .map(|_| 0)
            });
        Self {
            request_id: request_id.into(),
            questions,
            current_question_index: 0,
            selected_option_index,
            status: UserInputRequestStatus::Pending,
            is_blocking: true,
            auto_resolution_ms: None,
            failure_message: None,
            visual_state: UserInputVisualState {
                active_option_index: selected_option_index,
                ..UserInputVisualState::default()
            },
            keyboard_focus: None,
            other_answer: String::new(),
            answers: vec![UserInputQuestionAnswer::default(); question_count],
        }
    }

    pub fn current_question(&self) -> Option<&UserInputQuestionPresentation> {
        self.questions.get(self.current_question_index)
    }

    pub fn should_render(&self) -> bool {
        self.status != UserInputRequestStatus::Resolved
            && (self.current_question().is_some() || self.status != UserInputRequestStatus::Pending)
    }

    pub fn is_interactive(&self) -> bool {
        self.status == UserInputRequestStatus::Pending && self.current_question().is_some()
    }

    #[cfg(test)]
    pub fn geometry(&self) -> Option<UserInputCardGeometry> {
        self.current_question()
            .map(UserInputCardGeometry::for_question)
    }

    pub fn focus_other_answer(&mut self) {
        self.keyboard_focus = Some(UserInputKeyboardFocus::Other);
        self.visual_state.focused_option_index = None;
        self.visual_state.skip_hovered = false;
    }

    pub fn is_multi_question(&self) -> bool {
        self.questions.len() > 1
    }

    pub fn is_last_question(&self) -> bool {
        self.current_question_index + 1 >= self.questions.len()
    }

    pub fn progress_label(&self) -> String {
        format!(
            "{} of {}",
            self.current_question_index.saturating_add(1),
            self.questions.len()
        )
    }

    pub fn save_selected_option(&mut self, option_index: usize, label: String) {
        self.selected_option_index = Some(option_index);
        self.other_answer.clear();
        let question_index = self.current_question_index;
        self.save_answer_values(question_index, vec![label]);
        if let Some(answer) = self.answers.get_mut(self.current_question_index) {
            answer.selected_option_index = Some(option_index);
        }
        self.visual_state.active_option_index = Some(option_index);
    }

    pub fn save_other_answer(&mut self, answer: String) {
        self.selected_option_index = None;
        self.other_answer = answer.clone();
        if let Some(saved) = self.answers.get_mut(self.current_question_index) {
            saved.selected_option_index = None;
            saved.selected_label = None;
            saved.selected_labels.clear();
            saved.other_answer = answer;
            saved.skipped = false;
        }
        self.visual_state.active_option_index = None;
    }

    pub fn skip_current_question(&mut self) {
        self.selected_option_index = None;
        self.other_answer.clear();
        if let Some(saved) = self.answers.get_mut(self.current_question_index) {
            *saved = UserInputQuestionAnswer {
                skipped: true,
                ..UserInputQuestionAnswer::default()
            };
        }
        self.visual_state.active_option_index = None;
    }

    /// Persists the current question's draft before moving between questions.
    /// Keyboard radio navigation updates the checked option immediately but
    /// does not submit it, so navigation must copy that transient state into
    /// `answers`. A previously skipped question remains skipped until the user
    /// explicitly chooses an option or enters an Other answer.
    pub fn persist_current_answer(&mut self) {
        if !self.other_answer.trim().is_empty() {
            self.save_other_answer(self.other_answer.clone());
            return;
        }

        let selected = self.selected_option_index.and_then(|index| {
            self.current_question()
                .and_then(|question| question.options.get(index))
                .map(|option| (index, option.label.clone()))
        });
        if let Some((index, label)) = selected {
            self.save_selected_option(index, label);
            return;
        }

        if self
            .answers
            .get(self.current_question_index)
            .is_some_and(|answer| answer.skipped)
        {
            self.skip_current_question();
        }
    }

    pub fn next_question(&mut self) -> bool {
        if self.is_last_question() {
            return false;
        }
        self.current_question_index += 1;
        self.load_current_answer();
        true
    }

    pub fn previous_question(&mut self) -> bool {
        if self.current_question_index == 0 {
            return false;
        }
        self.current_question_index -= 1;
        self.load_current_answer();
        true
    }

    pub fn response_answers(&self) -> Vec<(String, Vec<String>)> {
        self.questions
            .iter()
            .zip(&self.answers)
            .filter_map(|(question, answer)| {
                if answer.skipped {
                    return None;
                }
                let mut values = if answer.selected_labels.is_empty() {
                    answer.selected_label.clone().into_iter().collect()
                } else {
                    answer.selected_labels.clone()
                };
                if !answer.other_answer.trim().is_empty() {
                    values.push(answer.other_answer.trim().to_owned());
                }
                (!values.is_empty()).then(|| (question.id.clone(), values))
            })
            .collect()
    }

    pub fn save_answer_values(&mut self, question_index: usize, values: Vec<String>) -> bool {
        let Some(answer) = self.answers.get_mut(question_index) else {
            return false;
        };
        answer.selected_option_index = None;
        answer.selected_label = values.first().cloned();
        answer.selected_labels = values;
        answer.other_answer.clear();
        answer.skipped = false;
        true
    }

    fn load_current_answer(&mut self) {
        let saved = self
            .answers
            .get(self.current_question_index)
            .cloned()
            .unwrap_or_default();
        let default_option = self
            .current_question()
            .and_then(UserInputQuestionPresentation::recommended_option_index)
            .or_else(|| {
                self.current_question()
                    .filter(|question| !question.options.is_empty())
                    .map(|_| 0)
            });
        // A skipped question is intentionally unanswered. Returning to it must
        // not resurrect the recommended/default option as a checked answer;
        // otherwise pressing Next would silently submit the value the user
        // explicitly skipped.
        self.selected_option_index = if saved.skipped {
            None
        } else {
            saved.selected_option_index.or(default_option)
        };
        self.other_answer = saved.other_answer;
        self.keyboard_focus = None;
        self.visual_state = UserInputVisualState {
            active_option_index: if saved.skipped || !self.other_answer.is_empty() {
                None
            } else {
                self.selected_option_index
            },
            ..UserInputVisualState::default()
        };
    }

    /// Applies the keyboard behavior observed in CDP 92–98. Arrow keys move
    /// the checked answer while keeping DOM focus on the same radio; Enter
    /// submits the checked value, and Escape closes the blocking request.
    pub fn keyboard_event(
        &mut self,
        key: &str,
        _key_char: Option<&str>,
        shift: bool,
        _command: bool,
        _control: bool,
    ) -> Option<UserInputKeyboardOutcome> {
        let question = self.current_question()?.clone();
        let can_go_back = self.is_multi_question() && self.current_question_index > 0;
        let can_go_forward = self.is_multi_question() && !self.is_last_question();
        let focus_order = || {
            // The real DOM places enabled previous/next header buttons before
            // dismiss and skips disabled navigation buttons in the Tab order.
            let mut order = Vec::with_capacity(question.options.len() + 5);
            if can_go_back {
                order.push(UserInputKeyboardFocus::Previous);
            }
            if can_go_forward {
                order.push(UserInputKeyboardFocus::Next);
            }
            order.push(UserInputKeyboardFocus::Dismiss);
            order.extend((0..question.options.len()).map(UserInputKeyboardFocus::Option));
            if question.allows_other {
                order.push(UserInputKeyboardFocus::Other);
                order.push(UserInputKeyboardFocus::Skip);
            }
            order
        };

        match key {
            "tab" => {
                let order = focus_order();
                let next = match self
                    .keyboard_focus
                    .and_then(|focused| order.iter().position(|candidate| *candidate == focused))
                {
                    Some(index) if shift => order[(index + order.len() - 1) % order.len()],
                    Some(index) => order[(index + 1) % order.len()],
                    None if shift => *order.last()?,
                    None => order[0],
                };
                self.keyboard_focus = Some(next);
                self.visual_state.focused_option_index = match next {
                    UserInputKeyboardFocus::Option(index) => Some(index),
                    _ => None,
                };
                self.visual_state.skip_hovered = false;
                Some(UserInputKeyboardOutcome::Handled)
            }
            "escape" => Some(UserInputKeyboardOutcome::Dismiss),
            "down" | "up" => {
                let UserInputKeyboardFocus::Option(_) = self.keyboard_focus? else {
                    return None;
                };
                if question.options.is_empty() {
                    return None;
                }
                let current = self.selected_option_index.unwrap_or(0);
                let next = if key == "down" {
                    (current + 1) % question.options.len()
                } else {
                    (current + question.options.len() - 1) % question.options.len()
                };
                self.selected_option_index = Some(next);
                self.visual_state.active_option_index = Some(next);
                Some(UserInputKeyboardOutcome::Handled)
            }
            "enter" => match self.keyboard_focus {
                Some(UserInputKeyboardFocus::Previous) => {
                    Some(UserInputKeyboardOutcome::PreviousQuestion)
                }
                Some(UserInputKeyboardFocus::Next) => Some(UserInputKeyboardOutcome::NextQuestion),
                Some(UserInputKeyboardFocus::Dismiss) => Some(UserInputKeyboardOutcome::Dismiss),
                Some(UserInputKeyboardFocus::Option(_)) => {
                    let option_index = self.selected_option_index?;
                    let label = question.options.get(option_index)?.label.clone();
                    Some(UserInputKeyboardOutcome::SubmitOption {
                        question_id: question.id,
                        option_index,
                        label,
                    })
                }
                Some(UserInputKeyboardFocus::Other) if !self.other_answer.trim().is_empty() => {
                    Some(UserInputKeyboardOutcome::SubmitOther {
                        question_id: question.id,
                        answer: self.other_answer.trim().to_owned(),
                    })
                }
                Some(UserInputKeyboardFocus::Skip) => Some(UserInputKeyboardOutcome::Skip),
                _ => Some(UserInputKeyboardOutcome::Handled),
            },
            _ => None,
        }
    }
}

/// Deterministic two-question fixtures derived from the natural N01-N04 and
/// controlled C02/D01-D02 renderer captures. These are screenshot-only
/// presentation states; they do not dispatch an app-server request.
pub fn captured_multi_question_fixture(
    mode: ThemeMode,
    state: &str,
) -> UserInputRequestPresentation {
    let questions = vec![
        UserInputQuestionPresentation::single_choice(
            "color",
            "请选择界面主色。",
            vec![
                UserInputOptionPresentation::recommended("蓝色", Some("清晰稳定".to_owned())),
                UserInputOptionPresentation::new("红色", Some("醒目强调".to_owned())),
            ],
        ),
        UserInputQuestionPresentation::single_choice(
            "shape",
            "请选择图标形状。",
            vec![
                UserInputOptionPresentation::recommended("圆形", Some("柔和连续".to_owned())),
                UserInputOptionPresentation::new("方形", Some("规整直接".to_owned())),
            ],
        ),
    ];
    let mut model = UserInputRequestPresentation::pending("user-input-multi-capture", questions);

    match state {
        "multi-q2-navigation" => {
            model.save_selected_option(1, "红色".to_owned());
            model.next_question();
            // N02/N04 stabilized with the checked first option active. D02
            // instead retained the pointer over the second row after next.
            model.visual_state = match mode {
                ThemeMode::Light => UserInputVisualState::option_active(0),
                ThemeMode::Dark => UserInputVisualState::option_active_exclusive(1),
            };
        }
        "multi-previous-answer" => {
            model.save_selected_option(1, "红色".to_owned());
            model.next_question();
            model.previous_question();
            model.visual_state = UserInputVisualState::option_active(1);
        }
        "multi-skip-navigation" => {
            model.skip_current_question();
            model.next_question();
            // C02 kept Q2's default checked state, while the pointer remained
            // on the same Other/Skip row and neither option had activity fill.
            model.visual_state = UserInputVisualState::skip_hovered();
        }
        "multi-q1-default" => {
            model.visual_state = UserInputVisualState::option_active(0);
        }
        _ => panic!("unknown multi-question capture state: {state}"),
    }

    model
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserInputKeyboardFocus {
    Previous,
    Next,
    Dismiss,
    Option(usize),
    Other,
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserInputKeyboardOutcome {
    Handled,
    PreviousQuestion,
    NextQuestion,
    SubmitOption {
        question_id: String,
        option_index: usize,
        label: String,
    },
    SubmitOther {
        question_id: String,
        answer: String,
    },
    Skip,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UserInputCardGeometry {
    pub card_height: f32,
    pub option_count: usize,
}

impl UserInputCardGeometry {
    pub fn for_question(question: &UserInputQuestionPresentation) -> Self {
        let option_height: f32 = question
            .options
            .iter()
            .map(|option| {
                if option.recommended {
                    USER_INPUT_RECOMMENDED_OPTION_HEIGHT
                } else {
                    USER_INPUT_OPTION_HEIGHT
                }
            })
            .sum();
        let gap_count = question.options.len().saturating_sub(1)
            + usize::from(question.allows_other && !question.options.is_empty());
        let card_height = USER_INPUT_HEADER_HEIGHT
            + USER_INPUT_CONTENT_TOP_PADDING
            + option_height
            + USER_INPUT_OPTION_GAP * gap_count as f32
            + if question.allows_other {
                USER_INPUT_OTHER_ROW_HEIGHT
            } else {
                0.0
            }
            + USER_INPUT_CONTENT_BOTTOM_PADDING;
        Self {
            card_height,
            option_count: question.options.len(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserInputRequestEvent {
    SelectOption {
        question_id: String,
        option_index: usize,
        label: String,
    },
    BeginOtherAnswer {
        question_id: String,
        is_secret: bool,
    },
    SubmitOtherAnswer {
        question_id: String,
        answer: String,
    },
    Skip,
    PreviousQuestion,
    NextQuestion,
    Dismiss,
    ActiveOptionChanged(Option<usize>),
}

#[derive(Clone)]
pub struct UserInputRequestCallback(
    Rc<dyn Fn(UserInputRequestEvent, &mut Window, &mut App) + 'static>,
);

impl UserInputRequestCallback {
    pub fn new(callback: impl Fn(UserInputRequestEvent, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(callback))
    }

    fn emit(&self, event: UserInputRequestEvent, window: &mut Window, cx: &mut App) {
        (self.0)(event, window, cx);
    }
}

#[derive(Clone, Copy)]
struct UserInputPalette {
    mode: ThemeMode,
    card: gpui::Rgba,
    outline: gpui::Rgba,
    text: gpui::Rgba,
    secondary: gpui::Rgba,
    soft: gpui::Rgba,
    border: gpui::Rgba,
    focus: gpui::Rgba,
    recommendation: gpui::Rgba,
    recommendation_text: gpui::Rgba,
}

impl UserInputPalette {
    fn for_theme(theme: Theme) -> Self {
        if theme.surface == rgba(0x181818ff) {
            Self {
                mode: ThemeMode::Dark,
                card: rgba(0x2c2c2cff),
                outline: rgba(0xffffff14),
                text: rgba(0xdfdfdfff),
                // CoreText produces denser PingFang glyph coverage than
                // Chromium. This calibrated alpha matches the captured
                // `text-codex-description` raster, not merely its CSS token.
                secondary: rgba(0xffffff63),
                soft: rgba(0xffffff0b),
                border: rgba(0xffffff15),
                // The captured Electron compositor resolves the CSS ring to
                // this exact raster color on the dark card.
                focus: rgba(0x799ec8ff),
                recommendation: rgba(0xffffff0b),
                recommendation_text: rgba(0xffffffb5),
            }
        } else {
            Self {
                mode: ThemeMode::Light,
                card: rgba(0xffffffff),
                outline: rgba(0x1a1c1f10),
                text: rgba(0x1a1c1fff),
                // See the dark-palette note above. The browser's antialiasing
                // makes the same CSS token lighter than a direct GPUI draw.
                secondary: rgba(0x1a1c1f6a),
                soft: rgba(0x0000000c),
                border: rgba(0x1a1c1f14),
                // Chromium's color-managed screenshot raster for the ring.
                focus: rgba(0x539af8ff),
                recommendation: rgba(0x0000000b),
                recommendation_text: rgba(0x1a1c1fb5),
            }
        }
    }

    fn shadows(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow::new(px(0.0), px(0.0), self.outline.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(3.0), rgba(0x0000000a).into()).blur_radius(px(7.5)),
            BoxShadow::new(px(0.0), px(0.0), rgba(0x0000000d).into()).blur_radius(px(20.0)),
        ]
    }

    fn for_multi_question(mut self, enabled: bool) -> Self {
        if enabled && self.mode == ThemeMode::Light {
            // N01-N04/C02 resolve the renderer's `bg-text/5` to #f4f4f4.
            // The earlier single-question captures resolve their calibrated
            // activity token to #f3f3f3 and must remain unchanged.
            self.soft = rgba(0x1a1c1f0c);
        }
        self
    }
}

/// Render the currently active question. A submitted request stays mounted in
/// a disabled status state until `serverRequest/resolved` finalizes it.
pub fn render_user_input_request(
    model: &UserInputRequestPresentation,
    theme: Theme,
    other_input: Entity<PromptInput>,
    callback: UserInputRequestCallback,
) -> Option<Stateful<Div>> {
    if !model.should_render() {
        return None;
    }
    if !model.is_interactive() {
        return Some(render_user_input_status(model, theme));
    }

    let question = model.current_question()?;
    let geometry = UserInputCardGeometry::for_question(question);
    let palette = UserInputPalette::for_theme(theme).for_multi_question(model.is_multi_question());
    let question_id = question.id.clone();
    let dismiss_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Dismiss);
    let navigation_callback = callback.clone();

    let dismiss_callback = callback.clone();
    let header = div()
        .h(px(USER_INPUT_HEADER_HEIGHT))
        .pl(px(16.0))
        .pr(px(12.0))
        .pt(px(16.0))
        .pb(px(8.0))
        .flex()
        .items_start()
        .justify_between()
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_start()
                .justify_between()
                .child(
                    div()
                        .min_w(px(0.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text)
                        .relative()
                        .left(px(-1.0))
                        .child(question.display_question().to_owned()),
                )
                .when(model.is_multi_question(), |content| {
                    content.child(render_question_navigation(
                        model,
                        palette,
                        navigation_callback,
                    ))
                }),
        )
        .child(
            div()
                .id(element_id(
                    "user-input-dismiss",
                    &model.request_id,
                    question_id.as_str(),
                ))
                .role(Role::Button)
                .aria_label("忽略")
                .size(px(26.0))
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .text_color(palette.secondary)
                .cursor_pointer()
                .hover(move |button| button.bg(palette.soft))
                .on_click(move |_, window, cx| {
                    dismiss_callback.emit(UserInputRequestEvent::Dismiss, window, cx);
                })
                .when(dismiss_focused, |button| {
                    button.child(
                        div()
                            .absolute()
                            .inset(px(-2.0))
                            .rounded(px(12.0))
                            .border_2()
                            .border_color(palette.focus),
                    )
                })
                .child(
                    icon("close-dialog", palette.secondary.into())
                        .size(px(16.0))
                        .relative()
                        .left(px(-1.0)),
                ),
        );

    let mut options = div().flex().flex_col().gap(px(USER_INPUT_OPTION_GAP));
    for (index, option) in question.options.iter().enumerate() {
        options = options.child(render_option(
            model,
            question,
            option,
            index,
            palette,
            callback.clone(),
        ));
    }

    if question.allows_other {
        options = options.child(render_other_row(
            model,
            question,
            palette,
            other_input,
            callback.clone(),
        ));
    }

    Some(
        div()
            .id(element_id(
                "user-input-card",
                &model.request_id,
                model.current_question_index,
            ))
            .role(Role::Form)
            .aria_label(question.display_question().to_owned())
            .h(px(geometry.card_height))
            .w_full()
            .overflow_hidden()
            .rounded(px(USER_INPUT_CARD_RADIUS))
            .bg(palette.card)
            .shadow(palette.shadows())
            .font_family("PingFang SC")
            .text_color(palette.text)
            .child(header)
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(USER_INPUT_CONTENT_TOP_PADDING))
                    .pb(px(USER_INPUT_CONTENT_BOTTOM_PADDING))
                    .child(options),
            ),
    )
}

fn render_user_input_status(model: &UserInputRequestPresentation, theme: Theme) -> Stateful<Div> {
    let palette = UserInputPalette::for_theme(theme).for_multi_question(model.is_multi_question());
    let title = model
        .current_question()
        .map(UserInputQuestionPresentation::display_question)
        .unwrap_or("用户输入请求")
        .to_owned();
    let status = match model.status {
        UserInputRequestStatus::Submitting => "正在提交…",
        UserInputRequestStatus::Cancelled => "请求已取消",
        UserInputRequestStatus::Failed => "提交失败",
        UserInputRequestStatus::Pending => "等待输入",
        UserInputRequestStatus::Resolved => "已完成",
    };
    div()
        .id(element_id("user-input-card", &model.request_id, "status"))
        .role(Role::Alert)
        .aria_label(format!("{title}，{status}"))
        .min_h(px(104.0))
        .w_full()
        .px(px(16.0))
        .py(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .rounded(px(USER_INPUT_CARD_RADIUS))
        .bg(palette.card)
        .shadow(palette.shadows())
        .font_family("PingFang SC")
        .text_color(palette.text)
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(palette.secondary)
                .child(status),
        )
        .when_some(model.failure_message.clone(), |card, message| {
            card.child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.secondary)
                    .child(message),
            )
        })
}

fn render_question_navigation(
    model: &UserInputRequestPresentation,
    palette: UserInputPalette,
    callback: UserInputRequestCallback,
) -> Div {
    let can_go_back = model.current_question_index > 0;
    let can_go_forward = !model.is_last_question();
    let previous_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Previous);
    let next_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Next);
    let previous_callback = callback.clone();

    div()
        .h(px(24.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(palette.secondary)
        .child(
            div()
                .id(element_id(
                    "user-input-previous",
                    &model.request_id,
                    model.current_question_index,
                ))
                .role(Role::Button)
                .aria_label("上一题")
                .size(px(24.0))
                .relative()
                .p(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .text_color(palette.secondary)
                .opacity(if can_go_back { 1.0 } else { 0.4 })
                .when(can_go_back, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(palette.soft))
                        .on_click(move |_, window, cx| {
                            previous_callback.emit(
                                UserInputRequestEvent::PreviousQuestion,
                                window,
                                cx,
                            );
                        })
                })
                .when(previous_focused, |button| {
                    button.child(
                        div()
                            .absolute()
                            .inset(px(-2.0))
                            .rounded(px(12.0))
                            .border_2()
                            .border_color(palette.focus),
                    )
                })
                .child(
                    icon("chevron-down", palette.secondary.into())
                        .size(px(14.0))
                        .with_transformation(Transformation::rotate(radians(
                            std::f32::consts::FRAC_PI_2,
                        ))),
                ),
        )
        .child(
            div()
                .id(element_id(
                    "user-input-progress",
                    &model.request_id,
                    model.current_question_index,
                ))
                .role(Role::Status)
                .aria_label(format!("问题 {}", model.progress_label()))
                .h(px(16.0))
                .text_color(palette.secondary)
                .child(model.progress_label()),
        )
        .child(
            div()
                .id(element_id(
                    "user-input-next",
                    &model.request_id,
                    model.current_question_index,
                ))
                .role(Role::Button)
                .aria_label("下一题")
                .size(px(24.0))
                .relative()
                .p(px(4.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .text_color(palette.secondary)
                .opacity(if can_go_forward { 1.0 } else { 0.4 })
                .when(can_go_forward, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(palette.soft))
                        .on_click(move |_, window, cx| {
                            callback.emit(UserInputRequestEvent::NextQuestion, window, cx);
                        })
                })
                .when(next_focused, |button| {
                    button.child(
                        div()
                            .absolute()
                            .inset(px(-2.0))
                            .rounded(px(12.0))
                            .border_2()
                            .border_color(palette.focus),
                    )
                })
                .child(
                    icon("chevron-down", palette.secondary.into())
                        .size(px(14.0))
                        .with_transformation(Transformation::rotate(radians(
                            -std::f32::consts::FRAC_PI_2,
                        ))),
                ),
        )
}

fn render_option(
    model: &UserInputRequestPresentation,
    question: &UserInputQuestionPresentation,
    option: &UserInputOptionPresentation,
    index: usize,
    palette: UserInputPalette,
    callback: UserInputRequestCallback,
) -> Stateful<Div> {
    let is_active = model.visual_state.active_option_index == Some(index);
    let is_focused = model.visual_state.focused_option_index == Some(index)
        || model.keyboard_focus == Some(UserInputKeyboardFocus::Option(index));
    let is_keyboard_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Option(index));
    let is_selected = model.selected_option_index == Some(index);
    // The captured single-question light interaction moves the activity fill
    // to the hovered/focused row, while dark keeps the checked row visible
    // during that secondary activity. A plain dark default still fills only
    // its checked/active row, leaving every unselected row transparent (D01).
    let dark_secondary_activity = palette.mode == ThemeMode::Dark
        && model.visual_state.retain_checked_fill
        && (model.visual_state.active_option_index != model.selected_option_index
            || model.visual_state.focused_option_index.is_some());
    let has_activity_fill = is_active || (is_selected && dark_secondary_activity);
    let row_height = if option.recommended {
        USER_INPUT_RECOMMENDED_OPTION_HEIGHT
    } else {
        USER_INPUT_OPTION_HEIGHT
    };
    let label = option.label.clone();
    let accessible_label = option.accessible_label();
    let description = option.description.clone();
    let question_id = question.id.clone();
    let hover_callback = callback.clone();
    let click_callback = callback;

    let mut title = div()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .min_w(px(0.0))
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(label.clone()),
        );
    if option.recommended {
        title = title.child(
            div()
                .w(px(40.0))
                .h(px(20.0))
                .relative()
                .top(px(-1.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .bg(palette.recommendation)
                .text_size(px(12.0))
                .line_height(px(12.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.recommendation_text)
                .child("推荐"),
        );
    }

    let mut copy = div()
        .min_w(px(0.0))
        .flex_1()
        .pl(px(3.0))
        .flex()
        .flex_col()
        .gap(px(2.0));
    if option.recommended {
        copy = copy.relative().top(px(1.0));
    }
    copy = copy.child(title);
    if let Some(description) = description.as_deref().and_then(non_empty) {
        copy = copy.child(
            div()
                .min_w(px(0.0))
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.secondary)
                .when(option.recommended, |description| {
                    description.relative().top(px(-1.0))
                })
                .child(description.to_owned()),
        );
    }

    div()
        .id(element_id("user-input-option", &model.request_id, index))
        .role(Role::RadioButton)
        .aria_label(accessible_label)
        .when_some(option.description.clone(), |row, description| {
            row.aria_description(description)
        })
        .aria_selected(is_selected)
        .h(px(row_height))
        .w_full()
        .px(px(8.0))
        .py(px(6.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .relative()
        .rounded(px(15.0))
        .when(has_activity_fill, |row| row.bg(palette.soft))
        .text_color(palette.text)
        .cursor_pointer()
        .hover(move |row| row.bg(palette.soft))
        .on_hover(move |hovered, window, cx| {
            hover_callback.emit(
                UserInputRequestEvent::ActiveOptionChanged((*hovered).then_some(index)),
                window,
                cx,
            );
        })
        .on_click(move |_, window, cx| {
            click_callback.emit(
                UserInputRequestEvent::SelectOption {
                    question_id: question_id.clone(),
                    option_index: index,
                    label: label.clone(),
                },
                window,
                cx,
            );
        })
        .when(is_focused, |row| {
            row.child(
                div()
                    .absolute()
                    .top(px(-2.0))
                    .right(px(if is_keyboard_focused { -2.0 } else { -1.0 }))
                    .bottom(px(-2.0))
                    .left(px(if is_keyboard_focused { -2.0 } else { -3.0 }))
                    // GPUI strokes borders inside the box; radius 15 yields
                    // the captured CSS shadow's effective outer radius 17.
                    .rounded(px(if is_keyboard_focused { 15.0 } else { 17.0 }))
                    .border_2()
                    .border_color(palette.focus),
            )
        })
        .child(
            div()
                .size(px(USER_INPUT_CONTROL_SIZE))
                .flex_none()
                .self_start()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9999.0))
                .border_1()
                .border_color(palette.border)
                .bg(palette.soft)
                .text_size(px(12.0))
                .line_height(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.secondary)
                .when(option.recommended, |control| {
                    control.relative().top(px(1.0))
                })
                .child((index + 1).to_string()),
        )
        .child(copy)
        .child(
            icon("user-input-submit", palette.secondary.into())
                .size(px(16.0))
                .when(option.recommended, |arrow| arrow.relative().top(px(1.0)))
                .when(!has_activity_fill && !is_focused, |arrow| {
                    arrow.opacity(0.0)
                }),
        )
}

fn render_other_row(
    model: &UserInputRequestPresentation,
    question: &UserInputQuestionPresentation,
    palette: UserInputPalette,
    other_input: Entity<PromptInput>,
    callback: UserInputRequestCallback,
) -> Stateful<Div> {
    let question_id = question.id.clone();
    let is_secret = question.is_secret;
    let other_callback = callback.clone();
    let submit_callback = callback.clone();
    let skip_callback = callback;
    let skip_hovered = model.visual_state.skip_hovered;
    let other_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Other);
    let skip_focused = model.keyboard_focus == Some(UserInputKeyboardFocus::Skip);
    let other_answer = model.other_answer.clone();
    let has_other_answer = !other_answer.trim().is_empty();
    let submitted_question_id = question.id.clone();
    let submitted_answer = other_answer.trim().to_owned();

    div()
        .id(element_id(
            "user-input-other",
            &model.request_id,
            question.id.as_str(),
        ))
        .role(Role::Group)
        .aria_label(question.other_placeholder.clone())
        .h(px(USER_INPUT_OTHER_ROW_HEIGHT))
        .w_full()
        .px(px(8.0))
        .py(px(6.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .relative()
        .rounded(px(20.0))
        .cursor_text()
        .on_click(move |_, window, cx| {
            other_callback.emit(
                UserInputRequestEvent::BeginOtherAnswer {
                    question_id: question_id.clone(),
                    is_secret,
                },
                window,
                cx,
            );
        })
        .when(skip_hovered || other_focused, |row| {
            row.bg(
                if other_focused && !has_other_answer && palette.mode == ThemeMode::Light {
                    rgba(0xf4f4f4ff)
                } else {
                    palette.soft
                },
            )
        })
        .hover(move |row| row.bg(palette.soft))
        .when(other_focused, |row| {
            row.child(
                div()
                    .absolute()
                    // CSS `ring-1` is an outer box-shadow. GPUI borders are
                    // inset, so expand the native child by one pixel.
                    .inset(px(-1.0))
                    .rounded(px(21.0))
                    .border_1()
                    .border_color(palette.focus),
            )
        })
        .child(
            div()
                .id(element_id(
                    "user-input-other-answer",
                    &model.request_id,
                    question.id.as_str(),
                ))
                .role(Role::Group)
                .aria_label(question.other_placeholder.clone())
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .items_center()
                .gap(px(if other_focused { 8.0 } else { 10.0 }))
                .cursor_text()
                .child(
                    div()
                        // The Other affordance uses the composer button token
                        // (28×28), not the numbered option token (24×24).
                        .size(px(if other_focused {
                            28.0
                        } else {
                            USER_INPUT_CONTROL_SIZE
                        }))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9999.0))
                        .border_1()
                        .border_color(palette.border)
                        .bg(palette.soft)
                        .child(icon("user-input-other", palette.secondary.into()).size(px(14.0))),
                )
                .child(div().min_w(px(0.0)).flex_1().h(px(28.0)).child(other_input)),
        )
        .child(
            div()
                .id(element_id(
                    "user-input-skip",
                    &model.request_id,
                    question.id.as_str(),
                ))
                .role(Role::Button)
                .aria_label("跳过")
                .h(px(USER_INPUT_SKIP_HEIGHT))
                .px(px(8.0))
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .rounded(px(9999.0))
                .border_1()
                .border_color(if has_other_answer {
                    palette.text
                } else {
                    palette.border
                })
                .bg(if has_other_answer {
                    palette.text
                } else if palette.mode == ThemeMode::Dark {
                    palette.soft
                } else {
                    palette.card
                })
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if has_other_answer {
                    palette.card
                } else {
                    palette.text
                })
                .cursor_pointer()
                .hover(move |button| button.bg(palette.soft))
                .on_click(move |_, window, cx| {
                    // This button is nested in the clickable Other row. Stop
                    // bubbling so Skip/Next cannot also reopen and focus the
                    // Other editor on the newly active question.
                    cx.stop_propagation();
                    if has_other_answer {
                        submit_callback.emit(
                            UserInputRequestEvent::SubmitOtherAnswer {
                                question_id: submitted_question_id.clone(),
                                answer: submitted_answer.clone(),
                            },
                            window,
                            cx,
                        );
                    } else {
                        skip_callback.emit(UserInputRequestEvent::Skip, window, cx);
                    }
                })
                .when(skip_focused, |button| {
                    button.child(
                        div()
                            .absolute()
                            .inset(px(-2.0))
                            .rounded(px(9999.0))
                            .border_2()
                            .border_color(palette.focus),
                    )
                })
                .child(if has_other_answer {
                    "下一步"
                } else {
                    "跳过"
                }),
        )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{
        Bounds, Context, IntoElement, MouseButton, Render, TestApp, Window, WindowBounds,
        WindowOptions, point, px, size,
    };

    use super::*;

    struct UserInputHarness {
        model: UserInputRequestPresentation,
        other_input: Entity<PromptInput>,
        events: Rc<RefCell<Vec<UserInputRequestEvent>>>,
    }

    impl Render for UserInputHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let events = self.events.clone();
            render_user_input_request(
                &self.model,
                Theme::for_mode(ThemeMode::Dark),
                self.other_input.clone(),
                UserInputRequestCallback::new(move |event, _, _| {
                    events.borrow_mut().push(event);
                }),
            )
            .expect("pending user-input request remains mounted")
        }
    }

    fn captured_question() -> UserInputQuestionPresentation {
        UserInputQuestionPresentation::single_choice(
            "color",
            "请选择一种颜色。",
            vec![
                UserInputOptionPresentation::recommended(
                    "红色",
                    Some("选择红色作为你的单选答案。".to_owned()),
                ),
                UserInputOptionPresentation::new(
                    "蓝色",
                    Some("选择蓝色作为你的单选答案。".to_owned()),
                ),
            ],
        )
    }

    #[test]
    fn captured_two_option_geometry_is_exact() {
        let geometry = UserInputCardGeometry::for_question(&captured_question());
        assert_eq!(geometry.option_count, 2);
        assert_eq!(geometry.card_height, USER_INPUT_CAPTURED_TWO_OPTION_HEIGHT);
    }

    #[test]
    fn pending_model_selects_and_activates_the_recommended_option() {
        let model = UserInputRequestPresentation::pending("request-1", vec![captured_question()]);
        assert_eq!(model.selected_option_index, Some(0));
        assert_eq!(model.visual_state.active_option_index, Some(0));
        assert_eq!(
            model.current_question().unwrap().display_question(),
            "请选择一种颜色。"
        );
    }

    #[test]
    fn hover_activity_and_keyboard_focus_are_independent() {
        let state = UserInputVisualState::option_focused(1, 0);
        assert_eq!(state.active_option_index, Some(1));
        assert_eq!(state.focused_option_index, Some(0));
        assert!(!state.skip_hovered);
    }

    #[test]
    fn submitting_stays_visible_and_disabled_until_resolved_unmounts() {
        let mut model =
            UserInputRequestPresentation::pending("request-1", vec![captured_question()]);
        assert!(model.should_render());
        model.status = UserInputRequestStatus::Submitting;
        assert!(model.should_render());
        assert!(!model.is_interactive());
        model.status = UserInputRequestStatus::Resolved;
        assert!(!model.should_render());
    }

    #[test]
    fn production_answer_serialization_preserves_multi_select_other_and_redacts_debug() {
        let mut question = captured_question();
        question.id = "secret-choice".into();
        question.is_secret = true;
        let mut model = UserInputRequestPresentation::pending("request-secret", vec![question]);
        assert!(model.save_answer_values(0, vec!["red".into(), "blue".into(), "green".into()]));
        model.answers[0].other_answer = "private custom value".into();
        assert_eq!(
            model.response_answers(),
            vec![(
                "secret-choice".into(),
                vec![
                    "red".into(),
                    "blue".into(),
                    "green".into(),
                    "private custom value".into(),
                ],
            )]
        );
        let debug = format!("{model:?}");
        assert!(!debug.contains("private custom value"));
        assert!(!debug.contains("green"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn presentation_copy_and_accessibility_labels_match_capture() {
        let question = captured_question();
        assert_eq!(
            question.other_placeholder,
            "否，并告诉 ChatGPT 应该如何做得不同"
        );
        assert_eq!(question.options[0].accessible_label(), "红色 (Recommended)");
        assert_eq!(question.options[1].accessible_label(), "蓝色");
        assert_eq!(
            question.options[0].description.as_deref(),
            Some("选择红色作为你的单选答案。")
        );
    }

    #[test]
    fn light_and_dark_palettes_match_calibrated_capture_tokens() {
        let light = UserInputPalette::for_theme(Theme::for_mode(ThemeMode::Light));
        let dark = UserInputPalette::for_theme(Theme::for_mode(ThemeMode::Dark));
        assert_eq!(light.card, rgba(0xffffffff));
        assert_eq!(light.text, rgba(0x1a1c1fff));
        assert_eq!(light.secondary, rgba(0x1a1c1f6a));
        assert_eq!(light.focus, rgba(0x539af8ff));
        assert_eq!(dark.card, rgba(0x2c2c2cff));
        assert_eq!(dark.text, rgba(0xdfdfdfff));
        assert_eq!(dark.secondary, rgba(0xffffff63));
        assert_eq!(dark.focus, rgba(0x799ec8ff));
    }

    #[test]
    fn tab_arrow_enter_and_escape_follow_cdp_keyboard_semantics() {
        let mut model =
            UserInputRequestPresentation::pending("request-1", vec![captured_question()]);

        assert_eq!(
            model.keyboard_event("tab", None, false, false, false),
            Some(UserInputKeyboardOutcome::Handled)
        );
        assert_eq!(model.keyboard_focus, Some(UserInputKeyboardFocus::Dismiss));
        model.keyboard_event("tab", None, false, false, false);
        assert_eq!(
            model.keyboard_focus,
            Some(UserInputKeyboardFocus::Option(0))
        );

        model.keyboard_event("down", None, false, false, false);
        assert_eq!(model.selected_option_index, Some(1));
        assert_eq!(model.visual_state.active_option_index, Some(1));
        assert_eq!(model.visual_state.focused_option_index, Some(0));
        assert_eq!(
            model.keyboard_event("enter", None, false, false, false),
            Some(UserInputKeyboardOutcome::SubmitOption {
                question_id: "color".to_owned(),
                option_index: 1,
                label: "蓝色".to_owned(),
            })
        );
        assert_eq!(
            model.keyboard_event("escape", None, false, false, false),
            Some(UserInputKeyboardOutcome::Dismiss)
        );
    }

    #[test]
    fn other_answer_is_supplied_by_the_native_editor_and_enter_submits_it() {
        let mut model =
            UserInputRequestPresentation::pending("request-1", vec![captured_question()]);
        model.focus_other_answer();
        assert_eq!(
            model.keyboard_event("我", Some("我"), false, false, false),
            None
        );
        model.save_other_answer("我想喝茶。".to_owned());
        assert_eq!(model.other_answer, "我想喝茶。");
        assert_eq!(
            model.keyboard_event("enter", None, false, false, false),
            Some(UserInputKeyboardOutcome::SubmitOther {
                question_id: "color".to_owned(),
                answer: "我想喝茶。".to_owned(),
            })
        );
    }

    #[test]
    fn multiple_questions_preserve_answers_across_forward_and_back_navigation() {
        let second = UserInputQuestionPresentation::single_choice(
            "shape",
            "请选择一种形状。",
            vec![
                UserInputOptionPresentation::recommended("圆形", None),
                UserInputOptionPresentation::new("方形", None),
            ],
        );
        let mut model = UserInputRequestPresentation::pending(
            "request-multi",
            vec![captured_question(), second],
        );

        assert_eq!(model.progress_label(), "1 of 2");
        assert_eq!(model.geometry().unwrap().card_height, 213.6875);
        model.save_selected_option(1, "蓝色".to_owned());
        assert!(model.next_question());
        assert_eq!(model.current_question_index, 1);
        assert_eq!(model.progress_label(), "2 of 2");
        model.save_other_answer("三角形".to_owned());
        assert!(model.previous_question());
        assert_eq!(model.selected_option_index, Some(1));
        assert_eq!(model.other_answer, "");
        assert!(model.next_question());
        assert_eq!(model.other_answer, "三角形");
        assert_eq!(
            model.response_answers(),
            vec![
                ("color".to_owned(), vec!["蓝色".to_owned()]),
                ("shape".to_owned(), vec!["三角形".to_owned()]),
            ]
        );
    }

    #[test]
    fn persist_current_answer_handles_option_other_and_skip_drafts() {
        let question = |id, prompt, first, second| {
            UserInputQuestionPresentation::single_choice(
                id,
                prompt,
                vec![
                    UserInputOptionPresentation::recommended(first, None),
                    UserInputOptionPresentation::new(second, None),
                ],
            )
        };
        let mut model = UserInputRequestPresentation::pending(
            "request-persist-drafts",
            vec![
                question("color", "请选择一种颜色。", "红色", "蓝色"),
                question("shape", "请选择一种形状。", "圆形", "方形"),
                question("size", "请选择一种尺寸。", "小", "大"),
            ],
        );

        model.keyboard_focus = Some(UserInputKeyboardFocus::Option(0));
        assert_eq!(
            model.keyboard_event("down", None, false, false, false),
            Some(UserInputKeyboardOutcome::Handled)
        );
        model.persist_current_answer();
        assert_eq!(model.answers[0].selected_label.as_deref(), Some("蓝色"));

        assert!(model.next_question());
        model.selected_option_index = None;
        model.other_answer = " 三角形 ".to_owned();
        model.persist_current_answer();
        assert_eq!(model.answers[1].other_answer, " 三角形 ");

        assert!(model.next_question());
        model.skip_current_question();
        model.persist_current_answer();
        assert!(model.answers[2].skipped);
        assert_eq!(
            model.response_answers(),
            vec![
                ("color".to_owned(), vec!["蓝色".to_owned()]),
                ("shape".to_owned(), vec!["三角形".to_owned()]),
            ]
        );
    }

    #[test]
    fn multi_question_header_navigation_is_in_the_keyboard_order() {
        let second = UserInputQuestionPresentation::single_choice(
            "shape",
            "请选择一种形状。",
            vec![
                UserInputOptionPresentation::recommended("圆形", None),
                UserInputOptionPresentation::new("方形", None),
            ],
        );
        let mut model = UserInputRequestPresentation::pending(
            "request-multi-keyboard",
            vec![captured_question(), second],
        );

        assert_eq!(
            model.keyboard_event("tab", None, false, false, false),
            Some(UserInputKeyboardOutcome::Handled)
        );
        assert_eq!(model.keyboard_focus, Some(UserInputKeyboardFocus::Next));
        assert_eq!(
            model.keyboard_event("enter", None, false, false, false),
            Some(UserInputKeyboardOutcome::NextQuestion)
        );

        assert!(model.next_question());
        assert_eq!(
            model.keyboard_event("tab", None, false, false, false),
            Some(UserInputKeyboardOutcome::Handled)
        );
        assert_eq!(model.keyboard_focus, Some(UserInputKeyboardFocus::Previous));
        assert_eq!(
            model.keyboard_event("enter", None, false, false, false),
            Some(UserInputKeyboardOutcome::PreviousQuestion)
        );
    }

    #[test]
    fn skipped_questions_are_omitted_from_the_wire_answer_map() {
        let second = UserInputQuestionPresentation::single_choice(
            "shape",
            "请选择一种形状。",
            vec![
                UserInputOptionPresentation::recommended("圆形", None),
                UserInputOptionPresentation::new("方形", None),
            ],
        );
        let mut model = UserInputRequestPresentation::pending(
            "request-skipped",
            vec![captured_question(), second],
        );

        model.skip_current_question();
        assert!(model.next_question());
        model.save_selected_option(1, "方形".to_owned());
        assert_eq!(
            model.response_answers(),
            vec![("shape".to_owned(), vec!["方形".to_owned()])]
        );
        model.skip_current_question();
        assert!(model.response_answers().is_empty());
    }

    #[test]
    fn revisiting_a_skipped_question_does_not_restore_a_default_answer() {
        let second = UserInputQuestionPresentation::single_choice(
            "shape",
            "请选择一种形状。",
            vec![
                UserInputOptionPresentation::recommended("圆形", None),
                UserInputOptionPresentation::new("方形", None),
            ],
        );
        let mut model = UserInputRequestPresentation::pending(
            "request-skipped-revisit",
            vec![captured_question(), second],
        );

        model.skip_current_question();
        assert!(model.next_question());
        assert!(model.previous_question());
        assert!(model.answers[0].skipped);
        assert_eq!(model.selected_option_index, None);
        assert_eq!(model.visual_state.active_option_index, None);
        assert!(model.next_question());
        assert!(model.answers[0].skipped);
        assert!(model.response_answers().is_empty());
    }

    #[test]
    fn captured_multi_question_fixtures_match_observed_navigation_states() {
        let q1 = captured_multi_question_fixture(ThemeMode::Light, "multi-q1-default");
        assert_eq!(q1.current_question_index, 0);
        assert_eq!(q1.progress_label(), "1 of 2");
        assert_eq!(q1.selected_option_index, Some(0));
        assert_eq!(q1.visual_state, UserInputVisualState::option_active(0));
        assert_eq!(q1.geometry().unwrap().card_height, 213.6875);

        let q2_light = captured_multi_question_fixture(ThemeMode::Light, "multi-q2-navigation");
        assert_eq!(q2_light.current_question_index, 1);
        assert_eq!(q2_light.answers[0].selected_label.as_deref(), Some("红色"));
        assert_eq!(q2_light.selected_option_index, Some(0));
        assert_eq!(
            q2_light.visual_state,
            UserInputVisualState::option_active(0)
        );

        let q2_dark = captured_multi_question_fixture(ThemeMode::Dark, "multi-q2-navigation");
        assert_eq!(q2_dark.selected_option_index, Some(0));
        assert_eq!(
            q2_dark.visual_state,
            UserInputVisualState::option_active_exclusive(1)
        );

        let previous = captured_multi_question_fixture(ThemeMode::Light, "multi-previous-answer");
        assert_eq!(previous.current_question_index, 0);
        assert_eq!(previous.selected_option_index, Some(1));
        assert_eq!(previous.answers[0].selected_label.as_deref(), Some("红色"));

        let skipped = captured_multi_question_fixture(ThemeMode::Light, "multi-skip-navigation");
        assert_eq!(skipped.current_question_index, 1);
        assert!(skipped.answers[0].skipped);
        assert_eq!(skipped.selected_option_index, Some(0));
        assert_eq!(skipped.visual_state, UserInputVisualState::skip_hovered());
        assert!(skipped.response_answers().is_empty());
    }

    #[test]
    fn skip_button_does_not_bubble_into_begin_other_answer() {
        let model =
            UserInputRequestPresentation::pending("request-no-bubble", vec![captured_question()]);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(736.0), px(USER_INPUT_CAPTURED_TWO_OPTION_HEIGHT)),
                })),
                ..Default::default()
            },
            |_, cx| UserInputHarness {
                model,
                other_input: cx.new(|cx| {
                    PromptInput::inline_other(
                        ThemeMode::Dark,
                        "否，并告诉 ChatGPT 应该如何做得不同",
                        false,
                        cx,
                    )
                }),
                events,
            },
        );

        window.draw();
        window.simulate_click(point(px(700.0), px(185.0)), MouseButton::Left);
        assert_eq!(
            captured_events.borrow().as_slice(),
            &[UserInputRequestEvent::Skip]
        );
    }

    #[test]
    fn blank_question_uses_captured_style_fallback_copy() {
        let mut question = captured_question();
        question.question = "  ".to_owned();
        question.header = Some("选择颜色".to_owned());
        assert_eq!(question.display_question(), "选择颜色");
        question.header = Some(" ".to_owned());
        assert_eq!(question.display_question(), "请选择一个选项。");
    }
}
