use std::time::Duration;

use chrono::Local;
use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, Entity, FocusHandle, KeyDownEvent,
    MouseButton, Render, Transformation, Window, deferred, div, hsla, linear_color_stop,
    linear_gradient, prelude::*, px, radians, rgba,
};

use crate::{
    agent::{
        AgentBackend, AgentEvent, AgentModel, AgentModelCatalog, AgentRequest,
        CodexAppServerBackend, CommandExecution, CommandExecutionStatus,
    },
    components::{
        icons::icon,
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    },
    theme::{Theme, ThemeMode},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerSubmenu {
    Model,
    Effort,
    ServiceTier,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DictationState {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionMode {
    Request,
    Assist,
    Full,
    Custom,
}

pub struct RequestFullAccess;
impl gpui::EventEmitter<RequestFullAccess> for ComposerView {}

pub struct ModelCatalogLoadFinished;
impl gpui::EventEmitter<ModelCatalogLoadFinished> for ComposerView {}

pub struct ConversationChanged;
impl gpui::EventEmitter<ConversationChanged> for ComposerView {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConversationPhase {
    #[default]
    Empty,
    Starting,
    Thinking,
    Streaming,
    Complete,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationActivity {
    AssistantMessage { item_id: String, text: String },
    Command(CommandExecution),
    Error { message: String },
}

const MODEL_PICKER_WIDTH: f32 = 224.0;
const MODEL_PICKER_SUBMENU_GAP: f32 = 1.0;
const MODEL_PICKER_TRIGGER_GAP: f32 = 4.0;
// The open trigger reserves 224px and ends immediately before the 64px
// dictation/send group inside the composer's 8px trailing inset.
const MODEL_PICKER_RIGHT_INSET: f32 = 72.0;
const MODEL_PICKER_MIN_SUBMENU_WIDTH: f32 = 180.0;
const MODEL_PICKER_ROW_HEIGHT: f32 = 28.5625;
const MODEL_PICKER_DETAIL_ROW_HEIGHT: f32 = 47.125;
const MODEL_PICKER_SUBMENU_HEADER_HEIGHT: f32 = 26.0;
const MODEL_PICKER_SUBMENU_VERTICAL_PADDING: f32 = 8.0;
// Radix collision handling in the ChatGPT desktop app keeps each submenu's
// lower edge at the same viewport inset. Relative to this composer's anchored
// main menu, CDP resolves that edge to 184px below the main menu's top.
const MODEL_PICKER_SUBMENU_BOTTOM_OFFSET: f32 = 184.0;
const MODEL_PICKER_SUBMENU_MAX_HEIGHT: f32 = 420.0;
const HOME_COMPOSER_MAX_WIDTH: f32 = 748.0;
const APP_SIDEBAR_WIDTH: f32 = 256.125;
const PARTICLE_TIMELINE_MS: f32 = 120_000.0;
// Collect for half a 60 Hz frame after the first event. This catches protocol
// micro-bursts while leaving enough time for GPUI's next 60/120 Hz paint.
const STREAM_UPDATE_INTERVAL: Duration = Duration::from_millis(8);
// Keep an unexpectedly large command-output burst from monopolizing the UI
// executor. Adjacent deltas are merged before they touch view state.
const STREAM_EVENTS_PER_UPDATE: usize = 512;
const STREAM_DISCONNECTED_MESSAGE: &str = "Codex 事件流意外断开";

fn current_local_time_label() -> String {
    Local::now().format("%H:%M").to_string()
}

fn push_coalesced_agent_event(batch: &mut Vec<AgentEvent>, event: AgentEvent) {
    match event {
        AgentEvent::TextDelta(delta) => {
            if let Some(AgentEvent::TextDelta(buffered)) = batch.last_mut() {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::TextDelta(delta));
            }
        }
        AgentEvent::CommandOutputDelta { item_id, delta } => {
            if let Some(AgentEvent::CommandOutputDelta {
                item_id: buffered_item_id,
                delta: buffered,
            }) = batch.last_mut()
                && buffered_item_id == &item_id
            {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::CommandOutputDelta { item_id, delta });
            }
        }
        event => batch.push(event),
    }
}

fn collect_ready_agent_events(
    receiver: &async_channel::Receiver<AgentEvent>,
    first_event: AgentEvent,
) -> (Vec<AgentEvent>, bool) {
    let mut batch = Vec::with_capacity(16);
    push_coalesced_agent_event(&mut batch, first_event);
    let mut channel_closed = false;
    for _ in 1..STREAM_EVENTS_PER_UPDATE {
        match receiver.try_recv() {
            Ok(event) => push_coalesced_agent_event(&mut batch, event),
            Err(async_channel::TryRecvError::Empty) => break,
            Err(async_channel::TryRecvError::Closed) => {
                channel_closed = true;
                break;
            }
        }
    }
    (batch, channel_closed)
}

fn ensure_closed_batch_is_terminal(batch: &mut Vec<AgentEvent>) {
    if !batch
        .iter()
        .any(|event| matches!(event, AgentEvent::Completed | AgentEvent::Failed(_)))
    {
        batch.push(AgentEvent::Failed(STREAM_DISCONNECTED_MESSAGE.to_owned()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SubmenuLayout {
    open_left: bool,
    width: f32,
}

fn submenu_layout(viewport_width: f32, natural_width: f32) -> SubmenuLayout {
    let main_width = (viewport_width - APP_SIDEBAR_WIDTH).max(0.0);
    let composer_width = main_width.min(HOME_COMPOSER_MAX_WIDTH);
    let trailing_margin = ((main_width - composer_width) * 0.5).max(0.0);
    let available_right = trailing_margin + MODEL_PICKER_RIGHT_INSET - MODEL_PICKER_SUBMENU_GAP;

    if available_right >= MODEL_PICKER_MIN_SUBMENU_WIDTH {
        SubmenuLayout {
            open_left: false,
            width: natural_width.min(available_right),
        }
    } else {
        SubmenuLayout {
            open_left: true,
            width: natural_width,
        }
    }
}

fn particle_transition_ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // Invert x for the reference cubic-bezier(.45, 0, .55, 1), then evaluate y.
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * 0.45
            + 3.0 * inverse * parameter * parameter * 0.55
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * parameter * parameter + parameter * parameter * parameter
}

fn particle_noise(index: usize, step: u32, channel: u32) -> f32 {
    let mut value = (index as u32 + 1)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(step.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(channel.wrapping_mul(0xc2b2_ae35));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

fn max_particle_drift(progress: f32, index: usize, duration_ms: u64) -> (f32, f32) {
    let segment_count = (PARTICLE_TIMELINE_MS / duration_ms as f32).round() as u32;
    let local = progress * segment_count as f32 + index as f32 * 0.37;
    let segment = local.floor() as u32 % segment_count;
    let next_segment = (segment + 1) % segment_count;
    let eased = particle_transition_ease(local.fract());
    let interpolate = |from: f32, to: f32| from + (to - from) * eased;

    let x = interpolate(
        particle_noise(index, segment, 0),
        particle_noise(index, next_segment, 0),
    );
    let y = interpolate(
        particle_noise(index, segment, 1),
        particle_noise(index, next_segment, 1),
    );
    ((x - 0.5) * 6.0, (y - 0.5) * 8.0)
}

fn particle_layers(ultra_mode: bool, accelerated: bool) -> (bool, bool) {
    let show_fast_particles = accelerated;
    // CDP: at data-max=true + data-fast-mode=true, MaxEffects retains only
    // its gradient canvas; the drifting TrackParticles layer is unmounted.
    let show_max_particles = ultra_mode && !accelerated;
    (show_max_particles, show_fast_particles)
}

pub struct ComposerView {
    mode: ThemeMode,
    prompt_input: Entity<PromptInput>,
    user_message: Option<String>,
    user_message_time: Option<String>,
    assistant_message: String,
    conversation_activity: Vec<ConversationActivity>,
    assistant_message_time: Option<String>,
    conversation_phase: ConversationPhase,
    conversation_cycle: u64,
    model_menu_focus: FocusHandle,
    model_menu_focused_item: usize,
    model_menu_keyboard_focus: bool,
    submenu_focused_item: usize,
    submenu_keyboard_focus: bool,
    menu_open: bool,
    advanced_expanded: bool,
    submenu: Option<PickerSubmenu>,
    models: Vec<AgentModel>,
    model_catalog_error: Option<String>,
    selected_model: String,
    selected_effort: String,
    selected_service_tier: Option<String>,
    actual_model: Option<String>,
    model_status: Option<String>,
    safety_buffering: bool,
    slider_index: usize,
    slider_dragging: bool,
    dictation_state: DictationState,
    dictation_cycle: u64,
    permission_mode: PermissionMode,
    permission_menu_open: bool,
}

impl ComposerView {
    pub fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let prompt_input = cx.new(|cx| PromptInput::new(mode, cx));
        cx.subscribe(&prompt_input, |_, _, _: &PromptChanged, cx| {
            cx.notify();
        })
        .detach();
        cx.subscribe(&prompt_input, |this, _, event: &PromptSubmitted, cx| {
            this.submit_prompt(event.0.clone(), cx);
        })
        .detach();
        let view = Self {
            mode,
            prompt_input,
            user_message: None,
            user_message_time: None,
            assistant_message: String::new(),
            conversation_activity: Vec::new(),
            assistant_message_time: None,
            conversation_phase: ConversationPhase::Empty,
            conversation_cycle: 0,
            model_menu_focus: cx.focus_handle(),
            model_menu_focused_item: 0,
            model_menu_keyboard_focus: false,
            submenu_focused_item: 0,
            submenu_keyboard_focus: false,
            menu_open: false,
            advanced_expanded: true,
            submenu: None,
            models: Vec::new(),
            model_catalog_error: None,
            selected_model: String::new(),
            selected_effort: String::new(),
            selected_service_tier: None,
            actual_model: None,
            model_status: None,
            safety_buffering: false,
            slider_index: 0,
            slider_dragging: false,
            dictation_state: DictationState::Idle,
            dictation_cycle: 0,
            permission_mode: PermissionMode::Full,
            permission_menu_open: false,
        };
        #[cfg(not(test))]
        let mut view = view;
        #[cfg(not(test))]
        view.load_model_catalog(cx);
        view
    }

    #[cfg(not(test))]
    fn load_model_catalog(&mut self, cx: &mut Context<Self>) {
        let receiver = CodexAppServerBackend::new().load_model_catalog();
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err("Codex 模型目录连接在返回结果前关闭".to_owned()));
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(catalog) => this.apply_model_catalog(catalog),
                    Err(error) => this.set_model_catalog_error(error),
                }
                cx.emit(ModelCatalogLoadFinished);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_model_catalog(&mut self, catalog: AgentModelCatalog) {
        let previous_model = self.selected_model.clone();
        let previous_effort = self.selected_effort.clone();
        let previous_service_tier = self.selected_service_tier.clone();

        self.models = catalog.models;
        self.model_catalog_error = None;
        if self.models.is_empty() {
            self.set_model_catalog_error("Codex 未返回可用模型".to_owned());
            return;
        }

        let selected_index = self
            .models
            .iter()
            .position(|model| model.model == previous_model)
            .or_else(|| self.models.iter().position(|model| model.is_default))
            .unwrap_or(0);
        let preserve_options = self.models[selected_index].model == previous_model;
        self.apply_model_selection(
            selected_index,
            preserve_options.then_some(previous_effort),
            preserve_options.then_some(previous_service_tier).flatten(),
            preserve_options,
        );
    }

    fn set_model_catalog_error(&mut self, error: String) {
        self.models.clear();
        self.model_catalog_error = Some(error);
        self.selected_model.clear();
        self.selected_effort.clear();
        self.selected_service_tier = None;
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
        self.slider_index = 0;
    }

    fn apply_model_selection(
        &mut self,
        index: usize,
        preferred_effort: Option<String>,
        preferred_service_tier: Option<String>,
        preserve_standard_tier: bool,
    ) {
        let Some(model) = self.models.get(index).cloned() else {
            return;
        };
        self.selected_model = model.model;
        self.selected_effort = preferred_effort
            .filter(|effort| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.id == *effort)
            })
            .or_else(|| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.id == model.default_reasoning_effort)
                    .then(|| model.default_reasoning_effort.clone())
            })
            .or_else(|| {
                model
                    .supported_reasoning_efforts
                    .first()
                    .map(|option| option.id.clone())
            })
            .unwrap_or_else(|| model.default_reasoning_effort.clone());

        self.selected_service_tier = if preserve_standard_tier && preferred_service_tier.is_none() {
            None
        } else {
            preferred_service_tier
                .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                .or_else(|| {
                    model
                        .default_service_tier
                        .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                })
        };
        self.slider_index = model
            .supported_reasoning_efforts
            .iter()
            .position(|option| option.id == self.selected_effort)
            .unwrap_or(0);
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }

    fn select_model_at(&mut self, index: usize) {
        self.apply_model_selection(index, None, None, false);
    }

    fn select_effort_at(&mut self, index: usize) {
        let Some(effort) = self
            .selected_model_entry()
            .and_then(|model| model.supported_reasoning_efforts.get(index))
            .map(|effort| effort.id.clone())
        else {
            return;
        };
        self.selected_effort = effort;
        self.slider_index = index;
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }

    fn select_service_tier_at(&mut self, index: usize) {
        self.selected_service_tier = if index == 0 {
            None
        } else {
            self.selected_model_entry()
                .and_then(|model| model.service_tiers.get(index - 1))
                .map(|tier| tier.id.clone())
        };
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }

    fn selected_model_entry(&self) -> Option<&AgentModel> {
        self.models
            .iter()
            .find(|model| model.model == self.selected_model)
    }

    fn model_display_name<'a>(&'a self, model_name: &'a str) -> &'a str {
        self.models
            .iter()
            .find(|model| model.model == model_name || model.id == model_name)
            .map(|model| model.display_name.as_str())
            .unwrap_or(model_name)
    }

    fn selected_model_label(&self) -> String {
        if self.selected_model.is_empty() {
            "模型不可用".to_owned()
        } else {
            self.model_display_name(&self.selected_model).to_owned()
        }
    }

    fn effective_model_label(&self) -> String {
        self.actual_model
            .as_deref()
            .map(|model| self.model_display_name(model).to_owned())
            .unwrap_or_else(|| self.selected_model_label())
    }

    fn effort_label(effort: &str) -> &str {
        match effort {
            "none" => "无",
            "minimal" => "最小",
            "low" => "轻度",
            "medium" => "中",
            "high" => "高",
            "xhigh" => "极高",
            "max" => "最高",
            "ultra" => "Ultra",
            other => other,
        }
    }

    fn effort_detail(effort: &str) -> Option<&'static str> {
        (effort == "ultra").then_some("更快消耗使用额度")
    }

    fn selected_effort_label(&self) -> String {
        if self.selected_effort.is_empty() {
            "—".to_owned()
        } else {
            Self::effort_label(&self.selected_effort).to_owned()
        }
    }

    fn selected_service_tier_label(&self) -> String {
        let Some(selected) = self.selected_service_tier.as_deref() else {
            return "标准".to_owned();
        };
        self.selected_model_entry()
            .and_then(|model| model.service_tiers.iter().find(|tier| tier.id == selected))
            .map(|tier| tier.name.clone())
            .unwrap_or_else(|| selected.to_owned())
    }

    fn default_model_index(&self) -> Option<usize> {
        self.models
            .iter()
            .position(|model| model.is_default)
            .or((!self.models.is_empty()).then_some(0))
    }

    fn selection_is_default(&self) -> bool {
        let Some(model) = self
            .default_model_index()
            .and_then(|index| self.models.get(index))
        else {
            return true;
        };
        let default_effort = model
            .supported_reasoning_efforts
            .iter()
            .find(|option| option.id == model.default_reasoning_effort)
            .or_else(|| model.supported_reasoning_efforts.first())
            .map(|option| option.id.as_str())
            .unwrap_or(model.default_reasoning_effort.as_str());
        let default_service_tier = model
            .default_service_tier
            .as_deref()
            .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier));

        self.selected_model == model.model
            && self.selected_effort == default_effort
            && self.selected_service_tier.as_deref() == default_service_tier
    }

    fn reset_model_selection(&mut self) {
        if let Some(index) = self.default_model_index() {
            self.apply_model_selection(index, None, None, false);
        }
        self.advanced_expanded = false;
        self.submenu = None;
        self.submenu_keyboard_focus = false;
        self.slider_dragging = false;
    }

    fn submenu_option_count(&self, submenu: PickerSubmenu) -> usize {
        match submenu {
            PickerSubmenu::Model => self.models.len(),
            PickerSubmenu::Effort => self
                .selected_model_entry()
                .map(|model| model.supported_reasoning_efforts.len())
                .unwrap_or(0),
            PickerSubmenu::ServiceTier => self
                .selected_model_entry()
                .map(|model| model.service_tiers.len() + 1)
                .unwrap_or(0),
        }
    }

    fn toggle_accelerated_service_tier(&mut self) {
        if self.selected_service_tier.is_some() {
            self.selected_service_tier = None;
        } else {
            self.selected_service_tier = self
                .selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone());
        }
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }

    #[cfg(test)]
    pub fn conversation_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) {
        (
            self.conversation_phase,
            self.user_message.clone(),
            self.user_message_time.clone(),
            self.assistant_message.clone(),
            self.assistant_message_time.clone(),
        )
    }

    #[cfg(test)]
    pub fn conversation_activity_snapshot(&self) -> Vec<ConversationActivity> {
        self.conversation_activity.clone()
    }

    pub fn conversation_phase(&self) -> ConversationPhase {
        self.conversation_phase
    }

    pub fn conversation_render_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Vec<ConversationActivity>,
    ) {
        // While activities are present they are the render source of truth.
        // Avoid cloning the same growing assistant response a second time on
        // every streaming frame; the aggregate is only needed for fallback
        // rendering and the completed response actions.
        let assistant_message = if self.conversation_activity.is_empty()
            || matches!(
                self.conversation_phase,
                ConversationPhase::Complete | ConversationPhase::Failed
            ) {
            self.assistant_message.clone()
        } else {
            String::new()
        };

        (
            self.conversation_phase,
            self.user_message.clone(),
            self.user_message_time.clone(),
            assistant_message,
            self.assistant_message_time.clone(),
            self.conversation_activity.clone(),
        )
    }

    fn submit_prompt(&mut self, prompt: String, cx: &mut Context<Self>) {
        if prompt.trim().is_empty()
            || matches!(
                self.conversation_phase,
                ConversationPhase::Starting
                    | ConversationPhase::Thinking
                    | ConversationPhase::Streaming
            )
        {
            return;
        }

        let selection = if self.selected_model.is_empty() || self.selected_effort.is_empty() {
            Err(self
                .model_catalog_error
                .clone()
                .unwrap_or_else(|| "没有可用的 Codex 模型".to_owned()))
        } else {
            Ok((
                self.selected_model.clone(),
                self.selected_effort.clone(),
                self.selected_service_tier.clone(),
            ))
        };

        self.user_message = Some(prompt.clone());
        self.user_message_time = Some(current_local_time_label());
        self.assistant_message.clear();
        self.conversation_activity.clear();
        self.assistant_message_time = None;
        self.conversation_phase = ConversationPhase::Starting;
        self.conversation_cycle = self.conversation_cycle.wrapping_add(1);
        let cycle = self.conversation_cycle;
        self.menu_open = false;
        self.permission_menu_open = false;
        self.submenu = None;
        self.prompt_input.update(cx, |input, cx| input.clear(cx));

        let (model, effort, service_tier) = match selection {
            Ok(selection) => selection,
            Err(error) => {
                self.assistant_message = error.clone();
                self.conversation_activity
                    .push(ConversationActivity::Error { message: error });
                self.assistant_message_time = Some(current_local_time_label());
                self.conversation_phase = ConversationPhase::Failed;
                cx.emit(ConversationChanged);
                cx.notify();
                return;
            }
        };
        self.actual_model = Some(model.clone());
        self.model_status = None;
        self.safety_buffering = false;
        cx.emit(ConversationChanged);
        cx.notify();

        let receiver = CodexAppServerBackend::new().run_prompt(AgentRequest {
            prompt,
            cwd: std::env::current_dir().unwrap_or_default(),
            model,
            effort,
            service_tier,
        });
        self.consume_agent_events(receiver, cycle, cx);
    }

    fn consume_agent_events(
        &mut self,
        receiver: async_channel::Receiver<AgentEvent>,
        cycle: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                let first_event = match receiver.recv().await {
                    Ok(event) => event,
                    Err(_) => {
                        let _ = this.update(cx, |this, cx| {
                            if this.conversation_cycle != cycle {
                                return;
                            }
                            this.apply_agent_event_batch(vec![AgentEvent::Failed(
                                STREAM_DISCONNECTED_MESSAGE.to_owned(),
                            )]);
                            cx.emit(ConversationChanged);
                            cx.notify();
                        });
                        return;
                    }
                };

                // Once the first event wakes us, leave a short collection
                // window for the rest of its protocol burst. No timer runs
                // while the channel is idle.
                cx.background_executor().timer(STREAM_UPDATE_INTERVAL).await;

                let (mut batch, channel_closed) =
                    collect_ready_agent_events(&receiver, first_event);
                if channel_closed {
                    ensure_closed_batch_is_terminal(&mut batch);
                }

                // Commit every frame's protocol burst atomically. Previously
                // each token emitted and notified independently, repeatedly
                // rebuilding the full conversation before the same paint.
                let result = this.update(cx, |this, cx| {
                    if this.conversation_cycle != cycle {
                        return true;
                    }
                    let finished = this.apply_agent_event_batch(batch);
                    cx.emit(ConversationChanged);
                    cx.notify();
                    finished
                });
                if result.unwrap_or(true) || channel_closed {
                    return;
                }
            }
        })
        .detach();
    }

    fn apply_agent_event_batch(&mut self, events: Vec<AgentEvent>) -> bool {
        let mut finished = false;
        for event in events {
            match event {
                AgentEvent::Started => {
                    self.conversation_phase = ConversationPhase::Thinking;
                }
                AgentEvent::AssistantMessageStarted { item_id } => {
                    if !self.conversation_activity.iter().any(|activity| {
                        matches!(
                            activity,
                            ConversationActivity::AssistantMessage {
                                item_id: existing,
                                ..
                            } if existing == &item_id
                        )
                    }) {
                        self.conversation_activity
                            .push(ConversationActivity::AssistantMessage {
                                item_id,
                                text: String::new(),
                            });
                    }
                }
                AgentEvent::TextDelta(delta) => {
                    self.assistant_message.push_str(&delta);
                    if let Some(ConversationActivity::AssistantMessage { text, .. }) = self
                        .conversation_activity
                        .iter_mut()
                        .rev()
                        .find(|activity| {
                            matches!(activity, ConversationActivity::AssistantMessage { .. })
                        })
                    {
                        text.push_str(&delta);
                    }
                    self.conversation_phase = ConversationPhase::Streaming;
                }
                AgentEvent::CommandStarted(command) => {
                    upsert_command_activity(&mut self.conversation_activity, command);
                    self.conversation_phase = ConversationPhase::Streaming;
                }
                AgentEvent::CommandOutputDelta { item_id, delta } => {
                    if let Some(command) =
                        find_command_activity_mut(&mut self.conversation_activity, &item_id)
                    {
                        command.output.push_str(&delta);
                    } else {
                        self.conversation_activity
                            .push(ConversationActivity::Command(CommandExecution {
                                id: item_id,
                                command: String::new(),
                                cwd: String::new(),
                                output: delta,
                                status: CommandExecutionStatus::InProgress,
                                exit_code: None,
                            }));
                    }
                    self.conversation_phase = ConversationPhase::Streaming;
                }
                AgentEvent::CommandCompleted(command) => {
                    upsert_command_activity(&mut self.conversation_activity, command);
                    self.conversation_phase = ConversationPhase::Streaming;
                }
                AgentEvent::ModelRerouted {
                    from_model,
                    to_model,
                    reason,
                } => {
                    self.actual_model = Some(to_model.clone());
                    self.model_status = Some(format!(
                        "已从 {} 自动切换到 {}（{}）",
                        self.model_display_name(&from_model),
                        self.model_display_name(&to_model),
                        reason
                    ));
                    self.safety_buffering = false;
                }
                AgentEvent::ModelVerificationRequired { verifications } => {
                    let requirements = if verifications.is_empty() {
                        "未知验证".to_owned()
                    } else {
                        verifications.join("、")
                    };
                    let error = format!("所选模型需要额外账户验证：{requirements}");
                    self.assistant_message = error.clone();
                    self.conversation_activity
                        .push(ConversationActivity::Error { message: error });
                    self.assistant_message_time = Some(current_local_time_label());
                    self.conversation_phase = ConversationPhase::Failed;
                    self.model_status = Some("需要账户验证".to_owned());
                    self.safety_buffering = false;
                    finished = true;
                    break;
                }
                AgentEvent::ModelSafetyBufferingUpdated {
                    model,
                    use_cases,
                    reasons,
                    show_buffering_ui,
                    faster_model,
                } => {
                    self.actual_model = Some(model);
                    self.safety_buffering = show_buffering_ui;
                    self.model_status = show_buffering_ui.then(|| {
                        let mut message = "安全检查中".to_owned();
                        if !use_cases.is_empty() || !reasons.is_empty() {
                            let detail = use_cases
                                .into_iter()
                                .chain(reasons)
                                .collect::<Vec<_>>()
                                .join("、");
                            message.push_str(&format!("：{detail}"));
                        }
                        if let Some(faster_model) = faster_model {
                            message.push_str(&format!(
                                "；可改用 {}",
                                self.model_display_name(&faster_model)
                            ));
                        }
                        message
                    });
                }
                AgentEvent::Completed => {
                    if self.safety_buffering {
                        self.model_status = None;
                        self.safety_buffering = false;
                    }
                    self.assistant_message_time = Some(current_local_time_label());
                    self.conversation_phase = ConversationPhase::Complete;
                    finished = true;
                    break;
                }
                AgentEvent::Failed(error) => {
                    self.assistant_message = error.clone();
                    self.conversation_activity
                        .push(ConversationActivity::Error { message: error });
                    self.assistant_message_time = Some(current_local_time_label());
                    self.conversation_phase = ConversationPhase::Failed;
                    self.safety_buffering = false;
                    finished = true;
                    break;
                }
            }
        }
        finished
    }

    fn stop_generation(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.conversation_phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
        ) {
            self.conversation_cycle = self.conversation_cycle.wrapping_add(1);
            self.assistant_message_time = Some(current_local_time_label());
            self.conversation_phase = ConversationPhase::Complete;
            cx.emit(ConversationChanged);
            cx.notify();
        }
    }

    fn handle_model_menu_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu_open {
            return;
        }
        let key = event.keystroke.key.as_str();
        if let Some(submenu) = self.submenu {
            let count = self.submenu_option_count(submenu);
            if count == 0 {
                if matches!(key, "left" | "escape") {
                    self.submenu = None;
                    self.submenu_keyboard_focus = false;
                    cx.stop_propagation();
                    cx.notify();
                }
                return;
            }
            match key {
                "down" => {
                    self.submenu_focused_item = if self.submenu_keyboard_focus {
                        (self.submenu_focused_item + 1) % count
                    } else {
                        0
                    };
                    self.submenu_keyboard_focus = true;
                }
                "up" => {
                    self.submenu_focused_item = if self.submenu_keyboard_focus {
                        (self.submenu_focused_item + count - 1) % count
                    } else {
                        count - 1
                    };
                    self.submenu_keyboard_focus = true;
                }
                "home" => {
                    self.submenu_focused_item = 0;
                    self.submenu_keyboard_focus = true;
                }
                "end" => {
                    self.submenu_focused_item = count - 1;
                    self.submenu_keyboard_focus = true;
                }
                "left" | "escape" => {
                    self.submenu = None;
                    self.submenu_keyboard_focus = false;
                }
                "enter" | "space" => {
                    if !self.submenu_keyboard_focus {
                        return;
                    }
                    match submenu {
                        PickerSubmenu::Model => self.select_model_at(self.submenu_focused_item),
                        PickerSubmenu::Effort => self.select_effort_at(self.submenu_focused_item),
                        PickerSubmenu::ServiceTier => {
                            self.select_service_tier_at(self.submenu_focused_item)
                        }
                    }
                    self.menu_open = false;
                    self.submenu = None;
                }
                "tab" => return,
                _ => return,
            }
        } else {
            match key {
                "down" => {
                    self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                        (self.model_menu_focused_item + 1) % 4
                    } else {
                        0
                    };
                    self.model_menu_keyboard_focus = true;
                }
                "up" => {
                    self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                        (self.model_menu_focused_item + 3) % 4
                    } else {
                        3
                    };
                    self.model_menu_keyboard_focus = true;
                }
                "home" => {
                    self.model_menu_focused_item = 0;
                    self.model_menu_keyboard_focus = true;
                }
                "end" => {
                    self.model_menu_focused_item = 3;
                    self.model_menu_keyboard_focus = true;
                }
                "right" | "enter" | "space" if self.model_menu_focused_item < 3 => {
                    self.submenu = Some(match self.model_menu_focused_item {
                        0 => PickerSubmenu::Model,
                        1 => PickerSubmenu::Effort,
                        _ => PickerSubmenu::ServiceTier,
                    });
                    self.submenu_keyboard_focus = false;
                }
                "enter" | "space" => {
                    if self.selection_is_default() {
                        self.advanced_expanded = !self.advanced_expanded;
                    } else {
                        self.reset_model_selection();
                    }
                }
                "escape" | "tab" => {
                    self.menu_open = false;
                    self.submenu = None;
                }
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.prompt_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            self.submenu = None;
            cx.notify();
        }
        if self.permission_menu_open {
            self.permission_menu_open = false;
            cx.notify();
        }
    }

    pub fn set_permission_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.permission_mode = match mode {
            "request" => PermissionMode::Request,
            "assist" => PermissionMode::Assist,
            "custom" => PermissionMode::Custom,
            _ => PermissionMode::Full,
        };
        self.permission_menu_open = false;
        cx.notify();
    }

    pub fn open_permission_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.submenu = None;
        self.permission_menu_open = true;
        cx.notify();
    }

    pub fn open_picker(&mut self, cx: &mut Context<Self>) {
        self.menu_open = true;
        cx.notify();
    }

    pub fn open_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.advanced_expanded = name != "simple";
        self.submenu = match name {
            "model" => Some(PickerSubmenu::Model),
            "effort" => Some(PickerSubmenu::Effort),
            "speed" | "service-tier" => Some(PickerSubmenu::ServiceTier),
            _ => None,
        };
        cx.notify();
    }

    pub fn open_picker_slider_at(&mut self, index: usize, fast: bool, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.advanced_expanded = false;
        self.submenu = None;
        self.selected_service_tier = if fast {
            self.selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone())
        } else {
            None
        };
        self.set_slider_index(index);
        cx.notify();
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.dictation_state = match state {
            "recording" => DictationState::Recording,
            "transcribing" => DictationState::Transcribing,
            _ => DictationState::Idle,
        };
        cx.notify();
    }

    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        self.submit_prompt(prompt.to_owned(), cx);
    }

    pub fn set_command_tool_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        let item_id = "exec-command-ui-capture".to_owned();
        let command = "printf 'SHELLPIXEL20260830\\n'".to_owned();
        self.user_message = Some(
            "请使用终端执行 printf 'SHELLPIXEL20260830\\n'，等待命令执行完成后告诉我输出。"
                .to_owned(),
        );
        self.user_message_time = Some("21:45".to_owned());
        self.assistant_message = if running {
            "我现在执行这条命令，完成后原样告诉你输出。".to_owned()
        } else {
            "我现在执行这条命令，完成后原样告诉你输出。输出为：SHELLPIXEL20260830".to_owned()
        };
        self.assistant_message_time = (!running).then(|| "21:45".to_owned());
        self.conversation_phase = if running {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation_activity = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-command-preamble".to_owned(),
                text: "我现在执行这条命令，完成后原样告诉你输出。".to_owned(),
            },
            ConversationActivity::Command(CommandExecution {
                id: item_id,
                command,
                cwd: "/path/to/project".to_owned(),
                output: "SHELLPIXEL20260830\n".to_owned(),
                status: if running {
                    CommandExecutionStatus::InProgress
                } else {
                    CommandExecutionStatus::Completed
                },
                exit_code: (!running).then_some(0),
            }),
        ];
        if !running {
            self.conversation_activity
                .push(ConversationActivity::AssistantMessage {
                    item_id: "msg-command-final".to_owned(),
                    text: "输出为：\n\nSHELLPIXEL20260830".to_owned(),
                });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    #[cfg(test)]
    fn dictation_state_name(&self) -> &'static str {
        match self.dictation_state {
            DictationState::Idle => "idle",
            DictationState::Recording => "recording",
            DictationState::Transcribing => "transcribing",
        }
    }

    #[cfg(test)]
    fn permission_mode_name(&self) -> &'static str {
        match self.permission_mode {
            PermissionMode::Request => "request",
            PermissionMode::Assist => "assist",
            PermissionMode::Full => "full",
            PermissionMode::Custom => "custom",
        }
    }

    fn set_slider_index(&mut self, index: usize) {
        let effort_count = self
            .selected_model_entry()
            .map(|model| model.supported_reasoning_efforts.len())
            .unwrap_or(0);
        if effort_count == 0 {
            self.slider_index = 0;
            return;
        }
        self.select_effort_at(index.min(effort_count - 1));
    }

    fn start_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Recording;
        self.menu_open = false;
        self.submenu = None;
        cx.notify();
    }

    fn cancel_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Idle;
        cx.notify();
    }

    fn stop_dictation(&mut self, cx: &mut Context<Self>) {
        if self.dictation_state != DictationState::Recording {
            return;
        }
        self.dictation_state = DictationState::Transcribing;
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        let cycle = self.dictation_cycle;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // CDP sampling showed the processing controls for roughly one second
            // before the ordinary composer footer returned.
            executor.timer(Duration::from_millis(1_050)).await;
            let _ = this.update(cx, |this, cx| {
                if this.dictation_state == DictationState::Transcribing
                    && this.dictation_cycle == cycle
                {
                    this.dictation_state = DictationState::Idle;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

fn find_command_activity_mut<'a>(
    activities: &'a mut [ConversationActivity],
    item_id: &str,
) -> Option<&'a mut CommandExecution> {
    activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::Command(command) if command.id == item_id => Some(command),
        _ => None,
    })
}

