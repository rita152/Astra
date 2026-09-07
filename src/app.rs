use std::{
    collections::HashMap,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, Entity, FocusHandle, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PathPromptOptions, Render, Role, StyleRefinement, Transformation, Window, WindowAppearance,
    canvas, deferred, div, hsla, linear_color_stop, linear_gradient, prelude::*, px, radians, rgba,
};

gpui::actions!(permission_ui, [DismissPermissionUi, ToggleTerminal]);

use crate::{
    agent::{
        AgentBackend, CodexAppServerBackend, CodexAppServerManager, ProjectId, ThreadId,
        generated_image_dimensions,
    },
    components::{
        composer::{
            ComposerView, ConversationThreadCreated, ModelCatalogLoadFinished,
            RequestFullAccessConfirmation,
        },
        file_change::{
            DiffFileVisualState, DiffReviewCallback, DiffReviewEvent, DiffReviewPresentation,
            captured_diff_review_fixture, render_diff_review_panel,
        },
        home::{
            HomeView, OpenDiffReview, OpenImagePreview, OpenSubAgentPanel, RetryImageGeneration,
        },
        icons::icon,
        sidebar::{NewConversation, OpenProjectCreation, OpenSettings, SelectThread, SidebarView},
        terminal::TerminalPanel,
    },
    settings::{ChangeTheme, CloseSettings, SettingsView},
    theme::{Theme, ThemeMode, UI_FONT_FAMILY, ui_font},
    workspace::{WorkspaceStore, project_id_for_thread},
};

#[cfg(not(test))]
use crate::workspace::WorkspaceSnapshot;

pub struct ChatApp {
    codex_app_server: Arc<CodexAppServerManager>,
    _agent_backend: Arc<dyn AgentBackend>,
    workspace_store: Arc<WorkspaceStore>,
    conversation_hosts: HashMap<ConversationKey, ConversationHost>,
    active_conversation: ConversationKey,
    next_draft_id: u64,
    mode: ThemeMode,
    startup_model_catalog_resolved: bool,
    startup_sidebar_resolved: bool,
    startup_minimum_duration_elapsed: bool,
    sidebar: Entity<SidebarView>,
    home: Entity<HomeView>,
    settings: Entity<SettingsView>,
    showing_settings: bool,
    sidebar_collapsed: bool,
    sidebar_resize_hovered: bool,
    sidebar_resize_dragging: bool,
    sidebar_resize_pointer_offset: f32,
    sidebar_reveal: f32,
    sidebar_animation_from: f32,
    sidebar_animation_to: f32,
    sidebar_animation_started_at: Option<Instant>,
    sidebar_animation_duration: Duration,
    sidebar_animation_running: bool,
    bottom_panel_open: bool,
    bottom_panel_tabs: Vec<BottomPanelMode>,
    bottom_panel_active_tab: Option<usize>,
    bottom_panel_hovered_tab: Option<usize>,
    bottom_panel_add_menu_open: bool,
    bottom_panel_focused_item: usize,
    bottom_panel_keyboard_focus: bool,
    bottom_panel_focus: FocusHandle,
    bottom_panel_focus_pending: bool,
    terminal_panels: HashMap<ConversationKey, Entity<TerminalPanel>>,
    terminal_return_focus_pending: bool,
    right_panel_open: bool,
    right_panel_mode: Option<RightPanelMode>,
    right_panel_focused_item: usize,
    right_panel_keyboard_focus: bool,
    right_panel_focus: FocusHandle,
    right_panel_focus_pending: bool,
    right_panel_width: Option<f32>,
    right_panel_resize_hovered: bool,
    right_panel_resize_dragging: bool,
    right_panel_resize_pointer_offset: f32,
    subagent_panel: Option<SubagentPanel>,
    subagent_panel_menu_open: bool,
    diff_review: Option<DiffReviewPresentation>,
    image_preview: Option<PathBuf>,
    image_preview_focus: FocusHandle,
    image_preview_focus_active: bool,
    image_preview_previous_focus: Option<FocusHandle>,
    image_preview_dimensions: Option<(u32, u32)>,
    image_preview_zoom: f32,
    permission_confirmation_open: bool,
    project_creation_open: bool,
    project_creation_kind: ProjectCreationKind,
    project_creation_step: ProjectCreationStep,
    project_creation_focused_item: usize,
    project_creation_keyboard_focus: bool,
    project_creation_focus: FocusHandle,
    project_creation_focus_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DraftId(u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ConversationKey {
    Draft(DraftId),
    Thread(ThreadId),
}

struct ConversationHost {
    composer: Entity<ComposerView>,
    cwd: PathBuf,
    project_id: Option<ProjectId>,
}

#[derive(Clone)]
struct SubagentPanel {
    thread_id: ThreadId,
    name: String,
    home: Entity<HomeView>,
}

impl Drop for ChatApp {
    fn drop(&mut self) {
        self.codex_app_server.shutdown();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectCreationKind {
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectCreationStep {
    Kind,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RightPanelMode {
    SideChat,
    Browser,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BottomPanelMode {
    Review,
    Terminal,
    Browser,
    Files,
    SideChat,
}

const BOTTOM_PANEL_ITEMS: &[(BottomPanelMode, &str, &str, &str)] = &[
    (BottomPanelMode::Review, "审查", "⌃⇧G", "panel-review"),
    (BottomPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
    (BottomPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (BottomPanelMode::Files, "文件", "⌘P", "panel-files"),
    (BottomPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
];
const BOTTOM_PANEL_HEIGHT: f32 = 280.0;

fn bottom_panel_tab_spec(mode: BottomPanelMode) -> (&'static str, &'static str) {
    match mode {
        BottomPanelMode::Review => ("审查", "panel-review"),
        BottomPanelMode::Terminal => ("终端", "panel-terminal"),
        // CDP: a browser item appended from the add menu is titled 新标签页.
        BottomPanelMode::Browser => ("新标签页", "panel-browser"),
        BottomPanelMode::Files => ("文件", "panel-files"),
        BottomPanelMode::SideChat => ("侧边聊天", "side-chat"),
    }
}

const RIGHT_PANEL_ITEMS: &[(RightPanelMode, &str, &str, &str)] = &[
    (RightPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
    (RightPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (RightPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
];
const SIDEBAR_MIN_WIDTH: f32 = 240.0;
const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const RIGHT_PANEL_MAIN_MIN_WIDTH: f32 = 384.0;
const SUBAGENT_PANEL_DEFAULT_WIDTH: f32 = 603.0;
const SUBAGENT_PANEL_HEADER_HEIGHT: f32 = 48.0;
const MAIN_CONTENT_HORIZONTAL_GUTTER: f32 = 24.0;
// The native 14px traffic lights start at y=18px, so their center is y=25px.
// Center the 28px leading titlebar controls on that same horizontal axis.
const LEADING_TITLEBAR_CONTROLS_TOP: f32 = 11.0;
const STARTUP_LOADING_LOGO_SIZE: f32 = 48.0;
const STARTUP_LOADING_BLINK_DURATION: Duration = Duration::from_millis(1_200);
const STARTUP_LOADING_MINIMUM_DURATION: Duration = Duration::from_secs(1);

const SIDEBAR_TRANSITION_DURATION: Duration = Duration::from_millis(400);

fn sidebar_transition_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // chat-reference: --cubic-enter: cubic-bezier(.19, 1, .22, 1)
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.19, 0.22) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 1.0, 1.0)
}

fn right_panel_width_limit(viewport_width: f32, revealed_sidebar_width: f32) -> f32 {
    (viewport_width - revealed_sidebar_width - RIGHT_PANEL_MAIN_MIN_WIDTH)
        .max(RIGHT_PANEL_MIN_WIDTH)
}

fn clamp_right_panel_width(width: f32, viewport_width: f32, revealed_sidebar_width: f32) -> f32 {
    width.clamp(
        RIGHT_PANEL_MIN_WIDTH,
        right_panel_width_limit(viewport_width, revealed_sidebar_width),
    )
}

fn panel_resize_handle(
    id: &'static str,
    left: f32,
    line_visible: bool,
    theme: Theme,
    input_layer: impl IntoElement,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(left))
        .w(px(16.0))
        .cursor_col_resize()
        .child(input_layer)
        .when(line_visible, |handle| {
            handle.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(7.5))
                    .w(px(1.0))
                    .flex()
                    .flex_col()
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.0), 0.0),
                        linear_color_stop(theme.text.alpha(0.25), 1.0),
                    )))
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.25), 0.0),
                        linear_color_stop(theme.text.alpha(0.0), 1.0),
                    ))),
            )
        })
}
fn titlebar_interaction_area() -> impl IntoElement {
    div()
        .id("titlebar-interaction-area")
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(46.0))
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.zoom_window();
            }
        })
}

fn startup_loading_logo_opacity(progress: f32) -> f32 {
    let blink = ((progress.clamp(0.0, 1.0) * std::f32::consts::TAU).cos() + 1.0) * 0.5;
    0.32 + blink * 0.68
}

#[cfg(not(test))]
fn startup_sidebar_resolved(snapshot: &WorkspaceSnapshot) -> bool {
    !snapshot.loading.projects && !snapshot.loading.recent && !snapshot.loading.pinned
}

fn startup_loading_view(theme: Theme) -> impl IntoElement {
    let logo = icon("home-mark", theme.home_mark.into())
        .size(px(STARTUP_LOADING_LOGO_SIZE))
        .with_animation(
            "startup-loading-logo-blink",
            Animation::new(STARTUP_LOADING_BLINK_DURATION).repeat(),
            |logo, progress| logo.opacity(startup_loading_logo_opacity(progress)),
        );

    div()
        .id("startup-loading-screen")
        .role(Role::ProgressIndicator)
        .aria_label("GPUI 正在加载")
        .size_full()
        .relative()
        .child(
            div()
                .size_full()
                .bg(theme.sidebar_surface)
                .flex()
                .items_center()
                .justify_center()
                .child(logo),
        )
        .child(titlebar_interaction_area())
}

fn titlebar_icon_button(
    name: &'static str,
    disabled: bool,
    active: bool,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let glyph = icon(name, theme.text_tertiary.into())
        .size(px(16.0))
        .when(name == "right-sidebar", |glyph| {
            glyph.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
        });

    div()
        .id(name)
        .size(px(28.0))
        .flex_none()
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .when(active, |button| button.bg(theme.text.alpha(0.05)))
        .when(disabled, |button| button.opacity(0.4).cursor_default())
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
                .active(move |style| style.bg(theme.sidebar_hover))
        })
        // The reference SVG declares 20x20, but its `icon-xs` class wins in
        // computed style and renders the glyph at 16x16.
        .child(glyph)
}

