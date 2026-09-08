mod bottom_panel;
mod capture;
mod conversations;
mod image_preview;
mod project_creation;
mod render;
mod review;
mod right_panel;
mod sidebar;
mod state;
mod workspace_panels;

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use gpui::{Context, Entity, WindowAppearance, prelude::*};

use state::{
    BottomPanelMode, BottomPanelState, ImagePreviewState, ProjectCreationState, RightPanelMode,
    RightPanelState, SidebarLayoutState,
};

gpui::actions!(
    permission_ui,
    [DismissPermissionUi, ToggleTerminal, ToggleReview, OpenFiles]
);

use crate::{
    agent::{AgentBackend, CodexAppServerBackend, CodexAppServerManager, ProjectId, ThreadId},
    components::{
        composer::{
            ComposerView, ConversationThreadCreated, ModelCatalogLoadFinished,
            RequestFullAccessConfirmation,
        },
        file_panel::FilePanel,
        home::{
            HomeView, OpenDiffReview, OpenImagePreview, OpenSubAgentPanel, RetryImageGeneration,
        },
        review_panel::ReviewPanel,
        sidebar::{NewConversation, OpenProjectCreation, OpenSettings, SelectThread, SidebarView},
        terminal::TerminalPanel,
    },
    media::read_image_dimensions,
    settings::{ChangeTheme, CloseSettings, SettingsView},
    theme::ThemeMode,
    workspace::WorkspaceStore,
};

pub struct ChatApp {
    codex_app_server: Arc<CodexAppServerManager>,
    agent_backend: Arc<dyn AgentBackend>,
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
    sidebar_layout: SidebarLayoutState,
    bottom_panel: BottomPanelState,
    terminal_panels: HashMap<ConversationKey, Entity<TerminalPanel>>,
    file_panels: HashMap<ConversationKey, Entity<FilePanel>>,
    review_panels: HashMap<ConversationKey, Entity<ReviewPanel>>,
    file_close_prompt_open: bool,
    terminal_return_focus_pending: bool,
    right_panel: RightPanelState,
    image_preview: ImagePreviewState,
    permission_confirmation_open: bool,
    project_creation: ProjectCreationState,
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

impl Drop for ChatApp {
    fn drop(&mut self) {
        #[cfg(not(test))]
        crate::git_review::shutdown();
        self.codex_app_server.shutdown();
    }
}

const BOTTOM_PANEL_ITEMS: &[(BottomPanelMode, &str, &str, &str)] = &[
    (BottomPanelMode::Review, "审查", "⌃⇧G", "panel-review"),
    (BottomPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
    (BottomPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (BottomPanelMode::Files, "文件", "⌘P", "panel-files"),
    (BottomPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
];
const BOTTOM_PANEL_HEIGHT: f32 = 280.0;

const RIGHT_PANEL_ITEMS: &[(RightPanelMode, &str, &str, &str)] = &[
    (RightPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
    (RightPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (RightPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
    (RightPanelMode::Files, "文件", "⌘P", "panel-files"),
    (RightPanelMode::Review, "审查", "⌃⇧G", "panel-review"),
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
            for panel in this.file_panels.values() {
                panel.update(cx, |panel, cx| panel.set_mode(event.0, cx));
            }
            for panel in this.review_panels.values() {
                panel.update(cx, |panel, cx| panel.set_mode(event.0, cx));
            }
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
            if let Some(panel) = &this.right_panel.subagent {
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
            this.image_preview.path = Some(event.0.clone());
            this.image_preview.dimensions = read_image_dimensions(&event.0).ok().flatten();
            this.image_preview.zoom = 1.0;
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
            agent_backend,
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
            sidebar_layout: SidebarLayoutState::default(),
            bottom_panel: BottomPanelState::new(cx),
            terminal_panels: HashMap::new(),
            file_panels: HashMap::new(),
            review_panels: HashMap::new(),
            file_close_prompt_open: false,
            terminal_return_focus_pending: false,
            right_panel: RightPanelState::new(cx),
            image_preview: ImagePreviewState::new(cx),
            permission_confirmation_open: false,
            project_creation: ProjectCreationState::new(cx),
        }
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
}

impl ChatApp {}

#[cfg(not(test))]
use render::startup_sidebar_resolved;

#[cfg(test)]
mod tests;