fn upsert_command_activity(
    activities: &mut Vec<ConversationActivity>,
    mut incoming: CommandExecution,
) {
    if let Some(existing) = find_command_activity_mut(activities, &incoming.id) {
        if incoming.output.is_empty() {
            incoming.output = std::mem::take(&mut existing.output);
        }
        *existing = incoming;
    } else {
        activities.push(ConversationActivity::Command(incoming));
    }
}

fn utility(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    horizontal_padding: f32,
    theme: Theme,
) -> impl IntoElement {
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id(id)
        .h(px(28.0))
        .px(px(horizontal_padding))
        .rounded_full()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .child(icon(glyph, theme.text.into()).size(px(16.0)))
        .child(label)
}

fn project_utility(theme: Theme) -> impl IntoElement {
    let group = "composer-project-hover";
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id("composer-project")
        .group(group)
        .relative()
        .h(px(28.0))
        .rounded_full()
        .flex_none()
        .child(
            div()
                .h(px(28.0))
                .px(px(8.0))
                .rounded_full()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(gpui::FontWeight::NORMAL)
                .text_color(theme.text)
                .cursor_pointer()
                .group_hover(group, move |style| style.bg(hover_fill))
                .child(
                    icon("utility-folder", theme.text.into())
                        .size(px(16.0))
                        .group_hover(group, |style| style.invisible()),
                )
                .child("coda"),
        )
        // The reference overlays a 28px clear-project control on the leading
        // edge and swaps it with the folder whenever the project trigger is
        // hovered. Only this nested control promotes tertiary -> primary.
        .child(
            div()
                .id("composer-clear-project")
                .absolute()
                .top_0()
                .left_0()
                .size(px(28.0))
                .group("composer-clear-project-icon")
                .invisible()
                .group_hover(group, |style| style.visible())
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.text_tertiary)
                .cursor_pointer()
                .hover(move |style| style.bg(hover_fill).text_color(theme.text))
                .child(
                    icon("clear-project", theme.text_tertiary.into())
                        .size(px(16.0))
                        .group_hover("composer-clear-project-icon", move |style| {
                            style.text_color(theme.text)
                        }),
                ),
        )
}