impl ChatApp {
    pub fn new(mode: ThemeMode, scroll_sidebar_to_bottom: bool, cx: &mut Context<Self>) -> Self {
        let codex_app_server = Arc::new(CodexAppServerManager::new());
        let agent_backend: Arc<dyn AgentBackend> = Arc::new(CodexAppServerBackend::with_manager(
            codex_app_server.clone(),
        ));
        let workspace_store = WorkspaceStore::new(agent_backend.clone());
        let sidebar = cx.new(|cx| {
            SidebarView::new(mode, scroll_sidebar_to_bottom, workspace_store.clone(), cx)
        });
        #[cfg(not(test))]
        workspace_store.refresh_all();
        #[cfg(not(test))]
        let workspace_receiver = workspace_store.subscribe();
        let settings = cx.new(|_| SettingsView::new(mode));
        let home = cx.new(|cx| HomeView::new_with_backend(mode, agent_backend.clone(), cx));
        let initial_composer = home.read(cx).composer_entity();
        let initial_cwd = std::env::current_dir().unwrap_or_default();
        initial_composer.update(cx, |composer, cx| {
            composer.set_workspace_context(initial_cwd.clone(), None, None, cx);
        });
        let active_conversation = ConversationKey::Draft(DraftId(1));
        let mut conversation_hosts = HashMap::new();
        conversation_hosts.insert(
            active_conversation.clone(),
            ConversationHost {
                composer: initial_composer,
                cwd: initial_cwd,
                project_id: None,
            },
        );
        cx.subscribe(&sidebar, |this, _, _: &OpenSettings, cx| {
            this.showing_settings = true;
            cx.notify();
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, _: &OpenProjectCreation, cx| {
            this.open_project_creation(cx);
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, event: &SelectThread, cx| {
            this.select_conversation(event.thread_id.clone(), cx);
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, event: &NewConversation, cx| {
            this.start_draft(event.project_id.clone(), event.cwd.clone(), cx);
        })
        .detach();
        cx.subscribe(&settings, |this, _, _: &CloseSettings, cx| {
            this.showing_settings = false;
            cx.notify();
        })
        .detach();
        cx.subscribe(&settings, |this, _, event: &ChangeTheme, cx| {
            this.mode = event.0;
            for panel in this.terminal_panels.values() {
                panel.update(cx, |panel, cx| panel.set_mode(event.0, cx));
            }
            cx.set_window_appearance(Some(match event.0 {
                ThemeMode::Light => WindowAppearance::Light,
                ThemeMode::Dark => WindowAppearance::Dark,
            }));
            this.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_mode(event.0, cx);
            });
            this.home.update(cx, |home, cx| {
                home.set_mode(event.0, cx);
            });
            for host in this.conversation_hosts.values() {
                host.composer.update(cx, |composer, cx| {
                    composer.set_mode(event.0, cx);
                });
            }
            if let Some(panel) = &this.subagent_panel {
                panel.home.update(cx, |home, cx| home.set_mode(event.0, cx));
            }
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &RequestFullAccessConfirmation, cx| {
            this.permission_confirmation_open = true;
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &ModelCatalogLoadFinished, cx| {
            this.startup_model_catalog_resolved = true;
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenDiffReview, cx| {
            this.open_diff_review(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenImagePreview, cx| {
            this.image_preview = Some(event.0.clone());
            this.image_preview_dimensions = generated_image_dimensions(&event.0).ok().flatten();
            this.image_preview_zoom = 1.0;
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &OpenSubAgentPanel, cx| {
            this.open_subagent_panel(event.clone(), cx);
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &RetryImageGeneration, cx| {
            if let Some(host) = this.conversation_hosts.get(&this.active_conversation) {
                host.composer
                    .update(cx, |composer, cx| composer.retry_image_generation(cx));
            }
        })
        .detach();
        cx.subscribe(&home, |this, _, event: &ConversationThreadCreated, cx| {
            this.rekey_created_thread(event.thread_id.clone(), cx);
        })
        .detach();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(STARTUP_LOADING_MINIMUM_DURATION)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.startup_minimum_duration_elapsed = true;
                cx.notify();
            });
        })
        .detach();
        #[cfg(not(test))]
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = workspace_receiver.recv().await {
                if startup_sidebar_resolved(&snapshot) {
                    let _ = this.update(cx, |this, cx| {
                        this.startup_sidebar_resolved = true;
                        cx.notify();
                    });
                    break;
                }
            }
        })
        .detach();
        Self {
            codex_app_server,
            _agent_backend: agent_backend,
            workspace_store,
            conversation_hosts,
            active_conversation,
            next_draft_id: 2,
            mode,
            // Unit tests intentionally exercise the full shell without spawning
            // the external Codex model-catalog process.
            startup_model_catalog_resolved: cfg!(test),
            startup_sidebar_resolved: cfg!(test),
            startup_minimum_duration_elapsed: cfg!(test),
            sidebar,
            home,
            settings,
            showing_settings: false,
            sidebar_collapsed: false,
            sidebar_resize_hovered: false,
            sidebar_resize_dragging: false,
            sidebar_resize_pointer_offset: 0.0,
            sidebar_reveal: 1.0,
            sidebar_animation_from: 1.0,
            sidebar_animation_to: 1.0,
            sidebar_animation_started_at: None,
            sidebar_animation_duration: Duration::ZERO,
            sidebar_animation_running: false,
            bottom_panel_open: false,
            bottom_panel_tabs: Vec::new(),
            bottom_panel_active_tab: None,
            bottom_panel_hovered_tab: None,
            bottom_panel_add_menu_open: false,
            bottom_panel_focused_item: 0,
            bottom_panel_keyboard_focus: false,
            bottom_panel_focus: cx.focus_handle().tab_stop(true),
            bottom_panel_focus_pending: false,
            terminal_panels: HashMap::new(),
            terminal_return_focus_pending: false,
            right_panel_open: false,
            right_panel_mode: None,
            right_panel_focused_item: 0,
            right_panel_keyboard_focus: false,
            right_panel_focus: cx.focus_handle().tab_stop(true),
            right_panel_focus_pending: false,
            right_panel_width: None,
            right_panel_resize_hovered: false,
            right_panel_resize_dragging: false,
            right_panel_resize_pointer_offset: 0.0,
            subagent_panel: None,
            subagent_panel_menu_open: false,
            diff_review: None,
            image_preview: None,
            image_preview_focus: cx.focus_handle(),
            image_preview_focus_active: false,
            image_preview_previous_focus: None,
            image_preview_dimensions: None,
            image_preview_zoom: 1.0,
            permission_confirmation_open: false,
            project_creation_open: false,
            project_creation_kind: ProjectCreationKind::Local,
            project_creation_step: ProjectCreationStep::Kind,
            project_creation_focused_item: 0,
            project_creation_keyboard_focus: false,
            project_creation_focus: cx.focus_handle().tab_stop(true),
            project_creation_focus_pending: false,
        }
    }

    fn switch_home_to(&mut self, key: ConversationKey, cx: &mut Context<Self>) {
        let Some(host) = self.conversation_hosts.get(&key) else {
            return;
        };
        let composer = host.composer.clone();
        let cwd = host.cwd.clone();
        let project_id = host.project_id.clone();
        let thread_id = match &key {
            ConversationKey::Draft(_) => None,
            ConversationKey::Thread(thread_id) => Some(thread_id.clone()),
        };
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd, project_id, thread_id, cx);
        });
        self.active_conversation = key;
        if self.right_panel_open && self.right_panel_mode == Some(RightPanelMode::Terminal) {
            self.ensure_terminal(cx);
        }
        self.home
            .update(cx, |home, cx| home.set_composer(composer, cx));
        cx.notify();
    }

    fn start_draft(&mut self, project_id: Option<ProjectId>, cwd: PathBuf, cx: &mut Context<Self>) {
        let draft_id = DraftId(self.next_draft_id);
        self.next_draft_id = self.next_draft_id.wrapping_add(1).max(1);
        let backend = self._agent_backend.clone();
        let composer = cx.new(|cx| ComposerView::new_with_backend(self.mode, backend, cx));
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd.clone(), project_id.clone(), None, cx);
        });
        let key = ConversationKey::Draft(draft_id);
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer,
                cwd,
                project_id,
            },
        );
        self.switch_home_to(key, cx);
    }

    fn ensure_thread_conversation(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) -> Entity<ComposerView> {
        let key = ConversationKey::Thread(thread_id.clone());
        if let Some(host) = self.conversation_hosts.get(&key) {
            let composer = host.composer.clone();
            let retry_history = composer.read(cx).history_needs_retry();
            if retry_history {
                composer.update(cx, |composer, cx| composer.set_history_loading(true, cx));
                self.load_conversation_history(
                    ConversationKey::Thread(thread_id.clone()),
                    thread_id,
                    composer.clone(),
                    cx,
                );
            }
            return composer;
        }

        let snapshot = self.workspace_store.snapshot();
        let summary = snapshot.thread(&thread_id).cloned().or_else(|| {
            snapshot
                .search_results
                .iter()
                .find(|result| result.thread.thread_id == thread_id)
                .map(|result| result.thread.clone())
        });
        let cwd = summary
            .as_ref()
            .map(|thread| thread.cwd.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let project_id = summary
            .as_ref()
            .and_then(|thread| project_id_for_thread(thread, &snapshot.projects));
        let backend = self._agent_backend.clone();
        let composer = cx.new(|cx| ComposerView::new_with_backend(self.mode, backend, cx));
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(
                cwd.clone(),
                project_id.clone(),
                Some(thread_id.clone()),
                cx,
            );
            composer.set_history_loading(true, cx);
        });
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer: composer.clone(),
                cwd,
                project_id,
            },
        );

        self.load_conversation_history(
            ConversationKey::Thread(thread_id.clone()),
            thread_id,
            composer.clone(),
            cx,
        );
        composer
    }

    fn select_conversation(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let key = ConversationKey::Thread(thread_id.clone());
        self.ensure_thread_conversation(thread_id, cx);
        self.switch_home_to(key, cx);
    }

    fn open_subagent_panel(&mut self, event: OpenSubAgentPanel, cx: &mut Context<Self>) {
        let composer = self.ensure_thread_conversation(event.thread_id.clone(), cx);
        let panel_home = cx.new(|cx| HomeView::new_subagent(self.mode, composer.clone(), cx));

        cx.subscribe(&panel_home, |this, _, nested: &OpenSubAgentPanel, cx| {
            this.open_subagent_panel(nested.clone(), cx)
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, preview: &OpenImagePreview, cx| {
            this.image_preview = Some(preview.0.clone());
            this.image_preview_dimensions = generated_image_dimensions(&preview.0).ok().flatten();
            this.image_preview_zoom = 1.0;
            cx.notify();
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, review: &OpenDiffReview, cx| {
            this.open_diff_review(review.0.clone(), cx)
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, _: &RetryImageGeneration, cx| {
            if let Some(panel) = &this.subagent_panel {
                let composer = panel.home.read(cx).composer_entity();
                composer.update(cx, |composer, cx| composer.retry_image_generation(cx));
            }
        })
        .detach();

        self.subagent_panel = Some(SubagentPanel {
            thread_id: event.thread_id,
            name: event.name,
            home: panel_home,
        });
        self.subagent_panel_menu_open = false;
        self.right_panel_open = true;
        self.right_panel_mode = None;
        self.diff_review = None;
        if self.right_panel_width.is_none() {
            self.right_panel_width = Some(SUBAGENT_PANEL_DEFAULT_WIDTH);
        }
        self.right_panel_keyboard_focus = false;
        self.right_panel_focus_pending = true;
        cx.notify();
    }

    /// Opens a persisted thread without requiring the sidebar to finish loading first.
    ///
    /// This is used by the deterministic Markdown capture path. It deliberately
    /// follows the same history-loading path as a real sidebar selection.
    pub fn resume_thread_for_capture(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.select_thread_for_capture(thread_id.clone(), cx)
        });
        self.select_conversation(thread_id, cx);
    }

    /// Returns whether the requested thread has finished hydrating, or its load error.
    #[cfg(feature = "screenshot")]
    pub fn resumed_thread_ready(&self, thread_id: &str, cx: &gpui::App) -> Result<bool, String> {
        let sidebar_ready = self
            .sidebar
            .read(cx)
            .resumed_thread_ready_for_capture(thread_id)?;
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed
            && sidebar_ready)
        {
            return Ok(false);
        }
        let key = ConversationKey::Thread(thread_id.to_owned());
        let Some(host) = self.conversation_hosts.get(&key) else {
            return Ok(false);
        };
        let composer = host.composer.read(cx);
        if let Some(error) = composer.history_error() {
            return Err(error.to_owned());
        }
        Ok(!composer.history_loading()
            && composer.thread_id() == Some(thread_id)
            && composer.model_catalog_ready_for_capture()?)
    }

    #[cfg(feature = "screenshot")]
    pub fn resumed_render_audit(&self, thread_id: &str, cx: &gpui::App) -> serde_json::Value {
        let composer = self.conversation_hosts[&ConversationKey::Thread(thread_id.to_owned())]
            .composer
            .read(cx);
        let mut turns = composer
            .transcript_render_snapshot()
            .into_iter()
            .map(|turn| {
                serde_json::json!({
                    "id":turn.resumed.map(|r|r.id), "user_message":turn.user_message,
                    "units":crate::components::home::resumed_activity_audit(&turn.activities)
                })
            })
            .collect::<Vec<_>>();
        let (_, user, _, _, _, activities) = composer.conversation_render_snapshot();
        turns.push(
            serde_json::json!({"id":composer.resumed_turn().map(|r|r.id), "user_message":user,
            "units":crate::components::home::resumed_activity_audit(&activities)}),
        );
        serde_json::json!({"thread_id":thread_id,"turns":turns})
    }

    #[cfg(feature = "screenshot")]
    pub fn set_conversation_scroll_from_bottom_for_capture(
        &mut self,
        distance: f32,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_conversation_scroll_from_bottom_for_capture(distance, cx)
        });
    }

    fn load_conversation_history(
        &mut self,
        key: ConversationKey,
        thread_id: ThreadId,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        let receiver = self.workspace_store.load_history(thread_id);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::agent::WorkspaceError::backend(
                    "读取聊天历史的响应通道提前关闭",
                ))
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(history) => {
                    if let Some(host) = this.conversation_hosts.get_mut(&key)
                        && host.composer == composer
                    {
                        host.cwd = history.thread.cwd.clone();
                        host.project_id = history.thread.project_id.clone();
                    }
                    composer.update(cx, |composer, cx| composer.hydrate_history(history, cx));
                }
                Err(error) => composer.update(cx, |composer, cx| {
                    composer.set_history_error(error.user_message("读取聊天历史"), cx)
                }),
            });
        })
        .detach();
    }

    fn rekey_created_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let draft_key = self.conversation_hosts.iter().find_map(|(key, host)| {
            matches!(key, ConversationKey::Draft(_))
                .then(|| {
                    (host.composer.read(cx).thread_id() == Some(thread_id.as_str()))
                        .then(|| key.clone())
                })
                .flatten()
        });
        let Some(draft_key) = draft_key else {
            return;
        };
        let Some(host) = self.conversation_hosts.remove(&draft_key) else {
            return;
        };
        let real_key = ConversationKey::Thread(thread_id);
        if self.active_conversation == draft_key {
            self.active_conversation = real_key.clone();
        }
        if let Some(panel) = self.terminal_panels.remove(&draft_key) {
            self.terminal_panels.insert(real_key.clone(), panel);
        }
        self.conversation_hosts.insert(real_key, host);
        #[cfg(not(test))]
        self.workspace_store.refresh_all();
        cx.notify();
    }

    pub fn complete_startup_for_capture(&mut self, cx: &mut Context<Self>) {
        self.startup_model_catalog_resolved = true;
        self.startup_sidebar_resolved = true;
        self.startup_minimum_duration_elapsed = true;
        cx.notify();
    }

    pub fn open_profile_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_profile_menu_open(true, cx));
    }

    pub fn open_activity(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_activity_open(true, cx));
    }

    pub fn open_projects_section_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_projects_section_menu_for_capture(cx)
        });
    }

    pub fn open_project_menu_for_capture(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_project_menu_for_capture(project_id, cx)
        });
    }

    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_scroll_for_capture(offset, cx)
        });
    }

    pub fn set_activity_hovered_thread_for_capture(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_hovered_thread_for_capture(thread_id, cx)
        });
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.showing_settings = true;
        cx.notify();
    }

    pub fn open_settings_page(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.select(slug, cx));
        self.open_settings(cx);
    }

    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.open_model_picker(cx));
    }

    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_model_picker_submenu(name, cx));
    }

    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.open_model_picker_slider_at(index, fast, cx)
        });
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_dictation_state_for_capture(state, cx)
        });
    }

    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        if self.startup_model_catalog_resolved {
            self.home
                .update(cx, |home, cx| home.submit_prompt_for_capture(prompt, cx));
            return;
        }

        let prompt = prompt.to_owned();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                match this.update(cx, |this, cx| {
                    if !this.startup_model_catalog_resolved {
                        return false;
                    }
                    this.home
                        .update(cx, |home, cx| home.submit_prompt_for_capture(&prompt, cx));
                    true
                }) {
                    Ok(true) | Err(_) => return,
                    Ok(false) => {}
                }
            }
        })
        .detach();
    }

    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.show_user_message_actions_for_capture(cx)
        });
    }

    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_command_tool_for_capture(running, expanded, cx)
        });
    }

    pub fn set_context_compaction_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_context_compaction_for_capture(running, cx)
        });
    }

    pub fn set_collaboration_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_collaboration_for_capture(state, cx));
    }

    pub fn set_mcp_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_mcp_tool_call_for_capture(state, cx));
    }

    pub fn set_tool_group_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_tool_group_for_capture(running, expanded, cx)
        });
    }

    pub fn set_reasoning_for_capture(
        &mut self,
        state: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_reasoning_for_capture(state, expanded, cx)
        });
    }

    pub fn set_approval_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_approval_for_capture(kind, state, cx)
        });
    }

    pub fn set_user_input_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_user_input_for_capture(state, cx));
    }

    pub fn set_file_approval_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_approval_for_capture(state, cx));
    }

    pub fn set_permissions_approval_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_permissions_approval_for_capture(kind, state, cx)
        });
    }

    pub fn set_file_change_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_change_for_capture(state, cx));
    }

    pub fn set_turn_diff_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_file_change_for_capture("completed", cx)
        });
        self.open_diff_review(captured_diff_review_fixture(state), cx);
    }

    fn open_diff_review(&mut self, review: DiffReviewPresentation, cx: &mut Context<Self>) {
        self.diff_review = Some(review);
        self.subagent_panel = None;
        self.subagent_panel_menu_open = false;
        self.right_panel_open = true;
        self.right_panel_mode = None;
        // Natural long-diff capture 29: the 2560px viewport split begins at
        // x=1202.359375, leaving a 1357.640625px Review panel. Its 250px file
        // tree leaves the measured 1107.640625px scroll viewport.
        self.right_panel_width = Some(1_357.640_6);
        self.right_panel_keyboard_focus = false;
        self.right_panel_focus_pending = false;
        cx.notify();
    }

    pub fn set_image_generation_for_capture(
        &mut self,
        state: &str,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_image_generation_for_capture(state, path, cx)
        });
        cx.notify();
    }

    fn handle_diff_review_event(&mut self, event: DiffReviewEvent, cx: &mut Context<Self>) {
        match event {
            DiffReviewEvent::Close => {
                self.diff_review = None;
                self.right_panel_open = false;
            }
            DiffReviewEvent::ToggleFile(index) => {
                let Some(file) = self
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                file.visual_state = if file.visual_state.is_expanded() {
                    DiffFileVisualState::Collapsed
                } else {
                    DiffFileVisualState::Expanded
                };
            }
            DiffReviewEvent::HeaderHoverChanged { index, hovered } => {
                let Some(file) = self
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                if file.visual_state.is_expanded() {
                    file.visual_state = if hovered {
                        DiffFileVisualState::HeaderHovered
                    } else {
                        DiffFileVisualState::Expanded
                    };
                }
            }
            DiffReviewEvent::CopyPath(path) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(path));
            }
            DiffReviewEvent::OpenLocation(path) => {
                // Keep this as a native OS action. `Command::arg` passes the
                // path as data (rather than through a shell), so spaces and
                // other filename characters cannot be interpreted as code.
                let _ = finder_reveal_command(&path).spawn();
            }
        }
        cx.notify();
    }

    pub fn enable_permission_ui_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.enable_permission_ui_for_capture(cx));
    }

    pub fn set_permission_mode_for_capture(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_mode_for_capture(mode, cx)
        });
    }

    pub fn open_permission_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_permission_menu_for_capture(cx));
    }

    pub fn set_permission_menu_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_menu_capture_state(state, cx)
        });
    }

    pub fn open_permission_confirmation_for_capture(&mut self, cx: &mut Context<Self>) {
        self.enable_permission_ui_for_capture(cx);
        self.permission_confirmation_open = true;
        cx.notify();
    }

    pub fn open_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_open = true;
        self.project_creation_focused_item = 0;
        self.project_creation_keyboard_focus = false;
        self.project_creation_focus_pending = true;
        self.permission_confirmation_open = false;
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.close_transient_menus(cx);
        });
        cx.notify();
    }

    pub fn open_project_creation_remote_for_capture(&mut self, cx: &mut Context<Self>) {
        self.open_project_creation(cx);
        self.project_creation_kind = ProjectCreationKind::Remote;
        self.project_creation_step = ProjectCreationStep::Remote;
        cx.notify();
    }

    fn close_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_open = false;
        self.project_creation_keyboard_focus = false;
        self.project_creation_focus_pending = false;
        cx.notify();
    }

    fn cancel_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_kind = ProjectCreationKind::Local;
        self.project_creation_step = ProjectCreationStep::Kind;
        self.close_project_creation(cx);
    }

    fn advance_project_creation(&mut self, cx: &mut Context<Self>) {
        match self.project_creation_kind {
            ProjectCreationKind::Local => {
                self.close_project_creation(cx);
                let paths = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: None,
                });
                #[cfg(not(test))]
                {
                    let workspace_store = self.workspace_store.clone();
                    cx.spawn(async move |_, _cx| {
                        let Ok(Ok(Some(mut paths))) = paths.await else {
                            return;
                        };
                        let Some(path) = paths.pop() else {
                            return;
                        };
                        workspace_store.create_project(path);
                    })
                    .detach();
                }
                #[cfg(test)]
                drop(paths);
            }
            ProjectCreationKind::Remote => {
                self.project_creation_step = ProjectCreationStep::Remote;
                self.project_creation_focused_item = 0;
                self.project_creation_keyboard_focus = false;
                cx.notify();
            }
        }
    }

    fn handle_project_creation_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_preview.is_some() && event.keystroke.key == "escape" {
            self.image_preview = None;
            self.image_preview_dimensions = None;
            self.image_preview_zoom = 1.0;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !self.project_creation_open {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_project_creation(cx);
            cx.stop_propagation();
            return;
        }
        if self.project_creation_step == ProjectCreationStep::Remote {
            if key == "tab" {
                let count = 4;
                self.project_creation_focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation_focused_item + count - 1) % count
                } else {
                    (self.project_creation_focused_item + 1) % count
                };
                self.project_creation_keyboard_focus = true;
                cx.stop_propagation();
                cx.notify();
            } else if matches!(key, "enter" | "space") {
                match self.project_creation_focused_item {
                    2 => self.cancel_project_creation(cx),
                    3 => self.close_project_creation(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }
            return;
        }

        match key {
            "tab" => {
                let count = 4;
                self.project_creation_focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation_focused_item + count - 1) % count
                } else {
                    (self.project_creation_focused_item + 1) % count
                };
                self.project_creation_keyboard_focus = true;
            }
            "enter" | "space" => match self.project_creation_focused_item {
                0 => self.project_creation_kind = ProjectCreationKind::Local,
                1 => self.project_creation_kind = ProjectCreationKind::Remote,
                2 => self.advance_project_creation(cx),
                3 => self.close_project_creation(cx),
                _ => {}
            },
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn download_preview_image(&mut self, source: PathBuf, cx: &mut Context<Self>) {
        let suggested_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image.png")
            .to_owned();
        let initial_directory = source
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let destination = cx.prompt_for_new_path(&initial_directory, Some(suggested_name.as_str()));
        cx.spawn(async move |_, _| {
            let Ok(Ok(Some(destination))) = destination.await else {
                return;
            };
            let _ = std::fs::copy(source, destination);
        })
        .detach();
    }

    pub fn open_bottom_panel(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel_open = true;
        // The real desktop app restores the active terminal tab when the
        // titlebar toggle is used. This clone has no process/session model, so
        // reuse its existing terminal mode without inventing a path or title.
        if self.bottom_panel_tabs.is_empty() {
            self.bottom_panel_tabs.push(BottomPanelMode::Terminal);
            self.bottom_panel_active_tab = Some(0);
        }
        self.bottom_panel_add_menu_open = false;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = false;
        cx.notify();
    }

    fn close_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_open {
            self.bottom_panel_open = false;
            self.bottom_panel_add_menu_open = false;
            self.bottom_panel_keyboard_focus = false;
            self.bottom_panel_focus_pending = false;
            cx.notify();
        }
    }

    fn toggle_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_open {
            self.close_bottom_panel(cx);
        } else {
            self.open_bottom_panel(cx);
        }
    }

    fn close_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_add_menu_open {
            self.bottom_panel_add_menu_open = false;
            self.bottom_panel_keyboard_focus = false;
            self.bottom_panel_focus_pending = false;
            cx.notify();
        }
    }

    fn toggle_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel_add_menu_open = !self.bottom_panel_add_menu_open;
        self.bottom_panel_focused_item = 0;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = self.bottom_panel_add_menu_open;
        cx.notify();
    }

    pub fn open_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        self.bottom_panel_add_menu_open = true;
        self.bottom_panel_focused_item = 0;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = true;
        cx.notify();
    }

    pub fn append_bottom_panel_item_for_capture(&mut self, name: &str, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        let index = match name {
            "review" => 0,
            "terminal" => 1,
            "browser" => 2,
            "files" => 3,
            "side-chat" => 4,
            _ => return,
        };
        self.select_bottom_panel_item(index, cx);
    }

    fn select_bottom_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = BOTTOM_PANEL_ITEMS.get(index) else {
            return;
        };
        self.bottom_panel_tabs.push(*mode);
        self.bottom_panel_active_tab = Some(self.bottom_panel_tabs.len() - 1);
        self.bottom_panel_hovered_tab = None;
        self.bottom_panel_add_menu_open = false;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = false;
        cx.notify();
    }

    fn activate_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.bottom_panel_tabs.len() {
            self.bottom_panel_active_tab = Some(index);
            self.close_bottom_panel_menu(cx);
            cx.notify();
        }
    }

    fn close_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.bottom_panel_tabs.len() {
            return;
        }
        self.bottom_panel_tabs.remove(index);
        self.bottom_panel_hovered_tab = None;
        self.bottom_panel_active_tab = match self.bottom_panel_active_tab {
            None => None,
            Some(_) if self.bottom_panel_tabs.is_empty() => None,
            Some(active) if active == index => Some(index.min(self.bottom_panel_tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active),
        };
        self.close_bottom_panel_menu(cx);
        cx.notify();
    }

    fn handle_bottom_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.bottom_panel_add_menu_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" => {
                self.bottom_panel_focused_item = if self.bottom_panel_keyboard_focus {
                    (self.bottom_panel_focused_item + 1) % BOTTOM_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.bottom_panel_keyboard_focus = true;
            }
            "up" => {
                self.bottom_panel_focused_item = if self.bottom_panel_keyboard_focus {
                    (self.bottom_panel_focused_item + BOTTOM_PANEL_ITEMS.len() - 1)
                        % BOTTOM_PANEL_ITEMS.len()
                } else {
                    BOTTOM_PANEL_ITEMS.len() - 1
                };
                self.bottom_panel_keyboard_focus = true;
            }
            // Radix keeps focus inside the open dropdown and ignores Tab.
            "tab" => {
                cx.stop_propagation();
                return;
            }
            "home" => {
                self.bottom_panel_focused_item = 0;
                self.bottom_panel_keyboard_focus = true;
            }
            "end" => {
                self.bottom_panel_focused_item = BOTTOM_PANEL_ITEMS.len() - 1;
                self.bottom_panel_keyboard_focus = true;
            }
            "enter" | "space" => {
                self.select_bottom_panel_item(self.bottom_panel_focused_item, cx);
            }
            "escape" => self.close_bottom_panel_menu(cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn ensure_terminal(&mut self, cx: &mut Context<Self>) {
        self.terminal_return_focus_pending = false;
        let key = self.active_conversation.clone();
        if !self.terminal_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|host| host.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| TerminalPanel::new(cwd, self.mode, cx));
            self.terminal_panels.insert(key.clone(), panel);
        }
        self.terminal_panels[&key].update(cx, |panel, cx| panel.focus(cx));
        self.right_panel_focus_pending = false;
    }

    pub fn open_right_panel(&mut self, cx: &mut Context<Self>) {
        self.right_panel_open = true;
        if self.right_panel_mode == Some(RightPanelMode::Terminal) {
            self.ensure_terminal(cx);
            cx.notify();
            return;
        }
        self.right_panel_mode = None;
        self.subagent_panel = None;
        self.subagent_panel_menu_open = false;
        self.diff_review = None;
        self.right_panel_focused_item = 0;
        self.right_panel_keyboard_focus = false;
        self.right_panel_focus_pending = true;
        cx.notify();
    }

    fn close_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel_open {
            self.right_panel_open = false;
            self.terminal_return_focus_pending =
                self.right_panel_mode == Some(RightPanelMode::Terminal);
            if self.right_panel_mode != Some(RightPanelMode::Terminal) {
                self.right_panel_mode = None;
            }
            self.subagent_panel = None;
            self.subagent_panel_menu_open = false;
            self.diff_review = None;
            self.right_panel_keyboard_focus = false;
            self.right_panel_focus_pending = false;
            self.right_panel_resize_hovered = false;
            self.right_panel_resize_dragging = false;
            cx.notify();
        }
    }

    fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel_open {
            self.close_right_panel(cx);
        } else {
            self.open_right_panel(cx);
        }
    }

    fn select_right_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = RIGHT_PANEL_ITEMS.get(index) else {
            return;
        };
        self.right_panel_mode = Some(*mode);
        if *mode == RightPanelMode::Terminal {
            self.ensure_terminal(cx);
        }
        self.subagent_panel = None;
        self.subagent_panel_menu_open = false;
        self.diff_review = None;
        self.right_panel_keyboard_focus = false;
        cx.notify();
    }

    fn handle_right_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.right_panel_open {
            return;
        }
        if self.subagent_panel.is_some() {
            match event.keystroke.key.as_str() {
                "escape" => {
                    if self.subagent_panel_menu_open {
                        self.subagent_panel_menu_open = false;
                        cx.notify();
                    } else {
                        self.close_right_panel(cx);
                    }
                    cx.stop_propagation();
                }
                _ => return,
            }
            return;
        }
        match event.keystroke.key.as_str() {
            "down" | "tab" if !event.keystroke.modifiers.shift => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + 1) % RIGHT_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.right_panel_keyboard_focus = true;
            }
            "up" => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel_keyboard_focus = true;
            }
            "tab" => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel_keyboard_focus = true;
            }
            "home" => {
                self.right_panel_focused_item = 0;
                self.right_panel_keyboard_focus = true;
            }
            "end" => {
                self.right_panel_focused_item = RIGHT_PANEL_ITEMS.len() - 1;
                self.right_panel_keyboard_focus = true;
            }
            "enter" | "space" if self.right_panel_mode.is_none() => {
                self.select_right_panel_item(self.right_panel_focused_item, cx);
            }
            // The docked panel is persistent. Escape belongs to the active
            // conversation/tool and must not hide the panel.
            "escape" => return,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_resize_hovered = false;
        self.sidebar_resize_dragging = false;
        self.sidebar_collapsed = !self.sidebar_collapsed;
        let target = if self.sidebar_collapsed { 0.0 } else { 1.0 };

        if cx.reduce_motion() || (target - self.sidebar_reveal).abs() <= f32::EPSILON {
            self.sidebar_reveal = target;
            self.sidebar_animation_from = target;
            self.sidebar_animation_to = target;
            self.sidebar_animation_started_at = None;
            self.sidebar_animation_duration = Duration::ZERO;
            self.sidebar_animation_running = false;
            cx.notify();
            return;
        }

        let was_running = self.sidebar_animation_running;
        self.sidebar_animation_from = self.sidebar_reveal;
        self.sidebar_animation_to = target;
        self.sidebar_animation_started_at = Some(cx.background_executor().now());
        self.sidebar_animation_duration = Duration::from_secs_f32(
            SIDEBAR_TRANSITION_DURATION.as_secs_f32() * (target - self.sidebar_reveal).abs(),
        );
        self.sidebar_animation_running = true;
        cx.notify();

        if !was_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }

    fn advance_sidebar_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sidebar_animation_running {
            return;
        }

        let elapsed = self
            .sidebar_animation_started_at
            .map_or(Duration::ZERO, |started| {
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started)
            });
        let progress = if self.sidebar_animation_duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f32() / self.sidebar_animation_duration.as_secs_f32()
        }
        .clamp(0.0, 1.0);

        self.sidebar_reveal = self.sidebar_animation_from
            + (self.sidebar_animation_to - self.sidebar_animation_from)
                * sidebar_transition_ease(progress);

        if progress >= 1.0 || cx.reduce_motion() {
            self.sidebar_reveal = self.sidebar_animation_to;
            self.sidebar_animation_running = false;
            self.sidebar_animation_started_at = None;
        }
        cx.notify();

        if self.sidebar_animation_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }
}

fn finder_reveal_command(path: &str) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

fn permission_risk_row(
    icon_name: &'static str,
    title: &'static str,
    detail: &'static str,
    separated: bool,
    theme: Theme,
) -> Div {
    div()
        .h(px(51.0))
        .mx(px(16.0))
        .when(separated, |row| row.border_t_1().border_color(theme.border))
        .flex()
        .items_center()
        // Preserve the reference's authored multicolor fills.
        .child(gpui::img(format!("icons/{icon_name}.svg")).size(px(24.0)))
        .child(
            div()
                .ml(px(12.0))
                .flex_1()
                .flex()
                .flex_col()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(div().text_color(theme.text).child(title))
                .child(div().text_color(theme.text_tertiary).child(detail)),
        )
}

fn project_creation_focus_shadow(theme: Theme, visible: bool) -> Vec<BoxShadow> {
    if visible {
        vec![
            BoxShadow::new(px(0.0), px(0.0), theme.accent.alpha(0.76).into())
                .spread_radius(px(2.0)),
        ]
    } else {
        Vec::new()
    }
}

impl ChatApp {
    fn project_kind_card(
        &self,
        index: usize,
        kind: ProjectCreationKind,
        glyph: &'static str,
        label: &'static str,
        detail: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.project_creation_kind == kind;
        let keyboard_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index;
        let radio = div()
            .size(px(20.0))
            .flex_none()
            .rounded_full()
            .border_1()
            .border_color(if selected { theme.accent } else { theme.text })
            .flex()
            .items_center()
            .justify_center()
            .when(selected, |radio| {
                radio.child(div().size(px(12.0)).rounded_full().bg(theme.accent))
            });

        div()
            .id(("project-creation-kind", index))
            .w(px(314.0))
            .h(px(144.0))
            .p(px(16.0))
            .rounded(px(20.0))
            .border_1()
            .border_color(if selected {
                rgba(0x00000000)
            } else {
                theme.border
            })
            .bg(if selected {
                theme.text.alpha(0.05)
            } else {
                rgba(0x00000000)
            })
            .shadow(project_creation_focus_shadow(theme, keyboard_focused))
            .flex()
            .flex_col()
            .justify_between()
            .cursor_pointer()
            .hover(move |style| {
                if selected {
                    style.bg(theme.text.alpha(0.05))
                } else {
                    style.bg(theme.text.alpha(0.03))
                }
            })
            .active(move |style| {
                if selected {
                    style.bg(theme.text.alpha(0.05))
                } else {
                    style.bg(theme.text.alpha(0.03))
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.project_creation_kind = kind;
                this.project_creation_focused_item = index;
                this.project_creation_keyboard_focus = false;
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(icon(glyph, theme.text_tertiary.into()).size(px(20.0)))
                    .child(radio),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .ml(px(1.0))
                    .child(
                        div()
                            .h(px(21.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(
                        div()
                            .h(px(19.25))
                            .text_size(px(14.0))
                            .line_height(px(19.25))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
    }

    fn project_creation_close_button(
        &self,
        index: usize,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index;
        div()
            .id("project-creation-close")
            .absolute()
            .top(px(16.0))
            .right(px(16.0))
            .size(px(24.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(theme.text.alpha(0.8))
            .shadow(project_creation_focus_shadow(theme, keyboard_focused))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.sidebar_hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_project_creation(cx);
            }))
            .child(icon("close-dialog", theme.text.alpha(0.8).into()).size(px(16.0)))
            .child(div().invisible().absolute().child(label))
    }

    fn project_creation_kind_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let next_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == 2;
        div()
            .id("project-creation-dialog")
            .relative()
            .w(px(680.0))
            .h(px(357.796875))
            .rounded(px(25.0))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                    .blur_radius(px(8.0))
                    .spread_radius(px(-2.0)),
            ])
            .font_family(".SystemUIFont")
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .size_full()
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(
                        div()
                            .h(px(28.796875))
                            .relative()
                            .top(px(-1.0))
                            .ml(px(1.0))
                            // CoreText's system Chinese advances are slightly
                            // narrower than Chromium's at the computed 24px.
                            .text_size(px(25.0))
                            .line_height(px(28.8))
                            .font_weight(gpui::FontWeight(500.0))
                            .child("创建项目"),
                    )
                    .child(
                        div()
                            .h(px(189.0))
                            .pt(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .h(px(21.0))
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child("项目类型"),
                            )
                            .child(
                                div()
                                    .h(px(144.0))
                                    .flex()
                                    .gap(px(12.0))
                                    .child(self.project_kind_card(
                                        0,
                                        ProjectCreationKind::Local,
                                        "project-local",
                                        "本地",
                                        "在你的电脑上编辑、运行和测试文件",
                                        theme,
                                        cx,
                                    ))
                                    .child(self.project_kind_card(
                                        1,
                                        ProjectCreationKind::Remote,
                                        "project-remote",
                                        "远程",
                                        "选择已连接计算机上的文件夹",
                                        theme,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div().h(px(44.0)).pt(px(12.0)).flex().justify_end().child(
                            div()
                                .id("project-creation-next")
                                .h(px(32.0))
                                .px(px(16.0))
                                .rounded(px(12.5))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.button)
                                .text_color(theme.button_text)
                                .shadow(project_creation_focus_shadow(theme, next_focused))
                                .flex()
                                .items_center()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.text.alpha(0.8)))
                                .active(move |style| style.bg(theme.text.alpha(0.8)))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.project_creation_focused_item = 2;
                                    this.project_creation_keyboard_focus = false;
                                    this.advance_project_creation(cx);
                                }))
                                .child("下一步"),
                        ),
                    ),
            )
            .child(self.project_creation_close_button(3, "关闭", theme, cx))
    }

    fn project_creation_remote_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focus = |index| {
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index
        };
        div()
            .id("project-creation-remote-dialog")
            .relative()
            .w(px(520.0))
            .h(px(331.0))
            .rounded(px(25.0))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                    .blur_radius(px(8.0))
                    .spread_radius(px(-2.0)),
            ])
            .p(px(20.0))
            .font_family(UI_FONT_FAMILY)
            .text_size(px(14.0))
            .line_height(px(21.0))
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .relative()
                    .top(px(0.0))
                    .ml(px(1.0))
                    .font_family(".SystemUIFont")
                    .text_size(px(20.5))
                    .line_height(px(28.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("新建远程项目"),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    .text_color(theme.text_tertiary)
                    .child("先设置远程主机。然后可在此处选择主机和文件夹。"),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .h(px(40.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.control)
                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(0)))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(theme.text_tertiary)
                    .child(
                        div()
                            .size(px(40.0))
                            .border_r_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("folder", theme.text_tertiary.into())),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .child("项目名称"),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("远程主机"),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .h(px(40.0))
                    .px(px(12.0))
                    .rounded(px(15.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.control)
                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(1)))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text_tertiary)
                    .child(div().flex_1().child("没有已连接的远程目标"))
                    .child(icon("chevron-down", theme.text_tertiary.into())),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("源文件夹"),
            )
            .child(
                div()
                    .h(px(68.0))
                    .pt(px(12.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(24.0))
                            .relative()
                            .top(px(3.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .line_height(px(20.0))
                            .text_color(theme.warning)
                            .child(icon("settings-warning", theme.warning.into()))
                            .child("目前没有连接任何远程主机。"),
                    )
                    .child(
                        div()
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .id("project-creation-remote-cancel")
                                    .h(px(32.0))
                                    .px(px(16.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(rgba(0x00000000))
                                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(2)))
                                    .text_color(theme.text_tertiary)
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.sidebar_hover))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.cancel_project_creation(cx);
                                    }))
                                    .child(div().relative().left(px(2.0)).child("取消")),
                            )
                            .child(
                                div()
                                    .h(px(32.0))
                                    .px(px(16.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.button)
                                    .text_color(theme.button_text)
                                    .opacity(0.4)
                                    .flex()
                                    .items_center()
                                    .child("添加项目"),
                            ),
                    ),
            )
            .child(self.project_creation_close_button(3, "关闭对话框", theme, cx))
    }

    fn project_creation_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id("project-creation-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000022))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.project_creation_focus)
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_project_creation(cx);
            }))
            .child(match self.project_creation_step {
                ProjectCreationStep::Kind => self.project_creation_kind_dialog(theme, cx),
                ProjectCreationStep::Remote => self.project_creation_remote_dialog(theme, cx),
            })
    }

    fn bottom_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.bottom_panel_keyboard_focus && self.bottom_panel_focused_item == index;
        div()
            .id(("bottom-panel-menu-item", index))
            .w_full()
            .h(px(28.5625))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .when(focused, |item| item.bg(theme.sidebar_hover))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.sidebar_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.bottom_panel_focused_item = index;
                    this.bottom_panel_keyboard_focus = false;
                    this.bottom_panel_focus.focus(window, cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_bottom_panel_item(index, cx);
            }))
            .child(
                icon(glyph, theme.text.alpha(0.75).into())
                    .size(px(16.0))
                    .flex_none(),
            )
            .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
            .child(
                div()
                    .ml(px(8.0))
                    .flex_none()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text.alpha(0.65))
                    .child(shortcut),
            )
    }

    fn bottom_panel_add_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("bottom-panel-add-menu")
            .absolute()
            .top(px(30.0))
            .left(px(2.0))
            .w(px(280.0))
            .h(px(150.8125))
            .p(px(4.0))
            .rounded(px(15.0))
            .bg(theme.model_picker_surface)
            .border(px(0.5))
            .border_color(theme.border)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .track_focus(&self.bottom_panel_focus)
            .on_key_down(cx.listener(Self::handle_bottom_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .children(BOTTOM_PANEL_ITEMS.iter().enumerate().map(
                |(index, (_, label, shortcut, glyph))| {
                    self.bottom_panel_menu_item(index, label, shortcut, glyph, theme, cx)
                },
            ))
    }

    fn bottom_panel_tab(
        &self,
        index: usize,
        mode: BottomPanelMode,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = self.bottom_panel_active_tab == Some(index);
        let hovered = self.bottom_panel_hovered_tab == Some(index);
        let (label, glyph) = bottom_panel_tab_spec(mode);
        div()
            .id(("bottom-panel-tab-wrapper", index))
            .w(px(160.0))
            .h(px(28.0))
            .flex_none()
            .child(
                div()
                    .id(("bottom-panel-tab", index))
                    .h(px(28.0))
                    .w(px(156.0))
                    .min_w(px(0.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(12.5))
                    .when(active, |tab| tab.bg(theme.text.alpha(0.05)))
                    .hover(move |style| style.bg(theme.text.alpha(0.05)))
                    .active(move |style| style.bg(theme.text.alpha(0.05)))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                        let next = is_hovered.then_some(index);
                        if this.bottom_panel_hovered_tab != next {
                            this.bottom_panel_hovered_tab = next;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.activate_bottom_panel_tab(index, cx);
                    }))
                    .child(
                        icon(
                            glyph,
                            if active {
                                theme.text.into()
                            } else {
                                theme.text_secondary.into()
                            },
                        )
                        .size(px(16.0))
                        .flex_none(),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .text_color(if active {
                                theme.text
                            } else {
                                theme.text_secondary
                            })
                            .child(label),
                    )
                    .child(
                        div()
                            .id(("bottom-panel-close-tab", index))
                            .size(px(20.0))
                            .flex_none()
                            .rounded(px(5.0))
                            .when(!active && !hovered, |close| close.opacity(0.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_bottom_panel_tab(index, cx);
                            }))
                            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0))),
                    ),
            )
    }

    fn bottom_panel(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let toolbar = div()
            .id("bottom-panel-toolbar")
            .h(px(40.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .child(
                div()
                    .id("bottom-panel-tabs")
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .children(
                        self.bottom_panel_tabs
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(index, mode)| self.bottom_panel_tab(index, mode, theme, cx)),
                    ),
            )
            .child(
                div()
                    .id("bottom-panel-add-anchor")
                    .relative()
                    .size(px(28.0))
                    .flex_none()
                    .child(
                        div()
                            .id("bottom-panel-add")
                            .size(px(28.0))
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(self.bottom_panel_add_menu_open, |button| {
                                button.bg(theme.sidebar_hover)
                            })
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .active(move |style| style.bg(theme.sidebar_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_bottom_panel_menu(cx);
                            }))
                            .child(icon("add", theme.text_tertiary.into()).size(px(16.0))),
                    )
                    .when(self.bottom_panel_add_menu_open, |anchor| {
                        anchor.child(deferred(self.bottom_panel_add_menu(theme, cx)))
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("bottom-panel-close")
                    .size(px(28.0))
                    .flex_none()
                    .rounded(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .active(move |style| style.bg(theme.sidebar_hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bottom_panel(cx);
                    }))
                    .child(icon("close-dialog", theme.text_tertiary.into()).size(px(16.0))),
            );

        div()
            .id("bottom-panel")
            .h(px(BOTTOM_PANEL_HEIGHT))
            .min_h(px(BOTTOM_PANEL_HEIGHT))
            .w_full()
            .flex_none()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().min_h(px(0.0)).flex_1().bg(theme.surface))
    }

    fn right_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.right_panel_keyboard_focus && self.right_panel_focused_item == index;
        div()
            .id(("right-panel-menu-item", index))
            .w_full()
            .h(px(40.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.text.alpha(0.03))
            .shadow(project_creation_focus_shadow(theme, focused))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.text.alpha(0.08)))
            .active(move |style| style.bg(theme.text.alpha(0.08)))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.right_panel_focused_item = index;
                    this.right_panel_keyboard_focus = false;
                    this.right_panel_focus.focus(window, cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_right_panel_item(index, cx);
            }))
            .child(
                icon(glyph, theme.text.alpha(0.65).into())
                    .size(px(16.0))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(label),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(16.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .bg(theme.text.alpha(0.065))
                    .text_size(px(12.0))
                    .line_height(px(12.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text.alpha(0.65))
                    .flex()
                    .items_center()
                    .child(shortcut),
            )
    }

    fn sidebar_resize_handle(
        &self,
        theme: Theme,
        width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let line_visible = self.sidebar_resize_hovered || self.sidebar_resize_dragging;
        let input_layer = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let mouse_down_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, _, _, cx| {
                    if event.button != MouseButton::Left || !bounds.contains(&event.position) {
                        return;
                    }
                    mouse_down_entity.update(cx, |this, cx| {
                        let divider_x = f32::from(bounds.origin.x) + 8.0;
                        this.sidebar_resize_dragging = true;
                        this.sidebar_resize_hovered = true;
                        this.sidebar_resize_pointer_offset =
                            divider_x - f32::from(event.position.x);
                        cx.notify();
                    });
                });

                let mouse_move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.sidebar_resize_dragging {
                            let viewport_width = f32::from(window.viewport_size().width);
                            let reserved_right_width = if this.right_panel_open {
                                RIGHT_PANEL_MIN_WIDTH
                            } else {
                                0.0
                            };
                            let max_width = (viewport_width
                                - reserved_right_width
                                - RIGHT_PANEL_MAIN_MIN_WIDTH)
                                .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                            let next_width = (f32::from(event.position.x)
                                + this.sidebar_resize_pointer_offset)
                                .clamp(SIDEBAR_MIN_WIDTH, max_width);
                            this.sidebar
                                .update(cx, |sidebar, cx| sidebar.set_width(next_width, cx));
                            changed = true;
                        }
                        let next_hovered = pointer_inside || this.sidebar_resize_dragging;
                        if this.sidebar_resize_hovered != next_hovered {
                            this.sidebar_resize_hovered = next_hovered;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    });
                });

                let mouse_up_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    mouse_up_entity.update(cx, |this, cx| {
                        if !this.sidebar_resize_dragging {
                            return;
                        }
                        this.sidebar_resize_dragging = false;
                        this.sidebar_resize_hovered = bounds.contains(&event.position);
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .inset_0();

        panel_resize_handle(
            "sidebar-resize-handle",
            width - 8.0,
            line_visible,
            theme,
            input_layer,
        )
    }

    fn right_panel_resize_handle(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let line_visible = self.right_panel_resize_hovered || self.right_panel_resize_dragging;
        let input_layer = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let mouse_down_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, _, window, cx| {
                    if event.button != MouseButton::Left || !bounds.contains(&event.position) {
                        return;
                    }
                    mouse_down_entity.update(cx, |this, cx| {
                        let divider_x = f32::from(bounds.origin.x) + 8.0;
                        let current_width = f32::from(window.viewport_size().width) - divider_x;
                        this.right_panel_resize_dragging = true;
                        this.right_panel_resize_hovered = true;
                        this.right_panel_resize_pointer_offset =
                            divider_x - f32::from(event.position.x);
                        // Resolve the responsive default to a persisted width as
                        // soon as the user starts dragging it.
                        if this.right_panel_width.is_none() {
                            this.right_panel_width = Some(current_width);
                        }
                        cx.notify();
                    });
                });

                let mouse_move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.right_panel_resize_dragging {
                            let viewport_width = f32::from(window.viewport_size().width);
                            let revealed_sidebar_width =
                                this.sidebar.read(cx).width() * this.sidebar_reveal;
                            let divider_x = f32::from(event.position.x)
                                + this.right_panel_resize_pointer_offset;
                            let next_width = clamp_right_panel_width(
                                viewport_width - divider_x,
                                viewport_width,
                                revealed_sidebar_width,
                            );
                            if this
                                .right_panel_width
                                .is_none_or(|width| (width - next_width).abs() > f32::EPSILON)
                            {
                                this.right_panel_width = Some(next_width);
                                changed = true;
                            }
                        }
                        let next_hovered = pointer_inside || this.right_panel_resize_dragging;
                        if this.right_panel_resize_hovered != next_hovered {
                            this.right_panel_resize_hovered = next_hovered;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    });
                });

                let mouse_up_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    mouse_up_entity.update(cx, |this, cx| {
                        if !this.right_panel_resize_dragging {
                            return;
                        }
                        this.right_panel_resize_dragging = false;
                        this.right_panel_resize_hovered = bounds.contains(&event.position);
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .inset_0();

        panel_resize_handle(
            "right-panel-resize-handle",
            -8.0,
            line_visible,
            theme,
            input_layer,
        )
    }

    fn subagent_right_panel(
        &self,
        panel: SubagentPanel,
        panel_width: gpui::Pixels,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let menu_open = self.subagent_panel_menu_open;
        let tab_trigger = div()
            .id("subagent-panel-tab-trigger")
            .h_full()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .items_center()
            .gap(px(8.0))
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_expanded(menu_open)
            .aria_label(if menu_open {
                "关闭面板信息下拉框"
            } else {
                "打开面板信息下拉框"
            })
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.subagent_panel_menu_open = !this.subagent_panel_menu_open;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.subagent_panel_menu_open = !this.subagent_panel_menu_open;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(icon("settings-agent", theme.text.into()).size(px(16.0)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text)
                    .child("子智能体"),
            )
            .child(
                icon("chevron-down", theme.text_tertiary.into())
                    .size(px(12.0))
                    .flex_none(),
            );
        let close_button = div()
            .id("subagent-panel-close")
            .size(px(20.0))
            .rounded(px(5.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("关闭子智能体面板")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.text.alpha(0.12)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_right_panel(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.close_right_panel(cx);
                }
            }))
            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0)));
        let plus_button = div()
            .id("subagent-panel-add-tab")
            .size(px(28.0))
            .rounded(px(8.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("打开面板选择器")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(|this, _, _, cx| {
                this.subagent_panel_menu_open = !this.subagent_panel_menu_open;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.subagent_panel_menu_open = !this.subagent_panel_menu_open;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_size(px(21.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(350.0))
                    .text_color(theme.text_tertiary)
                    .child("+"),
            );
        let toolbar = div()
            .id("subagent-panel-toolbar")
            .h(px(46.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .bg(theme.surface_under)
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .id("subagent-panel-tab")
                    .h(px(28.0))
                    .w(px(156.0))
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .bg(theme.surface)
                    .flex()
                    .items_center()
                    .child(tab_trigger)
                    .child(close_button),
            )
            .child(plus_button);

        let back_button = div()
            .id("subagent-panel-back")
            .size(px(24.0))
            .rounded(px(10.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("返回子智能体列表")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.text.alpha(0.12)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_right_panel(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.close_right_panel(cx);
                }
            }))
            .child(icon("back", theme.text_tertiary.into()).size(px(16.0)));
        let panel_name = panel.name.clone();
        let panel_label = format!("子智能体 {panel_name}，任务 {}", panel.thread_id);
        let header = div()
            .id("subagent-panel-header")
            .h(px(SUBAGENT_PANEL_HEADER_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(16.0))
            .border_b_1()
            .border_color(theme.command_border)
            .flex()
            .items_center()
            .gap(px(8.0))
            .aria_label(panel_label)
            .child(back_button)
            .child(
                icon("subagent-activity", rgba(0xff7b7fff).into())
                    .size(px(24.0))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(panel_name.clone()),
            );

        let dropdown = div()
            .id("subagent-panel-menu")
            .absolute()
            .top(px(59.0))
            .left(px(43.0))
            .w(px(240.0))
            .h(px(204.0))
            .p(px(10.0))
            .rounded(px(25.0))
            .bg(theme.elevated)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(3.0), rgba(0x0000000a).into()).blur_radius(px(7.5)),
                BoxShadow::new(px(0.0), px(0.0), rgba(0x0000000d).into()).blur_radius(px(20.0)),
            ])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .px(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("环境信息"),
            )
            .child(
                div()
                    .px(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("变更"),
            )
            .child(div().h(px(0.5)).mx(px(4.0)).bg(theme.border))
            .child(
                div()
                    .h(px(28.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("子智能体")
                    .child("1 完成"),
            )
            .child(
                div()
                    .id("subagent-panel-menu-current")
                    .h(px(40.0))
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label(format!("子智能体 {panel_name}"))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.subagent_panel_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.subagent_panel_menu_open = false;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(icon("subagent-activity", rgba(0xff7b7fff).into()).size(px(20.0)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(panel_name),
                    ),
            );

        div()
            .id("right-panel")
            .w(panel_width)
            .min_w(panel_width)
            .h_full()
            .flex_none()
            .relative()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .track_focus(&self.right_panel_focus)
            .on_key_down(cx.listener(Self::handle_right_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .child(self.right_panel_resize_handle(theme, cx))
            .child(toolbar)
            .child(header)
            .child(
                div()
                    .id("subagent-panel-body")
                    .min_h(px(0.0))
                    .flex_1()
                    .bg(theme.surface)
                    .child(panel.home.cached(StyleRefinement::default().size_full())),
            )
            .when(menu_open, |panel| panel.child(dropdown))
    }

    fn right_panel(
        &self,
        panel_width: gpui::Pixels,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        if let Some(panel) = self.subagent_panel.clone() {
            return self.subagent_right_panel(panel, panel_width, theme, cx);
        }
        if let Some(review) = self.diff_review.clone() {
            let target = cx.entity();
            let callback = DiffReviewCallback::new(move |event, _, cx| {
                target.update(cx, move |app, cx| app.handle_diff_review_event(event, cx));
            });
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .track_focus(&self.right_panel_focus)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(self.right_panel_resize_handle(theme, cx))
                .child(render_diff_review_panel(&review, theme, callback));
        }

        if self.right_panel_mode == Some(RightPanelMode::Terminal) {
            if let Some(terminal) = self.terminal_panels.get(&self.active_conversation) {
                return div()
                    .id("right-panel")
                    .w(panel_width)
                    .min_w(panel_width)
                    .h_full()
                    .flex_none()
                    .relative()
                    .border_l_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(terminal.clone())
                    .child(self.right_panel_resize_handle(theme, cx));
            }
        }

        let toolbar = div()
            .h(px(46.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .when_some(self.right_panel_mode, |toolbar, mode| {
                let (_, label, _, glyph) = RIGHT_PANEL_ITEMS
                    .iter()
                    .find(|(candidate, _, _, _)| *candidate == mode)
                    .copied()
                    .expect("right panel mode must have a launcher item");
                toolbar.child(
                    div()
                        .id("right-panel-active-tab")
                        .h(px(28.0))
                        .max_w(px(156.0))
                        .px(px(8.0))
                        .rounded(px(10.0))
                        .bg(theme.text.alpha(0.05))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon(glyph, theme.text.into()).size(px(16.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .text_size(px(13.0))
                                .line_height(px(18.5714))
                                .text_color(theme.text)
                                .child(label),
                        )
                        .child(
                            div()
                                .id("right-panel-close-tab")
                                .size(px(20.0))
                                .rounded(px(5.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.sidebar_hover))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.right_panel_mode = None;
                                    this.right_panel_keyboard_focus = false;
                                    cx.notify();
                                }))
                                .child(
                                    icon("close-dialog", theme.text_tertiary.into()).size(px(12.0)),
                                ),
                        ),
                )
            });

        div()
            .id("right-panel")
            .w(panel_width)
            .min_w(panel_width)
            .h_full()
            .flex_none()
            .relative()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .track_focus(&self.right_panel_focus)
            .on_key_down(cx.listener(Self::handle_right_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .child(self.right_panel_resize_handle(theme, cx))
            .child(toolbar)
            .child(
                div()
                    .min_h(px(0.0))
                    .flex_1()
                    .p(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.right_panel_mode.is_none(), |body| {
                        body.child(
                            div()
                                .w_full()
                                .max_w(px(576.0))
                                .px(px(20.0))
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .children(RIGHT_PANEL_ITEMS.iter().enumerate().map(
                                    |(index, (_, label, shortcut, glyph))| {
                                        self.right_panel_menu_item(
                                            index, label, shortcut, glyph, theme, cx,
                                        )
                                    },
                                )),
                        )
                    }),
            )
    }
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.terminal_return_focus_pending {
            if let Some(host) = self.conversation_hosts.get(&self.active_conversation) {
                host.composer
                    .read(cx)
                    .prompt_focus_handle(cx)
                    .focus(window, cx);
            }
            self.terminal_return_focus_pending = false;
        }
        let viewport = window.viewport_size();
        let theme = Theme::for_window(
            self.mode,
            window.is_window_active(),
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
        );
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed)
        {
            return startup_loading_view(theme).into_any_element();
        }
        if self.image_preview.is_some() && !self.image_preview_focus_active {
            self.image_preview_previous_focus = window.focused(cx);
            self.image_preview_focus.focus(window, cx);
            self.image_preview_focus_active = true;
        } else if self.image_preview.is_none() && self.image_preview_focus_active {
            if let Some(previous) = self.image_preview_previous_focus.take() {
                previous.focus(window, cx);
            }
            self.image_preview_focus_active = false;
        }
        if self.project_creation_open && self.project_creation_focus_pending {
            self.project_creation_focus.focus(window, cx);
            self.project_creation_focus_pending = false;
        }
        if self.right_panel_open && self.right_panel_focus_pending {
            self.right_panel_focus.focus(window, cx);
            self.right_panel_focus_pending = false;
        }
        if self.bottom_panel_add_menu_open && self.bottom_panel_focus_pending {
            self.bottom_panel_focus.focus(window, cx);
            self.bottom_panel_focus_pending = false;
        }
        let sidebar_width = self.sidebar.read(cx).width();
        let sidebar_reveal = self.sidebar_reveal.clamp(0.0, 1.0);
        let revealed_sidebar_width = sidebar_width * sidebar_reveal;
        let resumed_title = match &self.active_conversation {
            ConversationKey::Thread(id) if !self.showing_settings => {
                self.workspace_store.snapshot().thread(id).map(|thread| {
                    (
                        thread.title.clone(),
                        crate::workspace::project_id_for_thread(
                            thread,
                            &self.workspace_store.snapshot().projects,
                        )
                        .is_some(),
                    )
                })
            }
            _ => None,
        };
        // CDP at both 2560×1410 and the project's 1440×900 target showed a
        // persisted 1418.21875 px panel, clamped to leave the main thread at
        // its measured 773.09375 px right edge on narrower windows.
        let viewport_width = f32::from(window.viewport_size().width);
        let default_right_panel_width = (window.viewport_size().width - px(773.09375))
            .min(px(1_418.218_8))
            .max(px(RIGHT_PANEL_MIN_WIDTH));
        let right_panel_width = px(clamp_right_panel_width(
            self.right_panel_width
                .unwrap_or(f32::from(default_right_panel_width)),
            viewport_width,
            revealed_sidebar_width,
        ));
        div()
            .id(if self.showing_settings {
                "app-shell-settings"
            } else {
                "app-shell"
            })
            .size_full()
            .relative()
            .flex()
            .font(ui_font())
            .on_click(cx.listener(|this, _, _, cx| {
                this.home.update(cx, |home, cx| home.close_model_picker(cx));
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
                this.close_bottom_panel_menu(cx);
                if this.subagent_panel_menu_open {
                    this.subagent_panel_menu_open = false;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, _, cx| {
                if this.right_panel_open && this.right_panel_mode == Some(RightPanelMode::Terminal) { this.close_right_panel(cx); }
                else { this.right_panel_open = true; this.select_right_panel_item(2, cx); }
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(cx.listener(|this, _: &DismissPermissionUi, _, cx| {
                if this.image_preview.is_some() {
                    this.image_preview = None;
                    this.image_preview_dimensions = None;
                    this.image_preview_zoom = 1.0;
                    cx.stop_propagation();
                    cx.notify();
                } else if this.bottom_panel_add_menu_open {
                    this.close_bottom_panel_menu(cx);
                } else if this.project_creation_open {
                    this.close_project_creation(cx);
                } else if this.permission_confirmation_open {
                    this.permission_confirmation_open = false;
                    cx.notify();
                } else {
                    this.home
                        .update(cx, |home, cx| home.close_model_picker(cx));
                }
            }))
            .when(self.showing_settings, |shell| {
                shell.child(self.settings.clone())
            })
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .w(px(revealed_sidebar_width))
                            .min_w(px(revealed_sidebar_width))
                            .h_full()
                            .flex_none()
                            .overflow_hidden()
                            // Electron paints the translucent surface on the
                            // outer aside; only its inner contents fade while
                            // the panel collapses.
                            .bg(theme.sidebar_surface)
                            .child(
                                div()
                                    .w(px(sidebar_width))
                                    .min_w(px(sidebar_width))
                                    .h_full()
                                    // Avoid putting the settled sidebar foreground
                                    // through an opacity context. On a translucent
                                    // window, text already uses grayscale AA; an
                                    // additional alpha blend makes glyph and SVG
                                    // edges look soft over bright backgrounds.
                                    .when(sidebar_reveal < 1.0, |content| {
                                        content.opacity(sidebar_reveal)
                                    })
                                    .child(self.sidebar.clone()),
                            ),
                    )
                    // Sidebar scrolling dirties its ancestor view by design. Keep the
                    // much larger, static home/composer subtree cached so a wheel or
                    // trackpad frame does not rebuild and repaint the main pane.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .w_full()
                                    .min_h(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .h_full()
                                            // Keep the conversation surface away from both
                                            // workspace edges when a side panel narrows the main
                                            // column. Max-width content remains unchanged on wide
                                            // windows because HomeView still centers it internally.
                                            .px(px(MAIN_CONTENT_HORIZONTAL_GUTTER))
                                            .bg(theme.surface)
                                            .child(
                                                self.home.clone().cached(
                                                    StyleRefinement::default().size_full(),
                                                ),
                                            ),
                                    )
                                    .when(self.right_panel_open, |row| {
                                        row.child(self.right_panel(
                                            right_panel_width,
                                            theme,
                                            cx,
                                        ))
                                    }),
                            )
                            .when(self.bottom_panel_open, |workspace| {
                                workspace.child(self.bottom_panel(theme, cx))
                            }),
                    )
            })
            .when(self.permission_confirmation_open, |shell| {
                shell.child(
                    div()
                        .id("permission-confirmation-overlay")
                        .absolute()
                        .inset_0()
                        // Electron computed style: #00000022.
                        .bg(rgba(0x00000022))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                        .child(
                            div()
                                .id("permission-confirmation-dialog")
                                .w(px(520.0))
                                .h(px(376.6875))
                                .rounded(px(25.0))
                                .border(px(0.5))
                                .border_color(theme.border)
                                .bg(theme.model_picker_surface)
                                .shadow(vec![
                                    BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                                        .blur_radius(px(8.0))
                                        .spread_radius(px(-2.0)),
                                ])
                                .p(px(20.0))
                                .flex()
                                .flex_col()
                                .text_color(theme.text)
                                .child(
                                    div()
                                        .h(px(28.0))
                                        .flex()
                                        .items_start()
                                        .gap(px(8.0))
                                        .child(icon("permission-warning", theme.text.into()).size(px(20.0)))
                                        .child(
                                            div()
                                                .text_size(px(20.0))
                                                .line_height(px(24.0))
                                                .font_weight(gpui::FontWeight(600.0))
                                                .child("要开启完整访问权限吗？"),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child("Codex 将能够在未经您许可的情况下，在这台计算机上的任何位置运行命令、\n使用互联网，以及创建和编辑文件。这包括但不限于："),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(162.0))
                                        .rounded(px(17.0))
                                        .bg(theme.elevated)
                                        .child(permission_risk_row("permission-dialog-folder", "文件和文件夹", "读取、创建、修改、上传或删除此计算机上任意位置的文件", false, theme))
                                        .child(permission_risk_row("permission-dialog-terminal", "终端命令", "运行命令、安装软件和更改系统设置", true, theme))
                                        .child(permission_risk_row("permission-dialog-internet", "互联网和已连接的应用", "访问网站、发送数据并使用已启用的插件", true, theme)),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(21.0))
                                        .flex()
                                        .items_center()
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child(div().flex_1().child("这会带来敏感数据丢失或泄露、提示注入等风险。你可以将其关闭。"))
                                        .child(div().text_color(theme.accent).child("了解更多")),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(36.0))
                                        .flex()
                                        .justify_end()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .id("permission-confirmation-cancel")
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.05))
                                                .flex()
                                                .items_center()
                                                .text_size(px(14.0))
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.text.alpha(0.10)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.permission_confirmation_open = false;
                                                    cx.notify();
                                                }))
                                                .child("取消"),
                                        )
                                        .child(
                                            div()
                                                .id("permission-confirmation-confirm")
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(rgba(0xff67641a))
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(14.0))
                                                .text_color(rgba(0xff6764ff))
                                                .cursor_pointer()
                                                .hover(|style| style.bg(rgba(0xff676433)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.permission_confirmation_open = false;
                                                    this.home.update(cx, |home, cx| home.confirm_full_access(cx));
                                                    cx.notify();
                                                }))
                                                .child(icon("permission-warning", rgba(0xff6764ff).into()).size(px(16.0)))
                                                .child("确认"),
                                        ),
                                ),
                        ),
                )
            })
            .when_some(resumed_title, |shell, (title, in_project)| {
                // The resumed thread has its own opaque sticky header. Paint
                // it over the virtual list's overdraw band, just as Electron
                // masks scrolling Markdown beneath its 46px titlebar.
                shell.child(div()
                    .id("resumed-thread-header")
                    .absolute().top_0().left(px(revealed_sidebar_width))
                    .right(if self.right_panel_open { right_panel_width } else { px(0.0) })
                    .h(px(46.0)).bg(theme.surface).border_b_1().border_color(theme.border)
                    .pl(px(if sidebar_reveal < 0.5 { 184.0 } else { 14.0 })).pr(px(100.0))
                    .flex().items_center().gap(px(12.0))
                        .text_size(px(14.0)).line_height(px(20.0)).font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                    .when(in_project, |header| header.child(icon("folder", theme.text.into()).size(px(16.0)).flex_none()))
                    .child(div().min_w(px(0.0)).truncate().child(title)))
            })
            .when(!self.showing_settings && sidebar_reveal == 1.0, |shell| {
                shell.child(self.sidebar_resize_handle(theme, revealed_sidebar_width, cx))
            })
            // Keep the draggable titlebar behind its interactive controls so
            // their 28px hover hit areas receive pointer events.
            .child(titlebar_interaction_area())
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .absolute()
                            .top(px(LEADING_TITLEBAR_CONTROLS_TOP))
                            .left(px(88.0))
                            .flex()
                            .gap(px(4.0))
                            .child(
                                titlebar_icon_button("sidebar-toggle", false, false, theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_sidebar(window, cx);
                                    }),
                                ),
                            )
                            .child(titlebar_icon_button("back", false, false, theme))
                            // The captured reference has no forward history, so this
                            // control is intentionally disabled and 40% opaque.
                            .child(titlebar_icon_button("forward", true, false, theme)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .right(px(8.0))
                            .flex()
                            .gap(px(6.0))
                            .child(
                                titlebar_icon_button(
                                    "bottom-panel",
                                    false,
                                    self.bottom_panel_open,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_bottom_panel(cx);
                                })),
                            )
                            .child(
                                titlebar_icon_button(
                                    "right-sidebar",
                                    false,
                                    self.right_panel_open,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_right_panel(cx);
                                })),
                            ),
                    )
            })
            .when(self.project_creation_open, |shell| {
                shell.child(self.project_creation_overlay(theme, cx))
            })
            .when_some(self.image_preview.clone(), |shell, path| {
                let viewport = window.viewport_size();
                let zoom = self.image_preview_zoom;
                let image_width = (f32::from(viewport.width) - 64.0).max(160.0) * zoom;
                let image_height = (f32::from(viewport.height) - 128.0).max(120.0) * zoom;
                let percentage = format!("{}%", (zoom * 100.0).round() as i32);
                let preview_dimensions = self.image_preview_dimensions;
                shell.child(
                    div()
                        .id("image-preview-dialog")
                        .track_focus(&self.image_preview_focus)
                        .role(Role::Dialog)
                        .aria_label("图片预览")
                        .absolute()
                        .inset_0()
                        .bg(theme.surface)
                        .overflow_hidden()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.image_preview = None;
                            this.image_preview_dimensions = None;
                            this.image_preview_zoom = 1.0;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("image-preview-image-scroll")
                                .absolute()
                                .inset_0()
                                .pt(px(48.0))
                                .pb(px(80.0))
                                .px(px(32.0))
                                .overflow_scroll()
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    gpui::img(path.clone())
                                        .w(px(image_width))
                                        .h(px(image_height))
                                        .flex_none()
                                        .rounded(px(12.5))
                                        .object_fit(ObjectFit::Contain),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(12.0))
                                .right(px(12.0))
                                .flex()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("image-preview-open-original")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label("下载图片")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click({
                                            let path = path.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.download_preview_image(path.clone(), cx);
                                                cx.stop_propagation();
                                            })
                                        })
                                        .on_key_down({
                                            let path = path.clone();
                                            cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.download_preview_image(path.clone(), cx);
                                                    cx.stop_propagation();
                                                }
                                            })
                                        })
                                        .child(
                                            icon("image-download", theme.text.into()).size(px(20.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("image-preview-close")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label("关闭图片预览")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.image_preview = None;
                                            this.image_preview_dimensions = None;
                                            this.image_preview_zoom = 1.0;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(
                                            |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.image_preview = None;
                                                    this.image_preview_dimensions = None;
                                                    this.image_preview_zoom = 1.0;
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(icon("close-dialog", theme.text.into()).size(px(21.0))),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(32.0))
                                .left_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("image-preview-zoom-controls")
                                        .h(px(36.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .when_some(preview_dimensions, |controls, (width, height)| {
                                            controls.child(
                                                div()
                                                    .px(px(10.0))
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child(format!("{width} × {height}")),
                                            )
                                        })
                                        .child(
                                            div()
                                                .id("image-preview-zoom-out")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label("缩小图片")
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview_zoom =
                                                        (this.image_preview_zoom - 0.25).max(0.5);
                                                    cx.notify();
                                                }))
                                                .child("−"),
                                        )
                                        .child(
                                            div()
                                                .w(px(56.0))
                                                .text_center()
                                                .text_size(px(13.0))
                                                .text_color(theme.text)
                                                .child(percentage),
                                        )
                                        .child(
                                            div()
                                                .id("image-preview-zoom-in")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label("放大图片")
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview_zoom =
                                                        (this.image_preview_zoom + 0.25).min(3.0);
                                                    cx.notify();
                                                }))
                                                .child("+"),
                                        ),
                                ),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use gpui::{
        AppContext, Bounds, MouseButton, TestApp, TestAppWindow, WindowBounds, WindowOptions,
        point, px, size,
    };

    use super::{ChatApp, ConversationKey, finder_reveal_command, startup_loading_logo_opacity};
    use crate::agent::{
        HistoryItemDetail, HistoryTurnStatus, ThreadActivity, ThreadHistory, ThreadHistoryItem,
        ThreadSummary, ThreadTurn,
    };
    use crate::components::{
        composer::{ComposerView, ConversationPhase, ModelCatalogLoadFinished},
        file_change::DiffReviewEvent,
        home::OpenSubAgentPanel,
        sidebar::OpenSettings,
    };
    use crate::theme::ThemeMode;

    fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<ChatApp>, elapsed_ms: u64) {
        app.advance_clock(Duration::from_millis(elapsed_ms));
        let handle = window.handle();
        app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        });
    }

    #[test]
    fn startup_logo_blinks_in_place_without_disappearing() {
        assert_eq!(startup_loading_logo_opacity(0.0), 1.0);
        assert!((startup_loading_logo_opacity(0.5) - 0.32).abs() < f32::EPSILON);
        assert_eq!(startup_loading_logo_opacity(1.0), 1.0);
    }

    #[test]
    fn diff_review_copy_and_open_events_have_native_actions() {
        let path = "/tmp/a file with spaces.txt";
        let command = finder_reveal_command(path);
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("-R"), std::ffi::OsStr::new(path)]
        );

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        window.update(|chat, _, cx| {
            chat.handle_diff_review_event(DiffReviewEvent::CopyPath(path.to_owned()), cx)
        });
        assert_eq!(
            app.read_from_clipboard().and_then(|item| item.text()),
            Some(path.to_owned())
        );
    }

    #[test]
    fn image_preview_download_copies_the_source_and_close_unmounts_the_overlay() {
        let suffix = std::process::id();
        let source = std::env::temp_dir().join(format!("gpui-image-preview-source-{suffix}.png"));
        let destination =
            std::env::temp_dir().join(format!("gpui-image-preview-download-{suffix}.png"));
        std::fs::write(&source, b"real image payload").unwrap();
        let _ = std::fs::remove_file(&destination);

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        window.update(|chat, _, cx| {
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                super::DismissPermissionUi,
                None,
            )]);
            chat.startup_model_catalog_resolved = true;
            chat.startup_sidebar_resolved = true;
            chat.startup_minimum_duration_elapsed = true;
            chat.image_preview = Some(source.clone());
            chat.image_preview_dimensions = Some((1024, 1024));
            chat.image_preview_zoom = 1.75;
            cx.notify();
        });
        window.draw();

        // CDP geometry scaled to the 900px test viewport: download is the
        // left 40px control and close is the right 40px control.
        window.simulate_click(point(px(812.0), px(32.0)), MouseButton::Left);
        assert!(app.did_prompt_for_new_path());
        assert_eq!(
            window.read(|chat, _| chat.image_preview.clone()),
            Some(source.clone())
        );
        app.simulate_new_path_selection(|_| Some(destination.clone()));
        app.run_until_parked();
        assert_eq!(std::fs::read(&destination).unwrap(), b"real image payload");

        window.simulate_click(point(px(866.0), px(32.0)), MouseButton::Left);
        window.read(|chat, _| {
            assert_eq!(chat.image_preview, None);
            assert_eq!(chat.image_preview_dimensions, None);
            assert_eq!(chat.image_preview_zoom, 1.0);
        });

        window.draw();
        window.update(|chat, _, cx| {
            chat.image_preview = Some(source.clone());
            cx.notify();
        });
        window.draw();
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.image_preview.is_none()));
        std::fs::remove_file(source).unwrap();
        std::fs::remove_file(destination).unwrap();
    }

    #[test]
    fn startup_loading_screen_waits_for_catalog_sidebar_and_one_second_minimum() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| {
            chat.startup_model_catalog_resolved = false;
            chat.startup_sidebar_resolved = false;
            chat.startup_minimum_duration_elapsed = false;
            cx.notify();
        });
        window.draw();

        let sidebar_toggle_center = point(px(102.0), px(25.0));
        window.simulate_click(sidebar_toggle_center, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.sidebar_collapsed));

        let home = window.read(|chat, _| chat.home.clone());
        app.update(|cx| home.update(cx, |_, cx| cx.emit(ModelCatalogLoadFinished)));
        assert!(window.read(|chat, _| chat.startup_model_catalog_resolved));
        assert!(!window.read(|chat, _| chat.startup_minimum_duration_elapsed));

        app.advance_clock(Duration::from_millis(999));
        app.run_until_parked();
        assert!(!window.read(|chat, _| chat.startup_minimum_duration_elapsed));

        app.advance_clock(Duration::from_millis(1));
        app.run_until_parked();
        assert!(window.read(|chat, _| chat.startup_minimum_duration_elapsed));

        window.draw();
        window.simulate_click(sidebar_toggle_center, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.sidebar_collapsed));

        window.update(|chat, _, cx| {
            chat.startup_sidebar_resolved = true;
            cx.notify();
        });
        window.draw();
        window.simulate_click(sidebar_toggle_center, MouseButton::Left);
        assert!(window.read(|chat, _| chat.sidebar_collapsed));
    }

    #[test]
    fn sidebar_toggle_collapses_and_restores_without_moving_the_titlebar_control() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        assert!(!window.read(|app, _| app.sidebar_collapsed));

        // The control remains at left: 88px, centered at y=25px in both states.
        let toggle_center = point(px(102.0), px(25.0));
        window.simulate_click(toggle_center, MouseButton::Left);
        assert!(window.read(|app, _| app.sidebar_collapsed));
        assert!(window.read(|app, _| app.sidebar_animation_running));

        simulate_next_frame(&mut app, &window, 200);
        let collapsed_midpoint = window.read(|app, _| app.sidebar_reveal);
        assert!(collapsed_midpoint > 0.0 && collapsed_midpoint < 1.0);

        simulate_next_frame(&mut app, &window, 200);
        assert_eq!(window.read(|app, _| app.sidebar_reveal), 0.0);
        assert!(!window.read(|app, _| app.sidebar_animation_running));

        window.draw();
        window.simulate_click(toggle_center, MouseButton::Left);
        assert!(!window.read(|app, _| app.sidebar_collapsed));
        assert!(window.read(|app, _| app.sidebar_animation_running));

        simulate_next_frame(&mut app, &window, 400);
        assert_eq!(window.read(|app, _| app.sidebar_reveal), 1.0);
        assert!(!window.read(|app, _| app.sidebar_animation_running));
    }

    #[test]
    fn terminal_shortcut_restores_focus_and_preserves_the_session() {
        let mut app = TestApp::new();
        app.update(|cx| {
            crate::components::terminal::init(cx);
            cx.bind_keys([gpui::KeyBinding::new("ctrl-`", super::ToggleTerminal, None)]);
        });
        let mut window = app.open_window(|_, cx| ChatApp::new(ThemeMode::Dark, false, cx));
        window.update(|chat, window, cx| {
            chat.complete_startup_for_capture(cx);
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .read(cx)
                .prompt_focus_handle(cx)
                .focus(window, cx);
        });
        window.draw();
        window.simulate_keystroke("ctrl-`");
        window.draw();
        let terminal =
            window.read(|chat, _| chat.terminal_panels[&chat.active_conversation].entity_id());
        assert!(window.read(|chat, _| chat.right_panel_open));
        window.simulate_keystroke("ctrl-`");
        window.draw();
        assert!(!window.read(|chat, _| chat.right_panel_open));
        window.simulate_keystroke("ctrl-`");
        window.draw();
        assert!(window.read(|chat, _| chat.right_panel_open));
        assert_eq!(
            window.read(|chat, _| chat.terminal_panels[&chat.active_conversation].entity_id()),
            terminal
        );
    }

    #[test]
    fn right_panel_stays_open_until_its_titlebar_toggle_is_clicked_again() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        let trigger = point(px(878.0), px(23.0));
        window.simulate_click(trigger, MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_click(point(px(400.0), px(400.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.right_panel_open));
    }

    #[test]
    fn collaboration_event_opens_a_read_only_subagent_panel_without_switching_parent() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1_440.0), px(900.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Light, false, cx),
        );
        window.update(|chat, _, cx| {
            let backend = chat._agent_backend.clone();
            let child = cx.new(|cx| ComposerView::new_with_backend(chat.mode, backend, cx));
            let child_cwd = PathBuf::from("/tmp/collab-evidence-child");
            child.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    child_cwd.clone(),
                    None,
                    Some("thread-collab-evidence-child".to_owned()),
                    cx,
                )
            });
            chat.conversation_hosts.insert(
                ConversationKey::Thread("thread-collab-evidence-child".to_owned()),
                super::ConversationHost {
                    composer: child,
                    cwd: child_cwd,
                    project_id: None,
                },
            );
        });
        let (parent_key, parent_composer, home) = window.read(|chat, cx| {
            (
                chat.active_conversation.clone(),
                chat.home.read(cx).composer_entity(),
                chat.home.clone(),
            )
        });

        app.update(|cx| {
            home.update(cx, |_, cx| {
                cx.emit(OpenSubAgentPanel {
                    thread_id: "thread-collab-evidence-child".to_owned(),
                    name: "Collab evidence child".to_owned(),
                })
            })
        });

        window.read(|chat, cx| {
            assert!(chat.right_panel_open);
            assert_eq!(chat.active_conversation, parent_key);
            assert!(chat.home.read(cx).composer_entity() == parent_composer);
            let panel = chat
                .subagent_panel
                .as_ref()
                .expect("collaboration row should mount a subagent panel");
            assert_eq!(panel.thread_id, "thread-collab-evidence-child");
            assert_eq!(panel.name, "Collab evidence child");
            assert_eq!(
                panel.home.read(cx).composer_entity().read(cx).thread_id(),
                Some("thread-collab-evidence-child")
            );
        });

        window.draw();
        // At 1440px the 603px panel begins at x=837; the live 156×28 tab
        // occupies x=845..1001 and toggles the same information popover.
        window.simulate_click(point(px(900.0), px(23.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.subagent_panel_menu_open));
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.right_panel_open));
        assert!(!window.read(|chat, _| chat.subagent_panel_menu_open));

        window.simulate_keystroke("escape");
        window.read(|chat, _| {
            assert!(!chat.right_panel_open);
            assert!(chat.subagent_panel.is_none());
            assert_eq!(chat.active_conversation, parent_key);
        });
    }

    #[test]
    fn bottom_panel_matches_the_persistent_native_titlebar_toggle() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        let trigger = point(px(844.0), px(23.0));
        window.simulate_click(trigger, MouseButton::Left);
        assert!(window.read(|chat, _| chat.bottom_panel_open));
        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![super::BottomPanelMode::Terminal]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(0));

        window.draw();
        window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.bottom_panel_open));
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.bottom_panel_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.bottom_panel_open));
    }

    #[test]
    fn bottom_panel_add_menu_closes_on_toggle_outside_and_escape() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.toggle_bottom_panel_menu(cx);
        });
        assert!(window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        window.draw();
        window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));
        assert!(window.read(|chat, _| chat.bottom_panel_open));
    }

    #[test]
    fn bottom_panel_add_menu_supports_keyboard_selection() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.toggle_bottom_panel_menu(cx);
        });
        window.draw();
        window.simulate_keystroke("down");
        window.simulate_keystroke("down");
        assert_eq!(window.read(|chat, _| chat.bottom_panel_focused_item), 1);
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![
                super::BottomPanelMode::Terminal,
                super::BottomPanelMode::Terminal
            ]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(1));
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));
    }

    #[test]
    fn bottom_panel_add_menu_appends_after_the_default_terminal() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.select_bottom_panel_item(2, cx);
        });

        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![
                super::BottomPanelMode::Terminal,
                super::BottomPanelMode::Browser
            ]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(1));
        assert_eq!(
            super::bottom_panel_tab_spec(super::BottomPanelMode::Browser).0,
            "新标签页"
        );
    }

    #[test]
    fn right_panel_menu_supports_keyboard_selection() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_right_panel(cx));
        window.draw();
        window.simulate_keystroke("down");
        window.simulate_keystroke("down");
        assert_eq!(window.read(|chat, _| chat.right_panel_focused_item), 1);
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|chat, _| chat.right_panel_mode),
            Some(super::RightPanelMode::Browser)
        );
    }

    #[test]
    fn sidebar_resize_handle_supports_full_hit_area_limits_and_collapse() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1440.0), px(900.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        // Both edges of the 16px hit area work along its full height, with no
        // jump when grabbing away from the divider's center.
        for (offset, y) in [(-7.0, 10.0), (7.0, 450.0), (0.0, 890.0)] {
            window.update(|chat, _, cx| {
                chat.sidebar
                    .update(cx, |sidebar, cx| sidebar.set_width(300.0, cx));
            });
            window.draw();
            window.simulate_mouse_move(point(px(300.0 + offset), px(y)));
            assert!(window.read(|chat, _| chat.sidebar_resize_hovered));
            window.simulate_mouse_down(point(px(300.0 + offset), px(y)), MouseButton::Left);
            window.simulate_mouse_move(point(px(380.0 + offset), px(y)));
            window.simulate_mouse_up(point(px(380.0 + offset), px(y)), MouseButton::Left);
            assert!((window.read(|chat, cx| chat.sidebar.read(cx).width()) - 380.0).abs() < 0.2);
            assert!(!window.read(|chat, _| chat.sidebar_resize_dragging));
        }
        window.draw();
        window.simulate_mouse_down(point(px(380.0), px(450.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(1200.0), px(450.0)));
        window.simulate_mouse_up(point(px(1200.0), px(450.0)), MouseButton::Left);
        assert_eq!(
            window.read(|chat, cx| chat.sidebar.read(cx).width()),
            super::SIDEBAR_MAX_WIDTH
        );
        window.draw();
        window.simulate_mouse_down(point(px(480.0), px(450.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(10.0), px(450.0)));
        window.simulate_mouse_up(point(px(10.0), px(450.0)), MouseButton::Left);
        assert_eq!(
            window.read(|chat, cx| chat.sidebar.read(cx).width()),
            super::SIDEBAR_MIN_WIDTH
        );
        window.simulate_mouse_move(point(px(600.0), px(450.0)));
        assert!(!window.read(|chat, _| chat.sidebar_resize_hovered));
        window.update(|chat, win, cx| {
            chat.sidebar
                .update(cx, |sidebar, cx| sidebar.set_width(360.0, cx));
            chat.toggle_sidebar(win, cx);
            chat.toggle_sidebar(win, cx);
            assert_eq!(chat.sidebar.read(cx).width(), 360.0);
            assert!(!chat.sidebar_resize_dragging);
            chat.open_right_panel(cx);
        });
        window.draw();
        window.simulate_mouse_down(point(px(360.0), px(450.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(420.0), px(450.0)));
        window.simulate_mouse_up(point(px(420.0), px(450.0)), MouseButton::Left);
        assert!((window.read(|chat, cx| chat.sidebar.read(cx).width()) - 420.0).abs() < 0.2);
        assert!(window.read(|chat, _| chat.right_panel_open));
    }

    #[test]
    fn right_panel_resize_handle_matches_reference_limits_without_hiding_the_panel() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1440.0), px(900.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        window.update(|chat, _, cx| chat.open_right_panel(cx));
        window.draw();

        // CDP: a 16 px hit area is centered on the one-pixel divider.
        window.simulate_mouse_move(point(px(773.09375), px(300.0)));
        assert!(window.read(|chat, _| chat.right_panel_resize_hovered));
        window.simulate_mouse_down(point(px(773.09375), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(1100.0), px(300.0)));
        window.simulate_mouse_up(point(px(1100.0), px(300.0)), MouseButton::Left);
        let narrow_width = window.read(|chat, _| chat.right_panel_width.unwrap());
        assert!((narrow_width - 339.09375).abs() < 0.2, "{narrow_width}");

        window.draw();
        window.simulate_mouse_down(point(px(1100.0), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(400.0), px(300.0)));
        window.simulate_mouse_up(point(px(400.0), px(300.0)), MouseButton::Left);
        let sidebar_width = window.read(|chat, cx| chat.sidebar.read(cx).width());
        let expected_max = 1440.0 - sidebar_width - super::RIGHT_PANEL_MAIN_MIN_WIDTH;
        assert!(
            (window.read(|chat, _| chat.right_panel_width.unwrap()) - expected_max).abs() < 0.2
        );

        window.draw();
        let divider_x = 1440.0 - expected_max;
        window.simulate_mouse_down(point(px(divider_x), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(1300.0), px(300.0)));
        window.simulate_mouse_up(point(px(1300.0), px(300.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));
        assert_eq!(
            window.read(|chat, _| chat.right_panel_width),
            Some(super::RIGHT_PANEL_MIN_WIDTH)
        );
    }

    #[test]
    fn settings_event_from_profile_menu_navigates_to_settings() {
        let mut app = TestApp::new();
        let window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        let sidebar = window.read(|app, _| app.sidebar.clone());
        app.update(|cx| sidebar.update(cx, |_, cx| cx.emit(OpenSettings)));
        app.run_until_parked();
        assert!(window.read(|app, _| app.showing_settings));
    }

    #[test]
    fn permission_confirmation_buttons_close_the_modal() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_permission_confirmation_for_capture(cx));
        window.draw();
        window.simulate_click(point(px(553.0), px(500.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.permission_confirmation_open));

        window.update(|chat, _, cx| chat.open_permission_confirmation_for_capture(cx));
        window.draw();
        window.simulate_click(point(px(645.0), px(500.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.permission_confirmation_open));
    }

    #[test]
    fn clicking_settings_in_profile_menu_opens_settings_surface() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        window.draw();
        window.simulate_click(point(px(80.0), px(677.0)), MouseButton::Left);
        window.draw();
        // CDP: the single 30 px settings row is anchored 43 px above the
        // bottom of the 700 px window, with the menu's four-pixel inset.
        window.simulate_click(point(px(80.0), px(638.0)), MouseButton::Left);
        assert!(window.read(|app, _| app.showing_settings));
    }

    #[test]
    fn projects_menu_closes_when_the_main_surface_is_clicked() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| {
                let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
                app.open_projects_section_menu(cx);
                app
            },
        );

        window.draw();
        assert!(window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
        window.simulate_click(point(px(600.0), px(350.0)), MouseButton::Left);
        assert!(!window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
    }

    #[test]
    fn switching_drafts_preserves_the_background_conversation_host() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        let (first_key, first_composer) = window.read(|chat, _| {
            (
                chat.active_conversation.clone(),
                chat.conversation_hosts[&chat.active_conversation]
                    .composer
                    .clone(),
            )
        });
        app.update(|cx| {
            first_composer.update(cx, |composer, cx| {
                composer.set_command_tool_for_capture(true, cx)
            });
        });

        window.update(|chat, _, cx| {
            chat.start_draft(
                Some("project-stable-id".to_owned()),
                PathBuf::from("/tmp/second-project"),
                cx,
            );
        });
        assert_ne!(
            window.read(|chat, _| chat.active_conversation.clone()),
            first_key
        );
        assert_eq!(window.read(|chat, _| chat.conversation_hosts.len()), 2);
        assert_eq!(
            app.read_entity(&first_composer, |composer, _| composer.conversation_phase()),
            ConversationPhase::Streaming
        );

        window.update(|chat, _, cx| chat.switch_home_to(first_key.clone(), cx));
        let active_composer = window.read(|chat, cx| chat.home.read(cx).composer_entity());
        assert!(active_composer == first_composer);
        assert_eq!(
            window.read(|chat, _| chat.active_conversation.clone()),
            first_key
        );
        assert!(matches!(first_key, ConversationKey::Draft(_)));
        assert_eq!(
            app.read_entity(&active_composer, |composer, _| composer
                .conversation_phase()),
            ConversationPhase::Streaming
        );
    }

    #[test]
    fn thread_created_rekeys_the_draft_without_replacing_its_host() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        let (draft_key, composer) = window.read(|chat, _| {
            (
                chat.active_conversation.clone(),
                chat.conversation_hosts[&chat.active_conversation]
                    .composer
                    .clone(),
            )
        });
        app.update(|cx| {
            composer.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    PathBuf::from("/tmp/project"),
                    Some("project-stable-id".to_owned()),
                    Some("thread-stable-id".to_owned()),
                    cx,
                );
            });
        });

        window.update(|chat, _, cx| chat.rekey_created_thread("thread-stable-id".to_owned(), cx));
        let thread_key = ConversationKey::Thread("thread-stable-id".to_owned());
        assert!(!window.read(|chat, _| chat.conversation_hosts.contains_key(&draft_key)));
        assert_eq!(
            window.read(|chat, _| chat.active_conversation.clone()),
            thread_key
        );
        let rekeyed = window.read(|chat, _| chat.conversation_hosts[&thread_key].composer.clone());
        assert!(rekeyed == composer);
    }

    fn history_fixture(thread_id: &str, message: &str) -> ThreadHistory {
        ThreadHistory {
            thread: ThreadSummary {
                thread_id: thread_id.to_owned(),
                title: format!("Thread {thread_id}"),
                preview: message.to_owned(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                section: None,
                created_at: 1,
                updated_at: 2,
                recency_at: Some(2),
                activity: ThreadActivity::Idle,
            },
            turns: vec![ThreadTurn {
                turn_id: format!("turn-{thread_id}"),
                status: HistoryTurnStatus::Completed,
                items_view: HistoryItemDetail::Full,
                items: vec![
                    ThreadHistoryItem::UserMessage {
                        images: Vec::new(),
                        item_id: format!("user-{thread_id}"),
                        text: message.to_owned(),
                    },
                    ThreadHistoryItem::AssistantMessage {
                        item_id: format!("assistant-{thread_id}"),
                        text: format!("answer {message}"),
                        phase: None,
                    },
                ],
                started_at: Some(1),
                completed_at: Some(2),
                duration_ms: Some(1),
                error: None,
            }],
            next_turn_cursor: None,
            backwards_turn_cursor: None,
        }
    }

    #[cfg(feature = "screenshot")]
    #[test]
    fn resumed_thread_readiness_tracks_loading_completion_and_errors() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
            ChatApp::new(ThemeMode::Dark, false, cx)
        });
        let composer = window.read(|chat, _| {
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone()
        });
        app.update(|cx| {
            composer.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    PathBuf::from("/tmp/capture"),
                    None,
                    Some("thread-capture".to_owned()),
                    cx,
                );
                composer.set_history_loading(true, cx);
            });
        });
        window.update(|chat, _, cx| chat.rekey_created_thread("thread-capture".to_owned(), cx));
        window
            .update(|chat, _, cx| chat.resume_thread_for_capture("thread-capture".to_owned(), cx));

        assert_eq!(
            window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
            Ok(false)
        );

        app.update(|cx| {
            composer.update(cx, |composer, cx| {
                composer.hydrate_history(history_fixture("thread-capture", "captured history"), cx)
            });
        });
        assert_eq!(
            window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
            Ok(true)
        );

        app.update(|cx| {
            composer.update(cx, |composer, cx| {
                composer.set_history_error("history failed".to_owned(), cx)
            });
        });
        assert_eq!(
            window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
            Err("history failed".to_owned())
        );
    }

    #[test]
    fn rapid_thread_switch_keeps_late_history_scoped_to_its_original_host() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
            ChatApp::new(ThemeMode::Dark, false, cx)
        });
        let thread_a = window.read(|chat, _| {
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone()
        });
        app.update(|cx| {
            thread_a.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    PathBuf::from("/tmp/project-a"),
                    None,
                    Some("thread-a".to_owned()),
                    cx,
                );
                composer.set_history_loading(true, cx);
            });
        });
        window.update(|chat, _, cx| {
            chat.rekey_created_thread("thread-a".to_owned(), cx);
            chat.start_draft(None, PathBuf::from("/tmp/project-b"), cx);
        });
        let thread_b = window.read(|chat, _| {
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone()
        });

        app.update(|cx| {
            thread_a.update(cx, |composer, cx| {
                composer.hydrate_history(history_fixture("thread-a", "old selection"), cx)
            });
        });

        assert!(window.read(|chat, cx| chat.home.read(cx).composer_entity()) == thread_b);
        assert_eq!(
            app.read_entity(&thread_b, |composer, _| composer.conversation_phase()),
            ConversationPhase::Empty
        );
        assert_eq!(
            app.read_entity(&thread_a, |composer, _| composer.conversation_phase()),
            ConversationPhase::Complete
        );
    }

    #[test]
    fn switching_from_a_running_thread_does_not_stop_its_background_turn() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
            ChatApp::new(ThemeMode::Dark, false, cx)
        });
        let running = window.read(|chat, _| {
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone()
        });
        app.update(|cx| {
            running.update(cx, |composer, cx| {
                composer.set_workspace_context(
                    PathBuf::from("/tmp/running"),
                    None,
                    Some("thread-running".to_owned()),
                    cx,
                );
                composer.set_command_tool_for_capture(true, cx);
            });
        });
        window.update(|chat, _, cx| {
            chat.rekey_created_thread("thread-running".to_owned(), cx);
            chat.start_draft(None, PathBuf::from("/tmp/other"), cx);
        });

        assert_eq!(
            app.read_entity(&running, |composer, _| composer.conversation_phase()),
            ConversationPhase::Streaming
        );
        assert!(window.read(|chat, _| {
            chat.conversation_hosts
                .contains_key(&ConversationKey::Thread("thread-running".to_owned()))
                && chat.conversation_hosts.len() == 2
        }));
    }

    #[test]
    fn an_empty_backend_does_not_expose_a_phantom_project_menu() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| {
                let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
                app.open_project_menu_for_capture("missing-project".to_owned(), cx);
                app
            },
        );

        window.draw();
        assert!(!window.read(|app, cx| app.sidebar.read(cx).project_menu_is_open()));
    }

    #[test]
    fn an_empty_backend_does_not_expose_a_phantom_pinned_menu() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        assert!(!window.read(|app, cx| app.sidebar.read(cx).pinned_menu_is_open()));
    }

    #[test]
    fn project_creation_dialog_matches_reference_close_and_keyboard_behavior() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        assert!(window.read(|chat, _| chat.project_creation_open));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Local
        );

        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 1);
        window.simulate_keystroke("space");
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Remote
        );
        window.simulate_keystroke("tab");
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 3);
        window.simulate_keystroke("enter");
        assert!(!window.read(|chat, _| chat.project_creation_open));

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_click(point(px(50.0), px(80.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn local_project_next_closes_the_project_type_dialog() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_click(point(px(732.0), px(493.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn remote_project_step_is_preserved_by_dismiss_and_reset_by_cancel() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| ChatApp::new(ThemeMode::Dark, false, cx));

        window.update(|chat, _, cx| {
            chat.open_project_creation(cx);
            chat.project_creation_kind = super::ProjectCreationKind::Remote;
            chat.advance_project_creation(cx);
            chat.close_project_creation(cx);
            chat.open_project_creation(cx);
        });
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Remote
        );

        window.update(|chat, _, cx| chat.cancel_project_creation(cx));
        window.update(|chat, _, cx| chat.open_project_creation(cx));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Kind
        );
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Local
        );
    }

    #[test]
    fn remote_project_keyboard_order_matches_the_native_dialog() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| {
            let mut chat = ChatApp::new(ThemeMode::Dark, false, cx);
            chat.open_project_creation_remote_for_capture(cx);
            chat
        });

        window.draw();
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 1);
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 2);
        window.simulate_keystroke("enter");
        assert!(!window.read(|chat, _| chat.project_creation_open));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Kind
        );
    }

    #[test]
    fn project_creation_trigger_opens_and_the_same_screen_position_closes_on_the_overlay() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1440.0), px(900.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        // CDP-derived layout: 46 px titlebar safe area, 38 px brand header,
        // 31 px new-chat row, and the restored four-row navigation block.
        let trigger = point(px(214.0), px(282.5));
        window.simulate_mouse_move(trigger);
        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        app.run_until_parked();
        assert!(window.read(|chat, _| chat.project_creation_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn appearance_cards_change_the_application_theme() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| {
                let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
                app.open_settings_page("appearance", cx);
                app
            },
        );

        window.draw();
        window.simulate_click(point(px(583.0), px(250.0)), MouseButton::Left);
        assert_eq!(window.read(|app, _| app.mode), ThemeMode::Light);

        window.draw();
        window.simulate_click(point(px(772.0), px(250.0)), MouseButton::Left);
        assert_eq!(window.read(|app, _| app.mode), ThemeMode::Dark);
    }
}