fn context_toolbar(theme: Theme) -> Div {
    div()
        .relative()
        .h(px(38.0))
        .mx(px(13.0))
        // The reference toolbar continues underneath the Composer. Keeping
        // the extension in a separate background layer preserves the text's
        // 38px alignment while the later Composer layer masks its lower edge.
        .child(
            div()
                .absolute()
                .top(px(4.0))
                .left_0()
                .w_full()
                .h(px(52.0))
                .rounded(px(16.0))
                .bg(theme.surface_under),
        )
        .child(
            div()
                .relative()
                .top(px(4.0))
                .h(px(38.0))
                .w_full()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(project_utility(theme))
                .child(utility("composer-location", "本地", "local", 8.0, theme))
                // Although the HTML includes a trailing `px-0` class, the
                // home-placement `px-2` rule is emitted later in the bundled
                // stylesheet and wins the cascade. The computed button has
                // 8px inline padding on both sides.
                .child(utility("composer-branch", "main", "branch", 8.0, theme)),
        )
}

impl ComposerView {
    fn picker_row(
        &self,
        index: usize,
        id: &'static str,
        label: &str,
        value: &str,
        submenu: PickerSubmenu,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.submenu == Some(submenu);
        let focused = self.model_menu_keyboard_focus && self.model_menu_focused_item == index;
        div()
            .id(id)
            .h(px(28.5625))
            .px(px(8.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(18.5625))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .cursor_pointer()
            .when(selected || focused, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.submenu = if this.submenu == Some(submenu) {
                    None
                } else {
                    Some(submenu)
                };
                cx.notify();
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.submenu != Some(submenu) {
                    this.submenu = Some(submenu);
                    cx.notify();
                }
            }))
            .child(div().flex_1().child(label.to_owned()))
            .child(
                div()
                    .text_color(theme.text_tertiary)
                    .child(value.to_owned()),
            )
            .child(
                icon("chevron-down", theme.text_tertiary.into())
                    .size(px(16.0))
                    .ml(px(12.0))
                    .with_transformation(Transformation::rotate(radians(
                        -std::f32::consts::FRAC_PI_2,
                    ))),
            )
    }

    fn option_row(
        &self,
        id: (&'static str, usize),
        title: &str,
        detail: Option<&str>,
        truncate_detail: bool,
        selected: bool,
        focused: bool,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .min_h(px(if detail.is_some() {
                MODEL_PICKER_DETAIL_ROW_HEIGHT
            } else {
                MODEL_PICKER_ROW_HEIGHT
            }))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .when(focused, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .text_color(theme.text)
                    .child(title.to_owned())
                    .when_some(detail, |column, detail| {
                        column.child(
                            div()
                                .min_w(px(0.0))
                                .w_full()
                                .text_size(px(12.0))
                                .line_height(px(18.5625))
                                .text_color(theme.text_tertiary)
                                .when(truncate_detail, |detail| detail.truncate())
                                .child(detail.to_owned()),
                        )
                    }),
            )
            .when(selected, |row| {
                row.child(
                    icon("check", theme.text.into())
                        .size(px(17.0))
                        .flex_none()
                        .opacity(0.75),
                )
            })
    }

    fn submenu(
        &self,
        kind: PickerSubmenu,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let width = match kind {
            PickerSubmenu::Model => 280.0,
            PickerSubmenu::Effort => 180.0,
            PickerSubmenu::ServiceTier => 233.0,
        };
        let estimated_height = match kind {
            PickerSubmenu::Model => {
                self.models.len().max(1) as f32 * MODEL_PICKER_ROW_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
            PickerSubmenu::Effort => {
                self.selected_model_entry()
                    .map(|model| {
                        model
                            .supported_reasoning_efforts
                            .iter()
                            .map(|option| {
                                if option.id == "ultra" {
                                    MODEL_PICKER_DETAIL_ROW_HEIGHT
                                } else {
                                    MODEL_PICKER_ROW_HEIGHT
                                }
                            })
                            .sum::<f32>()
                    })
                    .unwrap_or(MODEL_PICKER_ROW_HEIGHT)
                    + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
            PickerSubmenu::ServiceTier => {
                self.submenu_option_count(kind).max(1) as f32 * MODEL_PICKER_DETAIL_ROW_HEIGHT
                    + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
        }
        .min(MODEL_PICKER_SUBMENU_MAX_HEIGHT);
        let top = MODEL_PICKER_SUBMENU_BOTTOM_OFFSET - estimated_height;
        let layout = submenu_layout(viewport_width, width);
        let mut menu = div()
            .id("model-picker-submenu")
            .absolute()
            .top(px(top))
            .w(px(layout.width))
            .max_h(px(MODEL_PICKER_SUBMENU_MAX_HEIGHT))
            .overflow_y_scroll()
            .when(layout.open_left, |menu| {
                menu.right(px(MODEL_PICKER_WIDTH + MODEL_PICKER_SUBMENU_GAP))
            })
            .when(!layout.open_left, |menu| {
                menu.left(px(MODEL_PICKER_WIDTH + MODEL_PICKER_SUBMENU_GAP))
            })
            .p(px(4.0))
            .rounded(px(15.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));

        match kind {
            PickerSubmenu::Model => {
                if self.models.is_empty() {
                    let message = self
                        .model_catalog_error
                        .clone()
                        .unwrap_or_else(|| "没有可用模型".to_owned());
                    menu = menu.child(self.option_row(
                        ("model-option", 0),
                        &message,
                        None,
                        false,
                        false,
                        false,
                        theme,
                    ));
                }
                for (index, model) in self.models.iter().cloned().enumerate() {
                    let model_name = model.model.clone();
                    menu = menu.child(
                        self.option_row(
                            ("model-option", index),
                            &model.display_name,
                            None,
                            false,
                            self.selected_model == model_name,
                            self.submenu_keyboard_focus && self.submenu_focused_item == index,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.models.iter().position(|model| {
                                model.model == model_name || model.id == model_name
                            }) {
                                this.select_model_at(index);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::Effort => {
                menu = menu.child(
                    div()
                        .h(px(26.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .text_color(theme.text_tertiary)
                        .child("推理强度"),
                );
                let options = self
                    .selected_model_entry()
                    .map(|model| model.supported_reasoning_efforts.clone())
                    .unwrap_or_default();
                for (index, option) in options.into_iter().enumerate() {
                    let effort_id = option.id.clone();
                    let title = Self::effort_label(&effort_id).to_owned();
                    let detail = Self::effort_detail(&effort_id);
                    menu = menu.child(
                        self.option_row(
                            ("effort-option", index),
                            &title,
                            detail,
                            true,
                            self.selected_effort == effort_id,
                            self.submenu_keyboard_focus && self.submenu_focused_item == index,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.selected_model_entry().and_then(|model| {
                                model
                                    .supported_reasoning_efforts
                                    .iter()
                                    .position(|option| option.id == effort_id)
                            }) {
                                this.select_effort_at(index);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::ServiceTier => {
                menu = menu.child(
                    div()
                        .h(px(26.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .text_color(theme.text_tertiary)
                        .child("速度"),
                );
                let service_tiers = self
                    .selected_model_entry()
                    .map(|model| model.service_tiers.clone())
                    .unwrap_or_default();
                menu = menu.child(
                    self.option_row(
                        ("service-tier-option", 0),
                        "标准",
                        Some("默认速度"),
                        false,
                        self.selected_service_tier.is_none(),
                        self.submenu_keyboard_focus && self.submenu_focused_item == 0,
                        theme,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.select_service_tier_at(0);
                        this.menu_open = false;
                        this.submenu = None;
                        cx.notify();
                    })),
                );
                for (offset, tier) in service_tiers.into_iter().enumerate() {
                    let index = offset + 1;
                    let tier_id = tier.id.clone();
                    menu = menu.child(
                        self.option_row(
                            ("service-tier-option", index),
                            &tier.name,
                            Some(&tier.description),
                            false,
                            self.selected_service_tier.as_deref() == Some(tier_id.as_str()),
                            self.submenu_keyboard_focus && self.submenu_focused_item == index,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.selected_model_entry().and_then(|model| {
                                model
                                    .service_tiers
                                    .iter()
                                    .position(|tier| tier.id == tier_id)
                            }) {
                                this.select_service_tier_at(index + 1);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
        }
        menu
    }

    fn view_controls(&self, show_fast_toggle: bool, theme: Theme, cx: &mut Context<Self>) -> Div {
        let focused = self.model_menu_keyboard_focus && self.model_menu_focused_item == 3;
        let mut controls = div().h(px(32.0)).flex().items_center();

        if self.selection_is_default() {
            controls = controls
                .child(
                    div()
                        .id("model-picker-advanced")
                        .h(px(32.0))
                        .w(px(58.0))
                        .p(px(4.0))
                        .rounded(px(8.0))
                        .flex()
                        .items_center()
                        .text_color(theme.text_tertiary)
                        .cursor_pointer()
                        .when(focused, |row| row.bg(theme.sidebar_hover))
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.advanced_expanded = !this.advanced_expanded;
                            this.submenu = None;
                            this.slider_dragging = false;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .px(px(4.0))
                                .py(px(2.0))
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child("高级")
                                .child(
                                    icon("chevron-down", theme.text_tertiary.into())
                                        .size(px(12.0))
                                        .with_transformation(Transformation::rotate(radians(
                                            if self.advanced_expanded {
                                                std::f32::consts::PI
                                            } else {
                                                0.0
                                            },
                                        ))),
                                ),
                        ),
                )
                .child(div().flex_1());
        } else {
            controls = controls.child(
                div()
                    .id("model-picker-reset")
                    .h(px(28.0))
                    .when(show_fast_toggle, |row| row.flex_1())
                    .when(!show_fast_toggle, |row| row.w_full())
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .text_color(theme.text_tertiary)
                    .cursor_pointer()
                    .when(focused, |row| row.bg(theme.sidebar_hover))
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.reset_model_selection();
                        cx.notify();
                    }))
                    .child(div().flex_1().child("重置为默认设置"))
                    .child(icon("model-reset", theme.text_tertiary.into()).size(px(14.0))),
            );
        }

        if show_fast_toggle {
            controls.child(
                div()
                    .id("model-picker-fast-toggle")
                    .size(px(32.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_accelerated_service_tier();
                        cx.notify();
                    }))
                    .child(
                        icon(
                            "model-fast",
                            if self.selected_service_tier.is_some() {
                                if self.selected_effort == "ultra" {
                                    rgba(0xad7bf9ff).into()
                                } else {
                                    rgba(0x339cffff).into()
                                }
                            } else {
                                theme.text_tertiary.into()
                            },
                        )
                        .size(px(16.0)),
                    ),
            )
        } else {
            controls
        }
    }

    fn power_slider(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        const TRACK_WIDTH: f32 = 200.0;
        const TRACK_INSET: f32 = 13.0;
        let effort_count = self
            .selected_model_entry()
            .map(|model| model.supported_reasoning_efforts.len())
            .unwrap_or(0)
            .max(1);
        let step = if effort_count > 1 {
            (TRACK_WIDTH - TRACK_INSET * 2.0) / (effort_count - 1) as f32
        } else {
            0.0
        };
        let step_center = |index: usize| {
            if effort_count > 1 {
                TRACK_INSET + step * index as f32
            } else {
                TRACK_WIDTH * 0.5
            }
        };
        let slider_index = self.slider_index.min(effort_count - 1);
        let thumb_center = step_center(slider_index);
        let ultra_mode = self.selected_effort == "ultra";
        let thumb_size = if self.slider_dragging { 32.0 } else { 28.0 };
        let mut range = div()
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .w(px(thumb_center))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(rgba(0x339cffff));

        if ultra_mode {
            // Keep the first radius of the rounded range as solid blue. Starting the
            // rectangular gradient at the circle tangent prevents its square corners
            // from leaking through the rounded left cap.
            const RANGE_RADIUS: f32 = 12.0;
            range = range.child(
                div()
                    .absolute()
                    .left(px(RANGE_RADIUS))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .child(div().h_full().w(px(TRACK_WIDTH * 0.55 - RANGE_RADIUS)).bg(
                        linear_gradient(
                            90.0,
                            linear_color_stop(rgba(0x339cffff), 0.0),
                            linear_color_stop(rgba(0xad7bf9ff), 1.0),
                        ),
                    ))
                    .child(div().h_full().flex_1().bg(linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xad7bf9ff), 0.0),
                        linear_color_stop(rgba(0x8b84fbff), 1.0),
                    ))),
            );
        }

        let (show_max_particles, show_fast_particles) =
            particle_layers(ultra_mode, self.selected_service_tier.is_some());
        if show_max_particles || show_fast_particles {
            const MAX_PARTICLES: [(f32, f32, f32, f32, f32, u64); 14] = [
                (0.50, 3.0, 17.0, 0.616, 0.405, 3102),
                (0.87, 3.0, 17.0, 0.740, 0.528, 2843),
                (0.72, 0.0, 17.0, 0.946, 0.837, 1352),
                (0.60, -3.0, 8.0, 0.595, 0.898, 1607),
                (0.48, -4.0, 15.0, 0.627, 0.993, 1738),
                (0.56, -1.0, 11.0, 0.617, 0.718, 1967),
                (0.18, -3.0, 12.0, 0.522, 0.788, 2066),
                (0.42, -2.0, 11.0, 0.682, 0.992, 1660),
                (0.29, -1.0, 12.0, 0.580, 0.755, 2481),
                (0.49, 0.0, 13.0, 0.564, 0.701, 1577),
                (0.90, -3.0, 12.0, 0.852, 0.614, 2675),
                (0.08, -3.0, 5.0, 0.523, 0.892, 2156),
                (0.13, -3.0, 14.0, 0.759, 0.773, 1553),
                (0.08, 2.0, 14.0, 0.563, 0.880, 1863),
            ];
            const FAST_PARTICLES: [(f32, f32, f32, u64, f32); 14] = [
                (8.94, 0.616, 0.405, 1701, 0.20),
                (16.65, 0.740, 0.528, 1708, 0.12),
                (20.50, 0.946, 0.837, 2259, 0.08),
                (7.68, 0.595, 0.898, 1928, 0.99),
                (10.22, 0.627, 0.993, 1843, 0.91),
                (19.24, 0.617, 0.718, 1629, 0.78),
                (6.07, 0.522, 0.788, 1963, 0.68),
                (15.18, 0.682, 0.992, 1851, 0.57),
                (10.92, 0.580, 0.755, 1745, 0.46),
                (3.44, 0.564, 0.701, 2014, 0.39),
                (18.27, 0.852, 0.614, 1843, 0.31),
                (13.25, 0.523, 0.892, 2292, 0.24),
                (20.65, 0.759, 0.773, 1982, 0.16),
                (14.98, 0.563, 0.880, 1809, 0.08),
            ];

            // One phase-locked clock drives the complete particle layer. Each particle
            // moves at canvas paint time, so animation frames neither fan out into 14/28
            // independent timers nor invalidate their layout positions.
            range = range.child(div().absolute().inset_0().with_animation(
                "model-slider-particles",
                Animation::new(Duration::from_secs(120)).repeat_synced(),
                move |layer, progress| {
                    layer.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                let mut paint_particle =
                                    |x: f32, y: f32, diameter: f32, opacity: f32| {
                                        let particle_bounds = gpui::Bounds {
                                            origin: gpui::point(
                                                bounds.origin.x + px(x),
                                                bounds.origin.y + px(y),
                                            ),
                                            size: gpui::size(px(diameter), px(diameter)),
                                        };
                                        let radius = px(diameter / 2.0);
                                        window.paint_drop_shadows(
                                            particle_bounds,
                                            radius.into(),
                                            &[BoxShadow::new(
                                                px(0.0),
                                                px(0.0),
                                                hsla(0.0, 0.0, 1.0, 0.34 * opacity),
                                            )
                                            .blur_radius(px(5.0))],
                                        );
                                        window.paint_quad(gpui::quad(
                                            particle_bounds,
                                            radius,
                                            hsla(0.0, 0.0, 1.0, 0.72 * opacity),
                                            px(0.0),
                                            hsla(0.0, 0.0, 1.0, 0.0),
                                            Default::default(),
                                        ));
                                    };

                                if show_max_particles {
                                    for (index, (position, offset, y, scale, opacity, duration)) in
                                        MAX_PARTICLES.into_iter().enumerate()
                                    {
                                        let (drift_x, drift_y) =
                                            max_particle_drift(progress, index, duration);
                                        paint_particle(
                                            position * TRACK_WIDTH + offset + drift_x,
                                            (y + drift_y).clamp(4.0, 20.0),
                                            3.0 * scale,
                                            opacity,
                                        );
                                    }
                                }

                                if show_fast_particles {
                                    for (y, scale, base_opacity, duration, phase) in FAST_PARTICLES
                                    {
                                        let loops =
                                            (PARTICLE_TIMELINE_MS / duration as f32).round();
                                        let travel = (progress * loops + phase).fract();
                                        let opacity = if travel < 0.08 {
                                            travel / 0.08
                                        } else if travel > 0.92 {
                                            (1.0 - travel) / 0.08
                                        } else {
                                            1.0
                                        };
                                        paint_particle(
                                            (1.0 - travel) * thumb_center,
                                            y,
                                            3.0 * scale,
                                            opacity * base_opacity,
                                        );
                                    }
                                }
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                },
            ));
        }

        let mut track = div()
            .absolute()
            .left_0()
            .top(px(2.0))
            .w(px(TRACK_WIDTH))
            .h(px(24.0))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(theme.text.alpha(0.10))
            .border(px(0.5))
            .border_color(theme.border)
            .child(range);

        for index in 0..effort_count {
            let center = step_center(index);
            let selected = index <= slider_index;
            let hidden = ultra_mode || (self.selected_service_tier.is_some() && selected);
            track = track.child(
                div()
                    .absolute()
                    .left(px(center - if hidden { 1.5 } else { 2.0 }))
                    .top(px(if hidden { 10.5 } else { 10.0 }))
                    .size(px(if hidden { 3.0 } else { 4.0 }))
                    .rounded_full()
                    .bg(if selected {
                        rgba(0xffffff4d)
                    } else {
                        rgba(0xffffff40)
                    })
                    .opacity(if hidden { 0.0 } else { 1.0 }),
            );
        }

        let mut hit_areas = div().absolute().inset_0();
        for index in 0..effort_count {
            let center = step_center(index);
            let left = if index == 0 { 0.0 } else { center - step * 0.5 };
            let right = if index + 1 == effort_count {
                TRACK_WIDTH
            } else {
                center + step * 0.5
            };
            hit_areas = hit_areas.child(
                div()
                    .id(("model-power-slider-step", index))
                    .absolute()
                    .left(px(left))
                    .top_0()
                    .w(px(right - left))
                    .h_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.slider_dragging = true;
                            this.set_slider_index(index);
                            cx.notify();
                        }),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.slider_dragging && this.slider_index != index {
                            this.set_slider_index(index);
                            cx.notify();
                        }
                    })),
            );
        }

        div()
            .id("model-power-slider")
            .relative()
            .h(px(32.0))
            .mx(px(2.0))
            .px(px(6.0))
            .py(px(2.0))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .w(px(TRACK_WIDTH))
                    .h(px(28.0))
                    .child(track)
                    .child(
                        div()
                            .absolute()
                            .left(px(thumb_center - thumb_size * 0.5))
                            .top(px((28.0 - thumb_size) * 0.5))
                            .size(px(thumb_size))
                            .rounded_full()
                            .bg(rgba(0xffffffff))
                            .border(px(0.5))
                            .border_color(rgba(0xffffff28))
                            .shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.10))
                                    .blur_radius(px(2.0)),
                            ]),
                    )
                    .child(hit_areas),
            )
    }

    fn model_menu(
        &self,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected_model_label = self.selected_model_label();
        let selected_effort_label = self.selected_effort_label();
        let selected_service_tier_label = self.selected_service_tier_label();
        let selection_is_default = self.selection_is_default();
        let mut menu = div()
            .id("model-picker-menu")
            .track_focus(&self.model_menu_focus)
            .on_key_down(cx.listener(Self::handle_model_menu_key))
            .absolute()
            .right(px(MODEL_PICKER_RIGHT_INSET))
            .bottom(px(46.0))
            .w(px(MODEL_PICKER_WIDTH))
            .p(px(4.0))
            .rounded(px(15.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));

        if self.advanced_expanded {
            menu = menu
                .child(self.picker_row(
                    0,
                    "model-picker-model-row",
                    "模型",
                    &selected_model_label,
                    PickerSubmenu::Model,
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    1,
                    "model-picker-effort-row",
                    "推理强度",
                    &selected_effort_label,
                    PickerSubmenu::Effort,
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    2,
                    "model-picker-service-tier-row",
                    "速度",
                    &selected_service_tier_label,
                    PickerSubmenu::ServiceTier,
                    theme,
                    cx,
                ))
                .child(if selection_is_default {
                    div()
                        .h(px(8.0))
                        .px(px(8.0))
                        .py(px(3.5))
                        .child(div().h(px(1.0)).w_full().bg(theme.border))
                } else {
                    div().h(px(4.0))
                })
                .child(self.view_controls(false, theme, cx));
        } else {
            menu = menu
                .child(self.view_controls(true, theme, cx))
                .child(div().h(px(4.0)))
                .child(self.power_slider(theme, cx))
                .child(div().h(px(8.0)));
        }

        if let Some(submenu) = self.submenu {
            menu = menu.child(deferred(self.submenu(submenu, viewport_width, theme, cx)));
        }
        menu
    }

    fn dictation_waveform(&self, theme: Theme) -> impl IntoElement {
        div()
            .id("composer-dictation-waveform")
            .h(px(28.0))
            .min_w(px(0.0))
            .flex_1()
            .with_animation(
                "composer-dictation-waveform-motion",
                Animation::new(Duration::from_secs(36)).repeat(),
                move |waveform, progress| {
                    waveform.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                // The desktop app paints a 2px pill every 6px on a
                                // transparent 28px canvas. Quiet samples are 2px tall at
                                // 20% text opacity; voiced samples grow to 22px and 55%.
                                let width = f32::from(bounds.size.width).max(0.0);
                                let count = (width / 6.0).ceil() as usize;
                                let phase = progress * 610.0;
                                for index in 0..count {
                                    let sample = index as f32 + phase;
                                    let envelope = ((sample * 0.097).sin().abs().powf(22.0)
                                        * (0.45 + 0.55 * (sample * 0.271).sin().abs()))
                                    .max(
                                        (sample * 0.043 + 1.7).sin().abs().powf(34.0)
                                            * (sample * 0.191).sin().abs(),
                                    );
                                    let height = 2.0 + 20.0 * envelope;
                                    let x = index as f32 * 6.0 + 1.0;
                                    if x >= width {
                                        break;
                                    }
                                    let bar = gpui::Bounds {
                                        origin: gpui::point(
                                            bounds.origin.x + px(x),
                                            bounds.origin.y + px((28.0 - height) * 0.5),
                                        ),
                                        size: gpui::size(px(2.0), px(height)),
                                    };
                                    window.paint_quad(gpui::quad(
                                        bar,
                                        px(1.0),
                                        theme.text.alpha(0.20 + 0.35 * envelope),
                                        px(0.0),
                                        rgba(0x00000000),
                                        Default::default(),
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    )
                },
            )
    }

    fn dictation_footer(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let transcribing = self.dictation_state == DictationState::Transcribing;
        let soft_fill = theme.text.alpha(0.05);
        let strong_fill = theme.text.alpha(0.10);

        let cancel = div()
            .id("composer-dictation-cancel")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(soft_fill)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(strong_fill))
            .on_click(cx.listener(|this, _, _, cx| this.cancel_dictation(cx)))
            .child(icon("dictation-cancel", theme.text.into()).size(px(16.0)));

        let stop_icon = if transcribing {
            icon("dictation-spinner", theme.text.into())
                .size(px(16.0))
                .with_animation(
                    "composer-dictation-spinner-motion",
                    Animation::new(Duration::from_millis(800)).repeat(),
                    |spinner, progress| {
                        spinner.with_transformation(Transformation::rotate(radians(
                            progress * std::f32::consts::TAU,
                        )))
                    },
                )
                .into_any_element()
        } else {
            icon("dictation-stop", theme.text.into())
                .size(px(16.0))
                .into_any_element()
        };

        let stop = div()
            .id("composer-dictation-stop")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(if transcribing { soft_fill } else { strong_fill })
            .flex()
            .items_center()
            .justify_center()
            .when(!transcribing, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(strong_fill))
                    .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            })
            .child(stop_icon);

        let send = div()
            .id("composer-dictation-send")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(theme.button)
            .when(transcribing, |button| button.opacity(0.4))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            .child(icon("dictation-send", theme.button_text.into()).size(px(16.0)));

        div()
            .id("composer-dictation-footer")
            .h(px(36.0))
            .relative()
            .top(px(7.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(cancel)
            .child(self.dictation_waveform(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(stop)
                    .child(send),
            )
    }

    fn permission_label(&self) -> (&'static str, &'static str, gpui::Rgba) {
        let theme = Theme::for_mode(self.mode);
        match self.permission_mode {
            PermissionMode::Request => ("请求批准", "permission-request", theme.text_tertiary),
            PermissionMode::Assist => ("帮我批准", "permission-assist", theme.text_tertiary),
            PermissionMode::Full => ("完全访问", "permission", theme.warning),
            PermissionMode::Custom => ("自定义", "permission-custom", theme.text_tertiary),
        }
    }

    fn permission_row(
        &self,
        index: usize,
        mode: PermissionMode,
        title: &'static str,
        detail: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.permission_mode == mode;
        let warning = mode == PermissionMode::Full;
        let color = if warning { theme.warning } else { theme.text };
        div()
            .id(("permission-menu-item", index))
            .h(px(47.125))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.permission_menu_open = false;
                if mode == PermissionMode::Full && this.permission_mode != PermissionMode::Full {
                    cx.emit(RequestFullAccess);
                } else {
                    this.permission_mode = mode;
                }
                cx.notify();
            }))
            .child(icon(glyph, color.into()).size(px(20.0)).opacity(0.75))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .ml(px(12.0))
                    // CoreText's Chinese glyph run is fractionally wider than
                    // Chromium's system-ui run sampled over CDP.
                    .text_size(px(12.75))
                    .line_height(px(18.5625))
                    .child(div().text_color(color).child(title))
                    .child(
                        div()
                            .text_color(if warning {
                                theme.warning
                            } else {
                                theme.text_tertiary
                            })
                            .child(detail),
                    ),
            )
            .when(selected, |row| {
                row.child(
                    icon("permission-check", color.into())
                        .size(px(17.0))
                        .ml(px(12.0))
                        .opacity(0.75),
                )
            })
    }

    fn permission_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let width = if self.permission_mode == PermissionMode::Full {
            355.0
        } else {
            327.0
        };
        div()
            .id("permission-menu")
            .absolute()
            .left(px(42.0))
            // CDP: the menu's bottom edge is 1.5px above the 28px trigger.
            .bottom(px(36.5))
            .w(px(width))
            .h(px(222.5))
            .p(px(4.0))
            .rounded(px(15.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(
                div()
                    .h(px(26.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .flex()
                    .items_start()
                    .text_size(px(13.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(div().flex_1().child("应如何批准 ChatGPT 操作？"))
                    .child(
                        div()
                            .id("permission-learn-more")
                            .cursor_pointer()
                            .underline()
                            .child("了解更多"),
                    ),
            )
            .child(self.permission_row(
                0,
                PermissionMode::Request,
                "请求批准",
                "编辑外部文件和使用互联网时始终询问",
                "permission-request",
                theme,
                cx,
            ))
            .child(self.permission_row(
                1,
                PermissionMode::Assist,
                "帮我批准",
                "仅对检测到的风险操作请求批准",
                "permission-assist",
                theme,
                cx,
            ))
            .child(self.permission_row(
                2,
                PermissionMode::Full,
                "完全访问权限",
                "可不受限制地访问互联网和你电脑上的任何文件",
                "permission",
                theme,
                cx,
            ))
            .child(self.permission_row(
                3,
                PermissionMode::Custom,
                "自定义 (config.toml)",
                "使用 config.toml 中定义的权限",
                "permission-custom",
                theme,
                cx,
            ))
    }

    fn render_composer(&self, viewport_width: f32, theme: Theme, cx: &mut Context<Self>) -> Div {
        let (permission_label, permission_icon, permission_color) = self.permission_label();
        let effective_model_label = self.effective_model_label();
        let model_status_active = self.model_status.is_some();
        let effort_or_status_label = self
            .model_status
            .clone()
            .unwrap_or_else(|| self.selected_effort_label());
        let fast_tier_selected = self.selected_service_tier.is_some();
        let trigger_label = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(MODEL_PICKER_TRIGGER_GAP))
            .when(self.menu_open, |label| label.flex_1().justify_center())
            .child(div().text_color(theme.text).child(effective_model_label))
            .child(
                div()
                    .text_color(if model_status_active {
                        theme.effort
                    } else {
                        theme.text_tertiary
                    })
                    .child(effort_or_status_label),
            );
        let trigger_value = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .when(self.menu_open, |value| {
                value.flex_1().child(
                    div()
                        .w(px(18.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(fast_tier_selected, |indicator| {
                            indicator.child(icon("model-fast", theme.text.into()).size(px(14.0)))
                        }),
                )
            })
            .when(!self.menu_open && fast_tier_selected, |value| {
                // ChatGPT CDP: the closed trigger uses a 14px fast glyph with
                // exactly 4px between its right edge and the model label.
                value
                    .gap(px(MODEL_PICKER_TRIGGER_GAP))
                    .child(icon("model-fast", theme.text.into()).size(px(14.0)))
            })
            .child(trigger_label);
        let prompt_is_empty = self.prompt_input.read(cx).text().is_empty();
        let conversation_started = self.conversation_phase != ConversationPhase::Empty;
        let generation_active = matches!(
            self.conversation_phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
        );
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .when(!conversation_started, |composer| {
                composer.child(context_toolbar(theme))
            })
            .child(
                div()
                    .h(px(98.0))
                    .w_full()
                    .rounded(px(24.0))
                    .bg(theme.control_soft)
                    .shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                            .spread_radius(px(0.5)),
                        BoxShadow::new(px(0.0), px(3.0), hsla(0.0, 0.0, 0.0, 0.04))
                            .blur_radius(px(7.5)),
                        BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.05))
                            .blur_radius(px(20.0)),
                        BoxShadow::new(px(0.0), px(0.0), theme.surface.into())
                            .spread_radius(px(0.5)),
                    ])
                    .flex()
                    .flex_col()
                    // Composer labels are uniformly system-ui 400 in the
                    // reference (placeholder 14/20; controls 13/18).
                    .font_weight(gpui::FontWeight::NORMAL)
                    .px(px(8.0))
                    .py(px(12.0))
                    .child(self.prompt_input.clone())
                    .when(self.dictation_state == DictationState::Idle, |composer| {
                        composer.child(
                            div()
                                .h(px(36.0))
                                .relative()
                                .top(px(7.0))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .id("composer-add-context")
                                                .size(px(28.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .child(
                                                    icon("add", theme.text.into()).size(px(16.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("composer-permissions")
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(13.0))
                                                .line_height(px(18.0))
                                                .text_color(permission_color)
                                                .cursor_pointer()
                                                .when(self.permission_menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        this.menu_open = false;
                                                        this.submenu = None;
                                                        this.permission_menu_open =
                                                            !this.permission_menu_open;
                                                        cx.notify();
                                                    }),
                                                )
                                                .on_click(cx.listener(|_, _, _, cx| {
                                                    cx.stop_propagation();
                                                }))
                                                .child(
                                                    div()
                                                        .size(px(16.0))
                                                        .flex_none()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(icon(
                                                            permission_icon,
                                                            permission_color.into(),
                                                        )),
                                                )
                                                .child(permission_label),
                                        ),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .id("composer-model-picker")
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .when(self.menu_open, |button| {
                                                    button.w(px(MODEL_PICKER_WIDTH)).flex_none()
                                                })
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(13.0))
                                                .line_height(px(18.0))
                                                .cursor_pointer()
                                                .when(self.menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    cx.stop_propagation();
                                                    this.permission_menu_open = false;
                                                    this.menu_open = !this.menu_open;
                                                    if !this.menu_open {
                                                        this.submenu = None;
                                                    } else {
                                                        this.model_menu_keyboard_focus = false;
                                                        this.submenu_keyboard_focus = false;
                                                        window.focus(&this.model_menu_focus, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(trigger_value)
                                                .child(
                                                    icon(
                                                        "chevron-down",
                                                        theme.text_tertiary.into(),
                                                    )
                                                    .size(px(14.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .id("composer-dictation")
                                                        .size(px(28.0))
                                                        .rounded_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(move |style| {
                                                            style.bg(theme.sidebar_hover)
                                                        })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_dictation(cx)
                                                        }))
                                                        .child(
                                                            icon("dictation", theme.text.into())
                                                                .size(px(16.0)),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("composer-voice")
                                                        .size(px(28.0))
                                                        .flex_none()
                                                        .rounded_full()
                                                        .bg(theme.button)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .when(
                                                            prompt_is_empty
                                                                && conversation_started
                                                                && !generation_active,
                                                            |button| button.opacity(0.4),
                                                        )
                                                        .when(
                                                            !prompt_is_empty
                                                                || generation_active
                                                                || !conversation_started,
                                                            |button| button.cursor_pointer(),
                                                        )
                                                        .when(!generation_active, |button| {
                                                            button.on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.prompt_input.update(
                                                                        cx,
                                                                        |input, cx| {
                                                                            input.submit(cx)
                                                                        },
                                                                    );
                                                                },
                                                            ))
                                                        })
                                                        .when(generation_active, |button| {
                                                            button.on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.stop_generation(cx)
                                                                },
                                                            ))
                                                        })
                                                        .child(
                                                            icon(
                                                                if generation_active {
                                                                    "composer-stop"
                                                                } else if !prompt_is_empty
                                                                    || conversation_started
                                                                {
                                                                    "dictation-send"
                                                                } else {
                                                                    "voice"
                                                                },
                                                                theme.button_text.into(),
                                                            )
                                                            .size(px(
                                                                if generation_active
                                                                    || !prompt_is_empty
                                                                    || conversation_started
                                                                {
                                                                    20.0
                                                                } else {
                                                                    16.0
                                                                },
                                                            )),
                                                        ),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .when(self.dictation_state != DictationState::Idle, |composer| {
                        composer.child(self.dictation_footer(theme, cx))
                    }),
            )
            .when(self.menu_open, |composer| {
                composer.child(deferred(self.model_menu(viewport_width, theme, cx)))
            })
            .when(self.permission_menu_open, |composer| {
                composer.child(deferred(self.permission_menu(theme, cx)))
            })
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_composer(
            f32::from(window.viewport_size().width),
            Theme::for_mode(self.mode),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComposerView, ConversationActivity, ConversationChanged, ConversationPhase,
        MODEL_PICKER_DETAIL_ROW_HEIGHT, MODEL_PICKER_ROW_HEIGHT,
        MODEL_PICKER_SUBMENU_BOTTOM_OFFSET, MODEL_PICKER_SUBMENU_HEADER_HEIGHT,
        MODEL_PICKER_SUBMENU_VERTICAL_PADDING, MODEL_PICKER_TRIGGER_GAP, STREAM_EVENTS_PER_UPDATE,
        STREAM_UPDATE_INTERVAL, SubmenuLayout, collect_ready_agent_events,
        current_local_time_label, ensure_closed_batch_is_terminal, find_command_activity_mut,
        max_particle_drift, particle_layers, particle_transition_ease, push_coalesced_agent_event,
        submenu_layout, upsert_command_activity,
    };
    use crate::agent::{
        AgentEvent, AgentModel, AgentModelCatalog, AgentReasoningEffort, AgentServiceTier,
        CommandExecution, CommandExecutionStatus,
    };
    use crate::theme::ThemeMode;
    use gpui::{
        Bounds, Focusable, MouseButton, TestApp, WindowBounds, WindowOptions, point, px, size,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    fn test_model_catalog() -> AgentModelCatalog {
        AgentModelCatalog {
            models: vec![
                AgentModel {
                    id: "model-a-id".into(),
                    model: "model-a".into(),
                    display_name: "Model A".into(),
                    description: "First model".into(),
                    supported_reasoning_efforts: vec![AgentReasoningEffort {
                        id: "low".into(),
                        description: "Light reasoning".into(),
                    }],
                    default_reasoning_effort: "low".into(),
                    service_tiers: Vec::new(),
                    default_service_tier: None,
                    is_default: false,
                },
                AgentModel {
                    id: "model-b-id".into(),
                    model: "model-b".into(),
                    display_name: "Model B".into(),
                    description: "Default model".into(),
                    supported_reasoning_efforts: vec![
                        AgentReasoningEffort {
                            id: "medium".into(),
                            description: "Balanced".into(),
                        },
                        AgentReasoningEffort {
                            id: "high".into(),
                            description: "Deep".into(),
                        },
                    ],
                    default_reasoning_effort: "high".into(),
                    service_tiers: vec![AgentServiceTier {
                        id: "priority".into(),
                        name: "Fast".into(),
                        description: "Lower latency".into(),
                    }],
                    default_service_tier: Some("priority".into()),
                    is_default: true,
                },
            ],
        }
    }

    #[test]
    fn command_output_deltas_are_reconciled_with_completion() {
        let mut activities = vec![ConversationActivity::Command(CommandExecution {
            id: "exec_1".into(),
            command: "printf hello".into(),
            cwd: "/tmp".into(),
            output: String::new(),
            status: CommandExecutionStatus::InProgress,
            exit_code: None,
        })];
        find_command_activity_mut(&mut activities, "exec_1")
            .unwrap()
            .output
            .push_str("hel");
        find_command_activity_mut(&mut activities, "exec_1")
            .unwrap()
            .output
            .push_str("lo\n");

        upsert_command_activity(
            &mut activities,
            CommandExecution {
                id: "exec_1".into(),
                command: "printf hello".into(),
                cwd: "/tmp".into(),
                output: "hello\n".into(),
                status: CommandExecutionStatus::Completed,
                exit_code: Some(0),
            },
        );

        let command = find_command_activity_mut(&mut activities, "exec_1").unwrap();
        assert_eq!(command.output, "hello\n");
        assert_eq!(command.status, CommandExecutionStatus::Completed);
        assert_eq!(command.exit_code, Some(0));
    }

    #[test]
    fn adjacent_stream_deltas_are_coalesced_without_reordering_boundaries() {
        let mut batch = Vec::new();
        for event in [
            AgentEvent::Started,
            AgentEvent::AssistantMessageStarted {
                item_id: "message_1".into(),
            },
            AgentEvent::TextDelta("你".into()),
            AgentEvent::TextDelta("好".into()),
            AgentEvent::CommandOutputDelta {
                item_id: "command_1".into(),
                delta: "hel".into(),
            },
            AgentEvent::CommandOutputDelta {
                item_id: "command_1".into(),
                delta: "lo\n".into(),
            },
            AgentEvent::TextDelta("世界".into()),
        ] {
            push_coalesced_agent_event(&mut batch, event);
        }

        assert_eq!(
            batch,
            vec![
                AgentEvent::Started,
                AgentEvent::AssistantMessageStarted {
                    item_id: "message_1".into(),
                },
                AgentEvent::TextDelta("你好".into()),
                AgentEvent::CommandOutputDelta {
                    item_id: "command_1".into(),
                    delta: "hello\n".into(),
                },
                AgentEvent::TextDelta("世界".into()),
            ]
        );
    }

    #[test]
    fn live_stream_commits_one_change_for_an_entire_protocol_burst() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let (sender, receiver) = async_channel::unbounded();
        let changed_count = Arc::new(AtomicUsize::new(0));
        let emitter = composer.clone();
        let changed_count_for_subscription = changed_count.clone();

        app.update_entity(&composer, move |composer, cx| {
            composer.conversation_cycle = 7;
            cx.subscribe(&emitter, move |_, _, _: &ConversationChanged, _| {
                changed_count_for_subscription.fetch_add(1, Ordering::Relaxed);
            })
            .detach();
            composer.consume_agent_events(receiver, 7, cx);
        });

        for event in [
            AgentEvent::Started,
            AgentEvent::AssistantMessageStarted {
                item_id: "message_1".into(),
            },
            AgentEvent::TextDelta("平滑".into()),
            AgentEvent::TextDelta("输出".into()),
        ] {
            sender.send_blocking(event).unwrap();
        }
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.conversation_phase()),
            ConversationPhase::Empty
        );

        app.advance_clock(STREAM_UPDATE_INTERVAL);
        app.run_until_parked();
        let (phase, _, _, text, _) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Streaming);
        assert_eq!(text, "平滑输出");
        assert_eq!(changed_count.load(Ordering::Relaxed), 1);

        sender.send_blocking(AgentEvent::Completed).unwrap();
        drop(sender);
        app.run_until_parked();
        app.advance_clock(STREAM_UPDATE_INTERVAL);
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.conversation_phase()),
            ConversationPhase::Complete
        );
        assert_eq!(changed_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn unexpected_live_stream_disconnect_fails_only_its_own_cycle() {
        let mut app = TestApp::new();
        let current = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let stale = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        let (current_sender, current_receiver) = async_channel::unbounded();
        let (stale_sender, stale_receiver) = async_channel::unbounded();

        app.update_entity(&current, |composer, cx| {
            composer.conversation_cycle = 3;
            composer.consume_agent_events(current_receiver, 3, cx);
        });
        app.update_entity(&stale, |composer, cx| {
            composer.conversation_cycle = 5;
            composer.conversation_phase = ConversationPhase::Starting;
            composer.consume_agent_events(stale_receiver, 4, cx);
        });

        drop(current_sender);
        drop(stale_sender);
        app.run_until_parked();

        assert_eq!(
            app.read_entity(&current, |composer, _| composer.conversation_phase()),
            ConversationPhase::Failed
        );
        assert_eq!(
            app.read_entity(&stale, |composer, _| composer.conversation_phase()),
            ConversationPhase::Starting
        );
    }

    #[test]
    fn a_stream_batch_applies_all_text_before_its_terminal_event() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        app.update_entity(&composer, |composer, _| {
            assert!(composer.apply_agent_event_batch(vec![
                AgentEvent::Started,
                AgentEvent::AssistantMessageStarted {
                    item_id: "message_1".into(),
                },
                AgentEvent::TextDelta("流式".into()),
                AgentEvent::TextDelta("内容".into()),
                AgentEvent::Completed,
                AgentEvent::TextDelta("不应越过终止事件".into()),
            ]));
        });

        let (phase, _, _, text, completed_at) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Complete);
        assert_eq!(text, "流式内容");
        assert!(completed_at.is_some());
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer
                .conversation_activity_snapshot()),
            vec![ConversationActivity::AssistantMessage {
                item_id: "message_1".into(),
                text: "流式内容".into(),
            }]
        );
    }

    #[test]
    fn stream_batch_limit_defers_excess_events_without_losing_text() {
        let (sender, receiver) = async_channel::unbounded();
        for _ in 0..=STREAM_EVENTS_PER_UPDATE {
            sender
                .send_blocking(AgentEvent::TextDelta("x".into()))
                .unwrap();
        }
        sender.send_blocking(AgentEvent::Completed).unwrap();
        drop(sender);

        let first_event = receiver.try_recv().unwrap();
        let (first_batch, first_closed) = collect_ready_agent_events(&receiver, first_event);
        assert!(!first_closed);
        assert_eq!(
            first_batch,
            vec![AgentEvent::TextDelta("x".repeat(STREAM_EVENTS_PER_UPDATE))]
        );

        let first_event = receiver.try_recv().unwrap();
        let (mut second_batch, second_closed) = collect_ready_agent_events(&receiver, first_event);
        assert!(second_closed);
        ensure_closed_batch_is_terminal(&mut second_batch);

        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            assert!(!composer.apply_agent_event_batch(first_batch));
            assert!(composer.apply_agent_event_batch(second_batch));
        });
        let (phase, _, _, text, _) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Complete);
        assert_eq!(text, "x".repeat(STREAM_EVENTS_PER_UPDATE + 1));
    }

    #[test]
    fn a_closed_stream_without_a_terminal_event_becomes_failed() {
        let mut batch = vec![
            AgentEvent::AssistantMessageStarted {
                item_id: "message_1".into(),
            },
            AgentEvent::TextDelta("partial".into()),
        ];
        ensure_closed_batch_is_terminal(&mut batch);

        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            assert!(composer.apply_agent_event_batch(batch));
        });
        let (phase, _, _, message, completed_at) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Failed);
        assert_eq!(message, super::STREAM_DISCONNECTED_MESSAGE);
        assert!(completed_at.is_some());
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer
                .conversation_activity_snapshot()),
            vec![
                ConversationActivity::AssistantMessage {
                    item_id: "message_1".into(),
                    text: "partial".into(),
                },
                ConversationActivity::Error {
                    message: super::STREAM_DISCONNECTED_MESSAGE.into(),
                },
            ]
        );
    }

    #[test]
    fn render_snapshot_only_copies_the_aggregate_when_the_view_needs_it() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            composer.apply_agent_event_batch(vec![
                AgentEvent::AssistantMessageStarted {
                    item_id: "message_1".into(),
                },
                AgentEvent::TextDelta("正在流式输出".into()),
            ]);
        });

        let streaming = app.read_entity(&composer, |composer, _| {
            composer.conversation_render_snapshot()
        });
        assert_eq!(streaming.0, ConversationPhase::Streaming);
        assert!(streaming.3.is_empty());
        assert_eq!(
            streaming.5,
            vec![ConversationActivity::AssistantMessage {
                item_id: "message_1".into(),
                text: "正在流式输出".into(),
            }]
        );

        app.update_entity(&composer, |composer, _| {
            composer.apply_agent_event_batch(vec![AgentEvent::Completed]);
        });
        let complete = app.read_entity(&composer, |composer, _| {
            composer.conversation_render_snapshot()
        });
        assert_eq!(complete.0, ConversationPhase::Complete);
        assert_eq!(complete.3, "正在流式输出");
    }

    #[test]
    fn sent_message_time_uses_the_local_twenty_four_hour_label() {
        let label = current_local_time_label();
        assert_eq!(label.len(), 5);
        assert_eq!(&label[2..3], ":");
        assert!(label[..2].parse::<u8>().is_ok_and(|hour| hour < 24));
        assert!(label[3..].parse::<u8>().is_ok_and(|minute| minute < 60));
    }

    #[test]
    fn stopping_generation_records_the_response_completion_time() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        app.update_entity(&composer, |composer, cx| {
            composer.conversation_phase = ConversationPhase::Streaming;
            composer.assistant_message_time = None;
            composer.stop_generation(cx);
        });

        let (phase, _, _, _, completed_at) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Complete);
        let completed_at = completed_at.expect("stopped response should retain its end time");
        assert_eq!(completed_at.len(), 5);
        assert_eq!(&completed_at[2..3], ":");
    }

    #[test]
    fn submenu_clamps_to_the_trailing_edge_at_reference_width() {
        let layout = submenu_layout(1440.0, 280.0);
        assert!(!layout.open_left);
        assert_eq!(layout.width, 280.0);

        assert_eq!(
            submenu_layout(1440.0, 180.0),
            SubmenuLayout {
                open_left: false,
                width: 180.0,
            }
        );
    }

    #[test]
    fn submenu_flips_before_it_can_overflow_a_small_window() {
        assert_eq!(
            submenu_layout(900.0, 280.0),
            SubmenuLayout {
                open_left: true,
                width: 280.0,
            }
        );
    }

    #[test]
    fn model_picker_geometry_matches_the_chatgpt_cdp_measurements() {
        let model_height = 7.0 * MODEL_PICKER_ROW_HEIGHT + MODEL_PICKER_SUBMENU_VERTICAL_PADDING;
        let effort_height = 5.0 * MODEL_PICKER_ROW_HEIGHT
            + MODEL_PICKER_DETAIL_ROW_HEIGHT
            + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
            + MODEL_PICKER_SUBMENU_VERTICAL_PADDING;
        let speed_height = 2.0 * MODEL_PICKER_DETAIL_ROW_HEIGHT
            + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
            + MODEL_PICKER_SUBMENU_VERTICAL_PADDING;

        assert!((model_height - 207.9375).abs() < 0.001);
        assert!((effort_height - 223.9375).abs() < 0.001);
        assert!((speed_height - 128.25).abs() < 0.001);
        assert!((MODEL_PICKER_SUBMENU_BOTTOM_OFFSET - model_height + 23.9375).abs() < 0.001);
        assert!((MODEL_PICKER_SUBMENU_BOTTOM_OFFSET - effort_height + 39.9375).abs() < 0.001);
        assert!((MODEL_PICKER_SUBMENU_BOTTOM_OFFSET - speed_height - 55.75).abs() < 0.001);
        assert_eq!(MODEL_PICKER_TRIGGER_GAP, 4.0);
        assert_eq!(ComposerView::effort_label("low"), "轻度");
        assert_eq!(
            ComposerView::effort_detail("ultra"),
            Some("更快消耗使用额度")
        );
        assert_eq!(ComposerView::effort_detail("high"), None);
    }

    #[test]
    fn catalog_default_selection_and_model_switch_use_advertised_defaults() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            composer.apply_model_catalog(test_model_catalog())
        });

        let selected = app.read_entity(&composer, |composer, _| {
            (
                composer.selected_model.clone(),
                composer.selected_effort.clone(),
                composer.selected_service_tier.clone(),
                composer.selected_model_label(),
            )
        });
        assert_eq!(
            selected,
            (
                "model-b".into(),
                "high".into(),
                Some("priority".into()),
                "Model B".into()
            )
        );

        app.update_entity(&composer, |composer, _| composer.select_model_at(0));
        let switched = app.read_entity(&composer, |composer, _| {
            (
                composer.selected_model.clone(),
                composer.selected_effort.clone(),
                composer.selected_service_tier.clone(),
            )
        });
        assert_eq!(switched, ("model-a".into(), "low".into(), None));
    }

    #[test]
    fn empty_model_picker_never_exposes_intermediate_loading_copy() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.selected_model_label()),
            "模型不可用"
        );
    }

    #[test]
    fn changed_picker_values_can_reset_to_the_advertised_defaults() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            composer.apply_model_catalog(test_model_catalog());
            assert!(composer.selection_is_default());
            composer.select_effort_at(0);
            assert!(!composer.selection_is_default());
            composer.reset_model_selection();
        });

        assert_eq!(
            app.read_entity(&composer, |composer, _| (
                composer.selected_model.clone(),
                composer.selected_effort.clone(),
                composer.selected_service_tier.clone(),
                composer.advanced_expanded,
                composer.selection_is_default(),
            )),
            (
                "model-b".into(),
                "high".into(),
                Some("priority".into()),
                false,
                true,
            )
        );
    }

    #[test]
    fn slider_uses_the_selected_models_dynamic_effort_options() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            composer.apply_model_catalog(test_model_catalog());
            composer.set_slider_index(0);
        });
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.selected_effort.clone()),
            "medium"
        );
        app.update_entity(&composer, |composer, _| composer.set_slider_index(99));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.selected_effort.clone()),
            "high"
        );
    }

    #[test]
    fn model_notifications_update_the_effective_model_buffering_and_error_state() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
        app.update_entity(&composer, |composer, _| {
            composer.apply_model_catalog(test_model_catalog());
            assert!(
                !composer.apply_agent_event_batch(vec![AgentEvent::ModelRerouted {
                    from_model: "model-b".into(),
                    to_model: "model-a".into(),
                    reason: "highRiskCyberActivity".into(),
                }])
            );
        });
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.effective_model_label()),
            "Model A"
        );
        assert!(app.read_entity(&composer, |composer, _| {
            composer
                .model_status
                .as_deref()
                .is_some_and(|status| status.contains("自动切换"))
        }));

        app.update_entity(&composer, |composer, _| {
            assert!(!composer.apply_agent_event_batch(vec![
                AgentEvent::ModelSafetyBufferingUpdated {
                    model: "model-a".into(),
                    use_cases: vec!["cyber".into()],
                    reasons: vec!["review".into()],
                    show_buffering_ui: true,
                    faster_model: Some("model-b".into()),
                },
            ]));
        });
        assert!(app.read_entity(&composer, |composer, _| composer.safety_buffering));
        assert!(app.read_entity(&composer, |composer, _| {
            composer
                .model_status
                .as_deref()
                .is_some_and(|status| status.contains("安全检查中"))
        }));

        app.update_entity(&composer, |composer, _| {
            assert!(!composer.apply_agent_event_batch(vec![
                AgentEvent::ModelSafetyBufferingUpdated {
                    model: "model-a".into(),
                    use_cases: Vec::new(),
                    reasons: Vec::new(),
                    show_buffering_ui: false,
                    faster_model: None,
                },
            ]));
        });
        assert!(!app.read_entity(&composer, |composer, _| composer.safety_buffering));
        assert!(app.read_entity(&composer, |composer, _| composer.model_status.is_none()));

        app.update_entity(&composer, |composer, _| {
            assert!(composer.apply_agent_event_batch(vec![
                AgentEvent::ModelVerificationRequired {
                    verifications: vec!["trustedAccessForCyber".into()],
                },
            ]));
        });
        let (phase, _, _, message, _) =
            app.read_entity(&composer, |composer, _| composer.conversation_snapshot());
        assert_eq!(phase, ConversationPhase::Failed);
        assert!(message.contains("trustedAccessForCyber"));
    }

    #[test]
    fn permission_modes_update_the_label_and_outside_close_dismisses_the_menu() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        assert_eq!(
            app.read_entity(&composer, |c, _| c.permission_mode_name()),
            "full"
        );
        app.update_entity(&composer, |composer, cx| {
            composer.set_permission_mode("assist", cx);
            composer.open_permission_menu(cx);
        });
        assert_eq!(
            app.read_entity(&composer, |c, _| c.permission_mode_name()),
            "assist"
        );
        assert!(app.read_entity(&composer, |c, _| c.permission_menu_open));

        app.update_entity(&composer, |composer, cx| composer.close_picker(cx));
        assert!(!app.read_entity(&composer, |c, _| c.permission_menu_open));
    }

    #[test]
    fn particle_drift_uses_the_reference_ease_and_has_a_seamless_loop() {
        assert_eq!(particle_transition_ease(0.0), 0.0);
        assert_eq!(particle_transition_ease(1.0), 1.0);
        assert!((particle_transition_ease(0.5) - 0.5).abs() < 0.002);

        for index in 0..14 {
            let start = max_particle_drift(0.0, index, 1_701);
            let end = max_particle_drift(1.0, index, 1_701);
            assert!((start.0 - end.0).abs() < 0.001);
            assert!((start.1 - end.1).abs() < 0.001);
        }
    }

    #[test]
    fn ultra_fast_uses_only_the_fast_particle_layer_observed_over_cdp() {
        assert_eq!(particle_layers(true, false), (true, false));
        assert_eq!(particle_layers(true, true), (false, true));
        assert_eq!(particle_layers(false, true), (false, true));
    }

    #[test]
    fn dictation_can_start_and_cancel_without_leaving_transcribed_content() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        app.update_entity(&composer, |composer, cx| composer.start_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "recording"
        );

        app.update_entity(&composer, |composer, cx| composer.cancel_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "idle"
        );
    }

    #[test]
    fn stopping_dictation_shows_processing_then_returns_to_idle() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));

        app.update_entity(&composer, |composer, cx| composer.start_dictation(cx));
        app.update_entity(&composer, |composer, cx| composer.stop_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "transcribing"
        );

        app.advance_clock(Duration::from_millis(1_050));
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "idle"
        );
    }

    #[test]
    fn rendered_microphone_cancel_and_stop_hit_targets_drive_the_state_machine() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(786.0), px(138.0)),
                })),
                ..Default::default()
            },
            |_, cx| ComposerView::new(ThemeMode::Dark, cx),
        );

        window.draw();
        window.simulate_mouse_move(point(px(716.0), px(110.0)));
        window.simulate_mouse_down(point(px(716.0), px(110.0)), MouseButton::Left);
        window.simulate_mouse_up(point(px(716.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "recording"
        );

        window.draw();
        window.simulate_click(point(px(22.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "idle"
        );

        window.draw();
        window.simulate_click(point(px(716.0), px(110.0)), MouseButton::Left);
        window.draw();
        window.simulate_click(point(px(716.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "transcribing"
        );
    }

    #[test]
    fn permission_trigger_opens_on_mouse_down_like_the_radix_reference() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(786.0), px(138.0)),
                })),
                ..Default::default()
            },
            |_, cx| ComposerView::new(ThemeMode::Dark, cx),
        );

        window.draw();
        window.simulate_mouse_move(point(px(83.0), px(111.0)));
        window.simulate_mouse_down(point(px(83.0), px(111.0)), MouseButton::Left);
        assert!(window.read(|composer, _| composer.permission_menu_open));
        window.simulate_mouse_up(point(px(83.0), px(111.0)), MouseButton::Left);
        assert!(window.read(|composer, _| composer.permission_menu_open));
    }

    #[test]
    fn prompt_accepts_native_text_and_clears_without_a_stale_ime_range() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(748.0), px(138.0)),
                })),
                ..Default::default()
            },
            |_, cx| ComposerView::new(ThemeMode::Dark, cx),
        );
        window.draw();
        // Reproduces the real crash: clicking to the right of the empty
        // placeholder used to store a placeholder byte index in an empty value.
        window.simulate_click(point(px(350.0), px(60.0)), MouseButton::Left);
        window.update(|composer, window, cx| {
            assert!(
                composer
                    .prompt_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
        });
        window.simulate_input("hello");
        assert_eq!(
            window.read(|composer, cx| composer.prompt_input.read(cx).text().to_owned()),
            "hello"
        );
        window.update(|composer, _, cx| {
            composer
                .prompt_input
                .update(cx, |input, cx| input.clear(cx));
        });
        assert_eq!(
            window.read(|composer, cx| composer.prompt_input.read(cx).text().to_owned()),
            ""
        );
    }

    #[test]
    fn model_picker_keyboard_navigation_enters_selects_and_escapes_submenus() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(748.0), px(400.0)),
                })),
                ..Default::default()
            },
            |_, cx| ComposerView::new(ThemeMode::Dark, cx),
        );
        window.update(|composer, window, cx| {
            composer.apply_model_catalog(test_model_catalog());
            composer.open_picker(cx);
            window.focus(&composer.model_menu_focus, cx);
        });
        window.draw();
        window.simulate_keystrokes("down right down enter");
        assert_eq!(
            window.read(|composer, _| composer.selected_model.clone()),
            "model-a"
        );
        assert!(!window.read(|composer, _| composer.menu_open));

        window.update(|composer, window, cx| {
            composer.open_picker(cx);
            window.focus(&composer.model_menu_focus, cx);
        });
        window.draw();
        window.simulate_keystrokes("down right escape escape");
        assert!(!window.read(|composer, _| composer.menu_open));
    }
}
