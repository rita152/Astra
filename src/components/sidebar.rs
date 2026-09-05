use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, App, Bounds, ContentMask, Context, Div, Entity, Focusable, Hsla,
    IntoElement, MouseButton, MouseDownEvent, Pixels, Render, ScrollHandle, ShapedLine,
    SharedString, TextAlign, TextRun, Transformation, Window, canvas, deferred, div, point,
    prelude::*, px, radians, size,
};

use crate::{
    agent::{
        AgentCapability, Project, ProjectId, ThreadActivity, ThreadId, ThreadSummary, UpdateProject,
    },
    components::{
        icons::{chevron, icon},
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
    },
    theme::{Theme, ThemeMode},
    workspace::{WorkspaceSnapshot, WorkspaceStore, project_id_for_thread},
};

pub struct OpenSettings;
pub struct OpenProjectCreation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectThread {
    pub thread_id: ThreadId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewConversation {
    pub project_id: Option<ProjectId>,
    pub cwd: PathBuf,
}

impl gpui::EventEmitter<OpenSettings> for SidebarView {}
impl gpui::EventEmitter<OpenProjectCreation> for SidebarView {}
impl gpui::EventEmitter<SelectThread> for SidebarView {}
impl gpui::EventEmitter<NewConversation> for SidebarView {}

// These values come from the live ChatGPT desktop app at 127.0.0.1:9222.
const SIDEBAR_WIDTH: f32 = 240.0;
const SIDEBAR_TITLEBAR_SAFE_TOP: f32 = 46.0;
const ROW_HEIGHT: f32 = 30.0;
const ROW_RADIUS: f32 = 12.5;
const ROW_HORIZONTAL_PADDING: f32 = 8.0;
const SECTION_HEADER_HEIGHT: f32 = 25.0;
const SEARCH_PANEL_WIDTH: f32 = 520.0;
const SEARCH_PANEL_HEIGHT: f32 = 487.0;
const SEARCH_INPUT_HEIGHT: f32 = 38.0;
const SEARCH_RESULT_HEIGHT: f32 = 31.0;
const MAX_VISIBLE_PROJECT_THREADS: usize = 5;
const MAX_VISIBLE_RECENTS: usize = 10;
const SIDEBAR_BODY_FONT_WEIGHT: gpui::FontWeight = crate::theme::UI_BODY_FONT_WEIGHT;
const MARQUEE_HOVER_DELAY: Duration = Duration::from_millis(350);
const MARQUEE_SPEED: f32 = 28.0;
const PROJECT_THREAD_TITLE_INSETS: f32 = 120.0;
const RECENT_THREAD_TITLE_INSETS: f32 = 96.0;
const THREAD_ACTION_RAIL_INSETS: f32 = 51.0;
const TITLE_FADE_IN: f32 = 8.0;
const TITLE_FADE_OUT: f32 = 16.0;

fn sidebar_thread_title_viewport_width(flat: bool, show_actions: bool) -> f32 {
    let action_insets = if flat {
        RECENT_THREAD_TITLE_INSETS
    } else {
        PROJECT_THREAD_TITLE_INSETS
    };
    let insets = if show_actions {
        action_insets
    } else {
        action_insets - THREAD_ACTION_RAIL_INSETS
    };
    (SIDEBAR_WIDTH - insets).max(0.0)
}

fn faded_sidebar_text_color(color: Hsla, fade: f32) -> Hsla {
    color.alpha(color.a * fade)
}

fn thread_title_canvas(
    title: SharedString,
    color: Hsla,
    scroll_offset: f32,
    overflows: bool,
) -> impl IntoElement {
    canvas(
        move |_, window, _| {
            let mut font = window.text_style().font();
            font.family = ".SystemUIFont".into();
            font.weight = SIDEBAR_BODY_FONT_WEIGHT;
            let shape = |alpha: f32| {
                window.text_system().shape_line(
                    title.clone(),
                    px(14.0),
                    &[TextRun {
                        len: title.len(),
                        font: font.clone(),
                        color: faded_sidebar_text_color(color, alpha),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
            };
            let opaque = shape(1.0);
            let left = overflows.then(|| {
                (0..TITLE_FADE_IN as usize)
                    .map(|index| shape((index as f32 + 0.5) / TITLE_FADE_IN))
                    .collect::<Vec<_>>()
            });
            let right = overflows.then(|| {
                (0..TITLE_FADE_OUT as usize)
                    .map(|index| shape(1.0 - (index as f32 + 0.5) / TITLE_FADE_OUT))
                    .collect::<Vec<_>>()
            });
            (opaque, left, right)
        },
        move |bounds, (opaque, left, right): (ShapedLine, _, _), window, cx| {
            let origin = point(
                bounds.origin.x + px(TITLE_FADE_IN - scroll_offset),
                bounds.origin.y,
            );
            let paint =
                |line: &ShapedLine, mask: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                        line.paint(origin, px(20.0), TextAlign::Left, None, window, cx)
                            .expect("sidebar title glyphs should paint")
                    });
                };
            if let (Some(left), Some(right)) = (left.as_ref(), right.as_ref()) {
                let center_left = bounds.origin.x + px(TITLE_FADE_IN);
                let center_right = bounds.right() - px(TITLE_FADE_OUT);
                if center_right > center_left {
                    paint(
                        &opaque,
                        Bounds::from_corners(
                            point(center_left, bounds.origin.y),
                            point(center_right, bounds.bottom()),
                        ),
                        window,
                        cx,
                    );
                }
                for (index, line) in left.iter().enumerate() {
                    let x = bounds.origin.x + px(index as f32);
                    paint(
                        line,
                        Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
                        window,
                        cx,
                    );
                }
                for (index, line) in right.iter().enumerate() {
                    let x = bounds.right() - px(TITLE_FADE_OUT) + px(index as f32);
                    paint(
                        line,
                        Bounds::new(point(x, bounds.origin.y), size(px(1.0), bounds.size.height)),
                        window,
                        cx,
                    );
                }
            } else {
                paint(&opaque, bounds, window, cx);
            }
        },
    )
    .h_full()
    .min_w(px(0.0))
    .ml(px(-TITLE_FADE_IN))
    .flex_1()
}

fn marquee_offset(scroll_distance: f32, elapsed: Duration, reduce_motion: bool) -> f32 {
    if reduce_motion || scroll_distance <= 0.0 || elapsed <= MARQUEE_HOVER_DELAY {
        return 0.0;
    }
    let travel_time = scroll_distance / MARQUEE_SPEED;
    let progress = ((elapsed - MARQUEE_HOVER_DELAY).as_secs_f32() / travel_time).clamp(0.0, 1.0);
    scroll_distance * marquee_ease(progress)
}

fn marquee_duration(scroll_distance: f32) -> Duration {
    MARQUEE_HOVER_DELAY + Duration::from_secs_f32(scroll_distance.max(0.0) / MARQUEE_SPEED)
}

fn marquee_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.5, 0.7) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 0.6, 1.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RenameTarget {
    Project(ProjectId),
    Thread(ThreadId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DeleteTarget {
    Project(ProjectId),
    Thread(ThreadId),
}

pub struct SidebarView {
    mode: ThemeMode,
    store: Arc<WorkspaceStore>,
    snapshot: WorkspaceSnapshot,
    scroll: ScrollHandle,
    activity_scroll: ScrollHandle,
    scroll_to_bottom: bool,
    selected_project_id: Option<ProjectId>,
    selected_thread_id: Option<ThreadId>,
    hovered_thread_id: Option<ThreadId>,
    hovered_project_id: Option<ProjectId>,
    hovered_section_id: Option<&'static str>,
    marquee_started_at: Option<Instant>,
    marquee_animation_ends_at: Option<Instant>,
    marquee_animation_running: bool,
    show_all_projects: BTreeSet<ProjectId>,
    search_input: Entity<PromptInput>,
    rename_input: Entity<PromptInput>,
    search_open: bool,
    search_focus_pending: bool,
    rename_target: Option<RenameTarget>,
    rename_focus_pending: bool,
    project_menu_id: Option<ProjectId>,
    thread_menu_id: Option<ThreadId>,
    menu_origin: (f32, f32),
    delete_confirmation: Option<DeleteTarget>,
    projects_section_menu_open: bool,
    pinned_menu_open: bool,
    profile_menu_open: bool,
    activity_open: bool,
    archived_open: bool,
    local_error: Option<String>,
}

impl SidebarView {
    pub fn new(
        mode: ThemeMode,
        scroll_to_bottom: bool,
        store: Arc<WorkspaceStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = store.snapshot();
        let search_input = cx.new(|cx| PromptInput::inline_other(mode, "搜索聊天", false, cx));
        let rename_input = cx.new(|cx| PromptInput::inline_other(mode, "名称", false, cx));
        cx.subscribe(&search_input, |this, input, _: &PromptChanged, cx| {
            this.store.search(input.read(cx).text().to_owned());
        })
        .detach();
        cx.subscribe(&search_input, |this, _, _: &PromptSubmitted, cx| {
            if let Some(result) = this.snapshot.search_results.first() {
                this.select_thread(result.thread.thread_id.clone(), cx);
            }
        })
        .detach();
        cx.subscribe(&rename_input, |this, _, event: &PromptSubmitted, cx| {
            this.commit_rename(event.0.clone(), cx);
        })
        .detach();
        let receiver = store.subscribe();
        cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = receiver.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.snapshot = snapshot;
                    cx.notify();
                });
            }
        })
        .detach();
        Self {
            mode,
            store,
            snapshot,
            scroll: ScrollHandle::new(),
            activity_scroll: ScrollHandle::new(),
            scroll_to_bottom,
            selected_project_id: None,
            selected_thread_id: None,
            hovered_thread_id: None,
            hovered_project_id: None,
            hovered_section_id: None,
            marquee_started_at: None,
            marquee_animation_ends_at: None,
            marquee_animation_running: false,
            show_all_projects: BTreeSet::new(),
            search_input,
            rename_input,
            search_open: false,
            search_focus_pending: false,
            rename_target: None,
            rename_focus_pending: false,
            project_menu_id: None,
            thread_menu_id: None,
            menu_origin: (0.0, 0.0),
            delete_confirmation: None,
            projects_section_menu_open: false,
            pinned_menu_open: false,
            profile_menu_open: false,
            activity_open: false,
            archived_open: false,
            local_error: None,
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.search_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        self.rename_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn width(&self) -> f32 {
        SIDEBAR_WIDTH
    }

    pub fn set_profile_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.close_transient_menus(cx);
        self.profile_menu_open = open;
        cx.notify();
    }

    pub fn set_activity_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.close_transient_menus(cx);
        self.activity_open = open;
        self.archived_open = false;
        cx.notify();
    }

    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.activity_scroll.set_offset(point(px(0.0), px(-offset)));
        cx.notify();
    }

    pub fn set_activity_hovered_thread_for_capture(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) {
        self.hovered_thread_id = self
            .snapshot
            .recent_threads
            .iter()
            .any(|thread| thread.thread_id == thread_id)
            .then_some(thread_id);
        cx.notify();
    }

    pub fn open_projects_section_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.projects_section_menu_open = true;
        cx.notify();
    }

    pub fn close_transient_menus(&mut self, cx: &mut Context<Self>) {
        self.project_menu_id = None;
        self.thread_menu_id = None;
        self.projects_section_menu_open = false;
        self.pinned_menu_open = false;
        self.profile_menu_open = false;
        self.delete_confirmation = None;
        cx.notify();
    }

    #[cfg(test)]
    pub fn projects_section_menu_is_open(&self) -> bool {
        self.projects_section_menu_open
    }

    #[cfg(test)]
    pub fn project_menu_is_open(&self) -> bool {
        self.project_menu_id.is_some()
    }

    #[cfg(test)]
    pub fn pinned_menu_is_open(&self) -> bool {
        self.pinned_menu_open
    }

    pub fn open_project_menu_for_capture(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        if self
            .snapshot
            .projects
            .iter()
            .any(|project| project.project_id == project_id)
        {
            self.project_menu_id = Some(project_id);
            self.menu_origin = (172.0, 160.0);
            cx.notify();
        }
    }

    /// Mirrors the visual state produced by a real sidebar thread selection
    /// without emitting a second navigation event. The capture resume path
    /// drives the matching conversation load directly from `ChatApp`.
    pub fn select_thread_for_capture(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread_id = Some(thread_id);
        self.search_open = false;
        self.activity_open = false;
        self.archived_open = false;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        cx.notify();
    }

    #[cfg(feature = "screenshot")]
    pub fn resumed_thread_ready_for_capture(&self, thread_id: &str) -> Result<bool, String> {
        if let Some(error) = &self.snapshot.error {
            return Err(format!("侧栏数据加载失败：{error}"));
        }
        Ok(self.selected_thread_id.as_deref() == Some(thread_id)
            && (cfg!(test) || self.snapshot.thread(thread_id).is_some())
            && !self.snapshot.loading.projects
            && !self.snapshot.loading.recent
            && !self.snapshot.loading.pinned)
    }

    fn select_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.selected_thread_id = Some(thread_id.clone());
        self.search_open = false;
        self.activity_open = false;
        self.archived_open = false;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        cx.emit(SelectThread { thread_id });
        cx.notify();
    }

    fn new_conversation(&mut self, project: Option<&Project>, cx: &mut Context<Self>) {
        let project_id = project.map(|project| project.project_id.clone());
        let cwd = project
            .and_then(|project| project.roots.first().cloned())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.selected_project_id = project_id.clone();
        self.selected_thread_id = None;
        self.search_open = false;
        cx.emit(NewConversation { project_id, cwd });
        cx.notify();
    }

    fn start_rename(&mut self, target: RenameTarget, current: String, cx: &mut Context<Self>) {
        self.rename_target = Some(target);
        self.rename_input
            .update(cx, |input, cx| input.set_text_silently(current, cx));
        self.rename_focus_pending = true;
        self.project_menu_id = None;
        self.thread_menu_id = None;
        self.delete_confirmation = None;
        cx.notify();
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(target) = self.rename_target.take() else {
            return;
        };
        match target {
            RenameTarget::Project(project_id) => self.store.update_project(
                project_id,
                UpdateProject {
                    name: Some(name),
                    roots: None,
                },
            ),
            RenameTarget::Thread(thread_id) => self.store.rename_thread(thread_id, name),
        }
        self.rename_input.update(cx, |input, cx| input.clear(cx));
        cx.notify();
    }

    fn reveal_in_finder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg("-R").arg(path).spawn();
        #[cfg(not(target_os = "macos"))]
        let result = Command::new("xdg-open").arg(path).spawn();
        self.local_error = result
            .err()
            .map(|error| format!("无法在 Finder 中显示：{error}"));
        self.project_menu_id = None;
        cx.notify();
    }

    fn confirm_or_delete(&mut self, target: DeleteTarget, cx: &mut Context<Self>) {
        if self.delete_confirmation.as_ref() == Some(&target) {
            match target {
                DeleteTarget::Project(project_id) => self.store.delete_project(project_id),
                DeleteTarget::Thread(thread_id) => self.store.delete_thread(thread_id),
            }
            self.delete_confirmation = None;
            self.project_menu_id = None;
            self.thread_menu_id = None;
        } else {
            self.delete_confirmation = Some(target);
        }
        cx.notify();
    }

    fn nav_icon_button(
        id: impl Into<gpui::ElementId>,
        glyph: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .size(px(24.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(theme.sidebar_icon_muted)
            .hover(move |style| style.bg(theme.sidebar_hover).text_color(theme.sidebar_text))
            .child(icon(glyph, theme.sidebar_icon_muted.into()).size(px(16.0)))
    }

    fn action_icon_button(
        id: impl Into<gpui::ElementId>,
        glyph: &'static str,
        theme: Theme,
        pending: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w(px(19.0))
            .h(px(20.0))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .when(pending, |button| button.opacity(0.4).cursor_default())
            .when(!pending, |button| {
                button.cursor_pointer().hover(move |style| {
                    style.bg(theme.sidebar_hover).text_color(theme.sidebar_text)
                })
            })
            .child(icon(glyph, theme.sidebar_icon_muted.into()).size(px(16.0)))
    }

    fn static_nav_row(
        id: &'static str,
        label: &'static str,
        glyph: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .flex_none()
            .relative()
            .top(px(1.0))
            .h(px(30.0))
            .w_full()
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(10.0))
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(icon(glyph, theme.sidebar_text.into()))
            .child(div().relative().left(px(0.25)).child(label))
            .child(div().flex_1())
    }

    fn section_header(
        &self,
        id: &'static str,
        label: &'static str,
        collapsed: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hovered = self.hovered_section_id == Some(id);
        div()
            .id(id)
            .h(px(SECTION_HEADER_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(10.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(theme.sidebar_text_muted)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(label)
                    .child(
                        icon("section-chevron", theme.sidebar_icon_muted.into())
                            .size(px(14.0))
                            .when(!hovered, |chevron| chevron.invisible())
                            .with_transformation(Transformation::rotate(radians(if collapsed {
                                -std::f32::consts::FRAC_PI_2
                            } else {
                                0.0
                            }))),
                    ),
            )
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                if *is_hovered {
                    this.hovered_section_id = Some(id);
                } else if this.hovered_section_id == Some(id) {
                    this.hovered_section_id = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, _| match label {
                "置顶" => this.store.set_section_collapsed("pinned", !collapsed),
                "项目" => this.store.set_section_collapsed("projects", !collapsed),
                "最近" => this.store.set_section_collapsed("recent", !collapsed),
                _ => {}
            }))
    }

    fn status_row(
        &self,
        id: impl Into<gpui::ElementId>,
        text: impl Into<SharedString>,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .h(px(ROW_HEIGHT))
            .px(px(ROW_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .text_color(theme.sidebar_text_muted)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(text.into())
    }

    fn retryable_error_row(
        &self,
        id: &'static str,
        error: String,
        search_query: Option<String>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .min_h(px(ROW_HEIGHT))
            .px(px(ROW_HORIZONTAL_PADDING))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(13.0))
            .text_color(theme.warning)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(error),
            )
            .child(
                div()
                    .id(format!("{id}-retry"))
                    .rounded(px(7.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .cursor_pointer()
                    .text_color(theme.sidebar_text)
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .child("重试")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(query) = &search_query {
                            this.store.search(query.clone());
                        } else {
                            this.store.retry();
                        }
                        cx.notify();
                    })),
            )
    }

    fn thread_title_width(text: &str, window: &mut Window) -> f32 {
        let mut font = window.text_style().font();
        font.family = ".SystemUIFont".into();
        font.weight = SIDEBAR_BODY_FONT_WEIGHT;
        let run = TextRun {
            len: text.len(),
            font,
            color: window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        f32::from(
            window
                .text_system()
                .shape_line(text.to_owned().into(), px(14.0), &[run], None)
                .width(),
        )
    }

    fn start_marquee(
        &mut self,
        text: &str,
        viewport_width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        self.marquee_started_at = Some(now);
        let distance = (Self::thread_title_width(text, window) - viewport_width).max(0.0);
        self.marquee_animation_ends_at = (distance > 0.0).then(|| now + marquee_duration(distance));
        if distance > 0.0 && !cx.reduce_motion() && !self.marquee_animation_running {
            self.marquee_animation_running = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_marquee_animation(window, cx)
            });
        }
    }

    fn stop_marquee(&mut self) {
        self.marquee_started_at = None;
        self.marquee_animation_ends_at = None;
        self.marquee_animation_running = false;
    }

    fn advance_marquee_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.marquee_animation_running {
            return;
        }
        let now = cx.background_executor().now();
        if cx.reduce_motion()
            || self
                .marquee_animation_ends_at
                .is_none_or(|animation_end| now >= animation_end)
            || self.hovered_thread_id.is_none()
        {
            self.marquee_animation_running = false;
            cx.notify();
            return;
        }
        cx.notify();
        cx.on_next_frame(window, |this, window, cx| {
            this.advance_marquee_animation(window, cx)
        });
    }

    fn thread_row(
        &self,
        thread: &ThreadSummary,
        indented: bool,
        pinned: bool,
        archived: bool,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let thread_id = thread.thread_id.clone();
        let selected = self.selected_thread_id.as_deref() == Some(thread_id.as_str());
        let hovered = self.hovered_thread_id.as_deref() == Some(thread_id.as_str());
        let pending = self.snapshot.is_pending_thread(&thread_id);
        let rename_active =
            self.rename_target.as_ref() == Some(&RenameTarget::Thread(thread_id.clone()));
        let active = matches!(thread.activity, ThreadActivity::Active { .. });
        let status_error = matches!(thread.activity, ThreadActivity::SystemError);
        let can_pin = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadSectionMove)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionList)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionCreate);
        let can_archive = self.snapshot.capabilities.supports(if archived {
            AgentCapability::ThreadUnarchive
        } else {
            AgentCapability::ThreadArchive
        });
        let pin_id = thread_id.clone();
        let pin = Self::action_icon_button(
            format!("thread-pin-{thread_id}"),
            "pin",
            theme,
            pending || !can_pin,
        )
        .when(!pending && can_pin, |button| {
            button.on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.store.set_thread_pinned(pin_id.clone(), !pinned);
            }))
        });
        let archive_id = thread_id.clone();
        let archive_button = Self::action_icon_button(
            format!("thread-archive-{thread_id}"),
            "archive",
            theme,
            pending || !can_archive,
        )
        .when(!pending && can_archive, |button| {
            button.on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if archived {
                    this.store.unarchive_thread(archive_id.clone());
                } else {
                    this.store.archive_thread(archive_id.clone());
                }
            }))
        });
        let actions = div()
            .absolute()
            .right(px(8.0))
            .top(px(5.0))
            .flex()
            .gap(px(8.0))
            .when(!hovered, |actions| actions.invisible())
            .child(pin)
            .child(archive_button);
        let title_viewport_width = sidebar_thread_title_viewport_width(!indented, hovered);
        let title_width = Self::thread_title_width(&thread.title, window);
        let title_scroll_distance = if hovered {
            (title_width - title_viewport_width).max(0.0)
        } else {
            0.0
        };
        let title_scroll_offset = self.marquee_started_at.map_or(0.0, |started_at| {
            marquee_offset(
                title_scroll_distance,
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started_at),
                cx.reduce_motion(),
            )
        });
        let trailing_rail = div()
            .ml(px(3.0))
            .flex_none()
            .when(hovered, |rail| rail.w(px(48.0)).min_w(px(48.0)))
            .when(active && !hovered, |rail| rail.w(px(25.0)).min_w(px(25.0)))
            .when(status_error && !hovered && !active, |rail| {
                rail.w(px(16.0)).min_w(px(16.0))
            })
            .when(!hovered && !active && !status_error, |rail| {
                rail.w(px(0.0)).min_w(px(0.0))
            });
        let title = if rename_active {
            self.rename_input.clone().into_any_element()
        } else {
            div()
                .min_w(px(0.0))
                .flex_1()
                .h(px(20.0))
                .child(thread_title_canvas(
                    thread.title.clone().into(),
                    theme.sidebar_text.into(),
                    title_scroll_offset,
                    title_width > title_viewport_width,
                ))
                .into_any_element()
        };
        let hover_id = thread_id.clone();
        let select_id = thread_id.clone();
        let context_id = thread_id.clone();
        let marquee_title = thread.title.clone();
        let spinner = icon("dictation-spinner", theme.sidebar_icon_muted.into())
            .size(px(20.0))
            .with_animation(
                format!("thread-running-{thread_id}"),
                Animation::new(Duration::from_millis(800)).repeat(),
                |spinner, progress| {
                    spinner.with_transformation(Transformation::rotate(radians(
                        progress * std::f32::consts::TAU,
                    )))
                },
            );
        div()
            .id(format!("thread-row-{thread_id}"))
            .h(px(ROW_HEIGHT))
            .relative()
            .top(px(1.0))
            .pl(px(8.0))
            .pr(px(5.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .text_size(px(14.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.sidebar_text)
            .overflow_hidden()
            .whitespace_nowrap()
            .when(pending, |row| row.opacity(0.4).cursor_default())
            .when(!pending, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
            })
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .h_full()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .when(indented, |title_row| {
                                title_row.gap(px(8.0)).child(div().w(px(16.0)).flex_none())
                            })
                            .child(title),
                    )
                    .child(trailing_rail),
            )
            .when(active && !hovered, |row| {
                row.child(
                    div()
                        .absolute()
                        .right(px(5.0))
                        .top(px(5.0))
                        .size(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(spinner),
                )
            })
            .when(status_error && !hovered, |row| {
                row.child(
                    div()
                        .absolute()
                        .right(px(12.0))
                        .size(px(6.0))
                        .rounded_full()
                        .bg(theme.warning),
                )
            })
            .child(actions)
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.hovered_thread_id = Some(hover_id.clone());
                    this.start_marquee(
                        &marquee_title,
                        sidebar_thread_title_viewport_width(!indented, true),
                        window,
                        cx,
                    );
                } else if this.hovered_thread_id.as_deref() == Some(hover_id.as_str()) {
                    this.hovered_thread_id = None;
                    this.stop_marquee();
                }
                cx.notify();
            }))
            .when(!pending && !rename_active, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.select_thread(select_id.clone(), cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.thread_menu_id = Some(context_id.clone());
                        this.project_menu_id = None;
                        this.delete_confirmation = None;
                        this.menu_origin =
                            (f32::from(event.position.x), f32::from(event.position.y));
                        cx.notify();
                    }),
                )
            })
    }

    fn project_group(
        &self,
        project: &Project,
        pinned_ids: &HashSet<&str>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.project_id.clone();
        let collapsed = self
            .snapshot
            .preferences
            .collapsed_project_ids
            .contains(&project_id);
        let pending = self.snapshot.is_pending_project(&project_id);
        let hovered = self.hovered_project_id.as_deref() == Some(project_id.as_str());
        let rename_active =
            self.rename_target.as_ref() == Some(&RenameTarget::Project(project_id.clone()));
        let menu_open = self.project_menu_id.as_deref() == Some(project_id.as_str());
        let threads = self
            .snapshot
            .recent_threads
            .iter()
            .filter(|thread| {
                project_id_for_thread(thread, &self.snapshot.projects).as_deref()
                    == Some(project_id.as_str())
                    && !pinned_ids.contains(thread.thread_id.as_str())
            })
            .collect::<Vec<_>>();
        let show_all = self.show_all_projects.contains(&project_id);
        let toggle_id = project_id.clone();
        let hover_id = project_id.clone();
        let menu_id = project_id.clone();
        let new_chat_project = project.clone();
        let actions = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!hovered && !menu_open, |actions| actions.invisible())
            .child(
                Self::nav_icon_button(
                    format!("project-menu-{project_id}"),
                    "more-horizontal",
                    theme,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.project_menu_id = Some(menu_id.clone());
                        this.thread_menu_id = None;
                        this.delete_confirmation = None;
                        this.menu_origin =
                            (f32::from(event.position.x), f32::from(event.position.y));
                        cx.notify();
                    }),
                ),
            )
            .child(
                Self::nav_icon_button(format!("project-new-chat-{project_id}"), "new-chat", theme)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.new_conversation(Some(&new_chat_project), cx);
                    })),
            );
        let title = if rename_active {
            self.rename_input.clone().into_any_element()
        } else {
            div()
                .min_w(px(0.0))
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(project.name.clone())
                .into_any_element()
        };
        let mut group = div()
            .id(format!("project-group-{project_id}"))
            .flex()
            .flex_col()
            .gap(px(1.0));
        group = group.child(
            div()
                .id(format!("project-row-{project_id}"))
                .h(px(ROW_HEIGHT))
                .w_full()
                .relative()
                .pl(px(1.0))
                .pr(px(6.0))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_size(px(14.0))
                .text_color(theme.sidebar_text)
                .when(pending, |row| row.opacity(0.4).cursor_default())
                .when(!pending, |row| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                })
                .child(
                    div()
                        .size(px(30.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("folder", theme.sidebar_text.into())),
                )
                .child(title)
                .child(actions)
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.hovered_project_id = hovered.then(|| hover_id.clone());
                    cx.notify();
                }))
                .when(!pending && !rename_active, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_project_id = Some(toggle_id.clone());
                        this.store
                            .set_project_collapsed(toggle_id.clone(), !collapsed);
                        cx.notify();
                    }))
                }),
        );
        if !collapsed {
            let visible = if show_all {
                threads.len()
            } else {
                threads.len().min(MAX_VISIBLE_PROJECT_THREADS)
            };
            for thread in threads.iter().take(visible) {
                group = group.child(self.thread_row(thread, true, false, false, theme, window, cx));
            }
            if threads.len() > MAX_VISIBLE_PROJECT_THREADS {
                let show_id = project_id.clone();
                group = group.child(
                    div()
                        .id(format!("project-show-all-{project_id}"))
                        .h(px(ROW_HEIGHT))
                        .pl(px(32.0))
                        .flex()
                        .items_center()
                        .text_size(px(14.0))
                        .text_color(theme.sidebar_text_muted)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(theme.sidebar_text))
                        .child(if show_all { "收起" } else { "展开显示" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.show_all_projects.insert(show_id.clone()) {
                                this.show_all_projects.remove(&show_id);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        group
    }

    fn pinned_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.pinned_collapsed;
        let mut section = div()
            .id("pinned-section")
            .pt(px(10.0))
            .flex()
            .flex_col()
            .child(self.section_header("pinned-heading", "置顶", collapsed, theme, cx));
        if collapsed {
            return section;
        }
        if self.snapshot.loading.pinned && self.snapshot.pinned_threads.is_empty() {
            return section.child(self.status_row("pinned-loading", "正在加载…", theme));
        }
        for thread in &self.snapshot.pinned_threads {
            section = section.child(self.thread_row(thread, false, true, false, theme, window, cx));
        }
        section
    }

    fn projects_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.projects_collapsed;
        let heading_hovered = self.hovered_section_id == Some("projects-heading");
        let show_heading_actions = heading_hovered || self.projects_section_menu_open;
        let can_create = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectCreate);
        let pinned_ids = self
            .snapshot
            .pinned_threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect::<HashSet<_>>();
        let menu_button = Self::nav_icon_button("projects-options", "more-horizontal", theme)
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.projects_section_menu_open = !this.projects_section_menu_open;
                this.project_menu_id = None;
                cx.notify();
            }));
        let add_button = Self::nav_icon_button("project-add", "add", theme)
            .when(!can_create, |button| button.opacity(0.4).cursor_default())
            .when(can_create, |button| {
                button.on_click(cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(OpenProjectCreation);
                }))
            });
        let mut section = div().id("projects-section").pt(px(10.0)).flex().flex_col();
        section = section.child(
            div()
                .id("projects-section-heading-row")
                .h(px(SECTION_HEADER_HEIGHT))
                .px(px(8.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.sidebar_text_muted)
                .child(
                    div()
                        .id("projects-heading")
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .cursor_pointer()
                        .child("项目")
                        .child(
                            icon("section-chevron", theme.sidebar_icon_muted.into())
                                .size(px(14.0))
                                .when(!show_heading_actions, |chevron| chevron.invisible())
                                .with_transformation(Transformation::rotate(radians(
                                    if collapsed {
                                        -std::f32::consts::FRAC_PI_2
                                    } else {
                                        0.0
                                    },
                                ))),
                        )
                        .on_click(cx.listener(move |this, _, _, _| {
                            this.store.set_section_collapsed("projects", !collapsed);
                        })),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .when(!show_heading_actions, |actions| actions.invisible())
                        .child(menu_button)
                        .child(add_button),
                )
                .on_hover(cx.listener(|this, is_hovered: &bool, _, cx| {
                    if *is_hovered {
                        this.hovered_section_id = Some("projects-heading");
                    } else if this.hovered_section_id == Some("projects-heading") {
                        this.hovered_section_id = None;
                    }
                    cx.notify();
                })),
        );
        if collapsed {
            return section;
        }
        if self.snapshot.loading.projects && self.snapshot.projects.is_empty() {
            return section.child(self.status_row("projects-loading", "正在加载…", theme));
        }
        let creating = self.snapshot.pending.iter().any(|operation| {
            matches!(
                operation,
                crate::workspace::WorkspaceOperation::CreateProject(_)
            )
        });
        if self.snapshot.projects.is_empty() {
            return section.child(self.status_row(
                "projects-empty",
                if creating {
                    "正在创建项目…"
                } else {
                    "暂无项目"
                },
                theme,
            ));
        }
        let mut projects = div().flex().flex_col().gap(px(10.0));
        for project in &self.snapshot.projects {
            projects = projects.child(self.project_group(project, &pinned_ids, theme, window, cx));
        }
        section = section.child(projects);
        if creating {
            section = section.child(self.status_row("project-creating", "正在创建项目…", theme));
        }
        section
    }

    fn recent_section(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let collapsed = self.snapshot.preferences.recent_collapsed;
        let pinned_ids = self
            .snapshot
            .pinned_threads
            .iter()
            .map(|thread| thread.thread_id.as_str())
            .collect::<HashSet<_>>();
        let threads = self
            .snapshot
            .recent_threads
            .iter()
            .filter(|thread| {
                project_id_for_thread(thread, &self.snapshot.projects).is_none()
                    && !pinned_ids.contains(thread.thread_id.as_str())
            })
            .take(MAX_VISIBLE_RECENTS)
            .collect::<Vec<_>>();
        let mut section = div()
            .id("recent-section")
            .pt(px(10.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .child(self.section_header("recent-heading", "最近", collapsed, theme, cx));
        if collapsed {
            return section;
        }
        if self.snapshot.loading.recent && self.snapshot.recent_threads.is_empty() {
            return section.child(self.status_row("recent-loading", "正在加载…", theme));
        }
        if threads.is_empty() {
            return section.child(self.status_row("recent-empty", "暂无最近聊天", theme));
        }
        for thread in threads {
            section =
                section.child(self.thread_row(thread, false, false, false, theme, window, cx));
        }
        section
    }

    fn activity_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut seen = HashSet::new();
        let active = self
            .snapshot
            .pinned_threads
            .iter()
            .chain(&self.snapshot.recent_threads)
            .filter(|thread| {
                seen.insert(thread.thread_id.as_str())
                    && matches!(
                        thread.activity,
                        ThreadActivity::Active { .. } | ThreadActivity::SystemError
                    )
            })
            .collect::<Vec<_>>();
        let mut content = div()
            .id("activity-content")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.activity_scroll)
            .px(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .h(px(SECTION_HEADER_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_text_muted)
                    .child("活动"),
            );
        if let Some(error) = self.snapshot.error.clone() {
            return content.child(self.retryable_error_row(
                "activity-error",
                error,
                None,
                theme,
                cx,
            ));
        }
        if (self.snapshot.loading.recent || self.snapshot.loading.pinned) && active.is_empty() {
            return content.child(self.status_row("activity-loading", "正在加载…", theme));
        }
        if active.is_empty() {
            return content.child(self.status_row("activity-empty", "暂无进行中的聊天", theme));
        }
        for thread in active {
            content =
                content.child(self.thread_row(thread, false, false, false, theme, window, cx));
        }
        content
    }

    fn archived_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut content = div()
            .id("archived-content")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(8.0))
            .pt(px(8.0))
            .child(
                div()
                    .h(px(SECTION_HEADER_HEIGHT))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_text_muted)
                    .child("已归档"),
            );
        if let Some(error) = self.snapshot.error.clone() {
            return content.child(self.retryable_error_row(
                "archived-error",
                error,
                None,
                theme,
                cx,
            ));
        }
        if self.snapshot.loading.archived && self.snapshot.archived_threads.is_empty() {
            return content.child(self.status_row("archived-loading", "正在加载…", theme));
        }
        if self.snapshot.archived_threads.is_empty() {
            return content.child(self.status_row("archived-empty", "暂无已归档聊天", theme));
        }
        for thread in &self.snapshot.archived_threads {
            content = content.child(self.thread_row(thread, false, false, true, theme, window, cx));
        }
        content
    }

    fn workspace_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut content = div()
            .id("workspace-content")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(8.0));
        if let Some(error) = self
            .local_error
            .as_ref()
            .or(self.snapshot.preference_error.as_ref())
            .or(self.snapshot.error.as_ref())
        {
            content = content.child(
                div()
                    .id("workspace-error")
                    .mx(px(8.0))
                    .my(px(6.0))
                    .p(px(8.0))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme.sidebar_hover)
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .text_size(px(13.0))
                    .text_color(theme.warning)
                    .child(error.clone())
                    .child(
                        div()
                            .id("workspace-retry")
                            .text_color(theme.sidebar_text)
                            .cursor_pointer()
                            .child("重试")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.local_error = None;
                                this.store.retry();
                                cx.notify();
                            })),
                    ),
            );
        }
        content
            .child(
                div()
                    .pb(px(21.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(Self::static_nav_row(
                        "sidebar-pull-requests",
                        "拉取请求",
                        "pull-request",
                        theme,
                    ))
                    .child(Self::static_nav_row(
                        "sidebar-sites",
                        "站点",
                        "sites",
                        theme,
                    ))
                    .child(Self::static_nav_row(
                        "sidebar-scheduled",
                        "已安排",
                        "scheduled",
                        theme,
                    ))
                    .child(Self::static_nav_row(
                        "sidebar-plugins",
                        "插件",
                        "plugins",
                        theme,
                    )),
            )
            .when(!self.snapshot.pinned_threads.is_empty(), |content| {
                content.child(self.pinned_section(theme, window, cx))
            })
            .child(self.projects_section(theme, window, cx))
            .child(self.recent_section(theme, window, cx))
    }

    fn menu_shell(
        &self,
        id: impl Into<gpui::ElementId>,
        width: f32,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w(px(width))
            .p(px(4.0))
            .rounded(px(10.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(10.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(24.0))
                    .spread_radius(px(-3.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .text_color(theme.sidebar_text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn menu_item(
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        glyph: &'static str,
        theme: Theme,
        enabled: bool,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .h(px(27.0))
            .px(px(6.0))
            .rounded(px(7.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .text_color(theme.sidebar_text)
            .when(enabled, |row| {
                row.cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
            })
            .when(!enabled, |row| row.opacity(0.4).cursor_default())
            .child(icon(glyph, theme.sidebar_text.into()).size(px(16.0)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(label.into()),
            )
    }

    fn menu_separator(theme: Theme) -> Div {
        div()
            .h(px(9.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .child(div().h(px(0.5)).w_full().bg(theme.border))
    }

    fn project_context_menu(
        &self,
        project: &Project,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let project_id = project.project_id.clone();
        let project_name = project.name.clone();
        let project_root = project.roots.first().cloned();
        let index = self
            .snapshot
            .projects
            .iter()
            .position(|candidate| candidate.project_id == project_id)
            .unwrap_or(0);
        let move_up_before = index
            .checked_sub(1)
            .and_then(|before| self.snapshot.projects.get(before))
            .map(|project| project.project_id.clone());
        let move_down_before = self
            .snapshot
            .projects
            .get(index + 2)
            .map(|project| project.project_id.clone());
        let can_move_up = index > 0;
        let can_move_down = index + 1 < self.snapshot.projects.len();
        let can_update = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectUpdate);
        let can_move = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectMove);
        let can_delete = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ProjectDelete);
        let rename_id = project_id.clone();
        let up_id = project_id.clone();
        let down_id = project_id.clone();
        let delete_id = project_id.clone();
        let deleting =
            self.delete_confirmation.as_ref() == Some(&DeleteTarget::Project(project_id.clone()));
        let mut menu = self
            .menu_shell(format!("project-context-{project_id}"), 190.0, theme)
            .child(
                Self::menu_item(
                    format!("project-rename-{project_id}"),
                    "重命名",
                    "settings-edit",
                    theme,
                    can_update,
                )
                .when(can_update, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.start_rename(
                            RenameTarget::Project(rename_id.clone()),
                            project_name.clone(),
                            cx,
                        );
                    }))
                }),
            );
        if let Some(root) = project_root {
            menu = menu.child(
                Self::menu_item(
                    format!("project-finder-{project_id}"),
                    "在 Finder 中显示",
                    "project-reveal",
                    theme,
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.reveal_in_finder(root.clone(), cx);
                })),
            );
        }
        menu = menu
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("project-up-{project_id}"),
                    "上移",
                    "settings-chevron-up",
                    theme,
                    can_move && can_move_up,
                )
                .when(can_move && can_move_up, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_project(up_id.clone(), move_up_before.clone());
                        this.project_menu_id = None;
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("project-down-{project_id}"),
                    "下移",
                    "chevron-down",
                    theme,
                    can_move && can_move_down,
                )
                .when(can_move && can_move_down, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_project(down_id.clone(), move_down_before.clone());
                        this.project_menu_id = None;
                    }))
                }),
            )
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("project-delete-{project_id}"),
                    if deleting {
                        "再次点击以移除"
                    } else {
                        "移除项目"
                    },
                    "close-dialog",
                    theme,
                    can_delete,
                )
                .when(can_delete, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm_or_delete(DeleteTarget::Project(delete_id.clone()), cx);
                    }))
                }),
            );
        menu
    }

    fn thread_context_menu(
        &self,
        thread: &ThreadSummary,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let thread_id = thread.thread_id.clone();
        let pinned = self
            .snapshot
            .pinned_threads
            .iter()
            .any(|candidate| candidate.thread_id == thread_id);
        let archived = self
            .snapshot
            .archived_threads
            .iter()
            .any(|candidate| candidate.thread_id == thread_id);
        let rename_id = thread_id.clone();
        let rename_title = thread.title.clone();
        let pin_id = thread_id.clone();
        let archive_id = thread_id.clone();
        let unassigned_id = thread_id.clone();
        let delete_id = thread_id.clone();
        let can_rename = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadRename);
        let can_pin = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadSectionMove)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionList)
            && self
                .snapshot
                .capabilities
                .supports(AgentCapability::ThreadSectionCreate);
        let can_archive = self.snapshot.capabilities.supports(if archived {
            AgentCapability::ThreadUnarchive
        } else {
            AgentCapability::ThreadArchive
        });
        let can_move = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadMetadataUpdate);
        let can_delete = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadDelete);
        let deleting =
            self.delete_confirmation.as_ref() == Some(&DeleteTarget::Thread(thread_id.clone()));
        let mut menu = self
            .menu_shell(format!("thread-context-{thread_id}"), 214.0, theme)
            .child(
                Self::menu_item(
                    format!("thread-rename-{thread_id}"),
                    "重命名",
                    "settings-edit",
                    theme,
                    can_rename,
                )
                .when(can_rename, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.start_rename(
                            RenameTarget::Thread(rename_id.clone()),
                            rename_title.clone(),
                            cx,
                        );
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("thread-context-pin-{thread_id}"),
                    if pinned { "取消置顶" } else { "置顶" },
                    "pin",
                    theme,
                    can_pin,
                )
                .when(can_pin, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store.set_thread_pinned(pin_id.clone(), !pinned);
                        this.thread_menu_id = None;
                    }))
                }),
            )
            .child(
                Self::menu_item(
                    format!("thread-context-archive-{thread_id}"),
                    if archived { "取消归档" } else { "归档" },
                    "archive",
                    theme,
                    can_archive,
                )
                .when(can_archive, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        if archived {
                            this.store.unarchive_thread(archive_id.clone());
                        } else {
                            this.store.archive_thread(archive_id.clone());
                        }
                        this.thread_menu_id = None;
                    }))
                }),
            )
            .child(Self::menu_separator(theme))
            .child(
                Self::menu_item(
                    format!("thread-unassign-{thread_id}"),
                    "移至“无项目”",
                    "folder",
                    theme,
                    can_move && thread.project_id.is_some(),
                )
                .when(can_move && thread.project_id.is_some(), |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_thread_to_project(unassigned_id.clone(), None);
                        this.thread_menu_id = None;
                    }))
                }),
            );
        for project in &self.snapshot.projects {
            let move_id = thread_id.clone();
            let project_id = project.project_id.clone();
            let enabled = can_move && thread.project_id.as_deref() != Some(project_id.as_str());
            menu = menu.child(
                Self::menu_item(
                    format!("thread-move-{thread_id}-{project_id}"),
                    format!("移至“{}”", project.name),
                    "folder",
                    theme,
                    enabled,
                )
                .when(enabled, |row| {
                    row.on_click(cx.listener(move |this, _, _, _| {
                        this.store
                            .move_thread_to_project(move_id.clone(), Some(project_id.clone()));
                        this.thread_menu_id = None;
                    }))
                }),
            );
        }
        menu.child(Self::menu_separator(theme)).child(
            Self::menu_item(
                format!("thread-delete-{thread_id}"),
                if deleting {
                    "再次点击以删除"
                } else {
                    "删除聊天"
                },
                "close-dialog",
                theme,
                can_delete,
            )
            .when(can_delete, |row| {
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm_or_delete(DeleteTarget::Thread(delete_id.clone()), cx);
                }))
            }),
        )
    }

    fn projects_options_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let can_list_threads = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadList);
        self.menu_shell("projects-options-menu", 176.0, theme)
            .child(
                Self::menu_item("projects-refresh", "刷新", "settings-refresh", theme, true)
                    .on_click(cx.listener(|this, _, _, _| {
                        this.store.refresh_all();
                        this.projects_section_menu_open = false;
                    })),
            )
            .child(
                Self::menu_item(
                    "projects-archived",
                    "已归档聊天",
                    "archive",
                    theme,
                    can_list_threads,
                )
                .when(can_list_threads, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        this.archived_open = true;
                        this.activity_open = false;
                        this.projects_section_menu_open = false;
                        cx.notify();
                    }))
                }),
            )
    }

    fn search_panel(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let mut results = div()
            .id("search-results")
            .min_h(px(0.0))
            .flex_1()
            .overflow_y_scroll()
            .px(px(5.0))
            .pb(px(5.0));
        if let Some(error) = self.snapshot.error.clone() {
            results = results.child(self.retryable_error_row(
                "search-error",
                error,
                Some(self.snapshot.search_query.clone()),
                theme,
                cx,
            ));
        } else if self.snapshot.loading.search {
            results = results.child(self.status_row("search-loading", "正在搜索…", theme));
        } else if self.snapshot.search_query.trim().is_empty() {
            results = results.child(self.status_row("search-hint", "输入关键词搜索聊天", theme));
        } else if self.snapshot.search_results.is_empty() {
            results = results.child(self.status_row("search-empty", "未找到聊天", theme));
        } else {
            for (index, result) in self.snapshot.search_results.iter().enumerate() {
                let thread_id = result.thread.thread_id.clone();
                results = results.child(
                    div()
                        .id(format!("search-result-{thread_id}"))
                        .h(px(SEARCH_RESULT_HEIGHT))
                        .px(px(8.0))
                        .rounded(px(ROW_RADIUS))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .text_size(px(14.0))
                        .text_color(theme.sidebar_text)
                        .when(index == 0, |row| row.bg(theme.sidebar_hover))
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .child(icon("search", theme.sidebar_icon_muted.into()).size(px(16.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(result.thread.title.clone()),
                        )
                        .child(
                            div()
                                .max_w(px(210.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(13.0))
                                .text_color(theme.sidebar_text_muted)
                                .child(result.snippet.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_thread(thread_id.clone(), cx);
                        })),
                );
            }
        }
        self.menu_shell("search-panel", SEARCH_PANEL_WIDTH, theme)
            .h(px(SEARCH_PANEL_HEIGHT))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(SEARCH_INPUT_HEIGHT))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(icon("search", theme.sidebar_icon_muted.into()).size(px(16.0)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(self.search_input.clone()),
                    ),
            )
            .child(Self::menu_separator(theme))
            .child(results)
    }

    fn header(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let can_search = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadSearch);
        let can_list_threads = self
            .snapshot
            .capabilities
            .supports(AgentCapability::ThreadList);
        let new_project = self
            .selected_project_id
            .as_deref()
            .and_then(|project_id| {
                self.snapshot
                    .projects
                    .iter()
                    .find(|project| project.project_id == project_id)
            })
            .cloned();
        let new_conversation = div()
            .id("sidebar-new-conversation")
            .h(px(ROW_HEIGHT))
            .w_full()
            .px(px(8.0))
            .rounded(px(10.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(icon("new-chat", theme.sidebar_text.into()))
            .child(div().flex_1().child("新对话"))
            .child(icon("quick-chat", theme.sidebar_text_muted.into()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.new_conversation(new_project.as_ref(), cx);
            }));
        let search = Self::nav_icon_button("sidebar-search", "search", theme)
            .when(!can_search, |button| button.opacity(0.4).cursor_default())
            .when(can_search, |button| {
                button.on_click(cx.listener(|this, _, _, cx| {
                    this.close_transient_menus(cx);
                    this.search_open = true;
                    this.search_focus_pending = true;
                    cx.notify();
                }))
            });
        let activity = Self::nav_icon_button("sidebar-activity", "activity", theme)
            .when(!can_list_threads, |button| {
                button.opacity(0.4).cursor_default()
            })
            .when(can_list_threads, |button| {
                button.on_click(cx.listener(|this, _, _, cx| {
                    this.activity_open = true;
                    this.archived_open = false;
                    cx.notify();
                }))
            });
        div()
            .flex_none()
            .child(
                div()
                    .h(px(38.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .relative()
                            .top(px(-2.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(17.0))
                            .font(crate::typography::brand_font(cx))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.sidebar_title_text)
                            .child("Codex")
                            .child(chevron(theme.sidebar_icon_muted.into()).size(px(14.0))),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(search)
                            .child(activity),
                    ),
            )
            // 保留修改前已由 CDP 校准的标题栏与新对话入口；未接入的入口
            // 只保留既有视觉，不制造对应业务数据或协议行为。
            .child(
                div()
                    .relative()
                    .top(px(1.0))
                    .px(px(8.0))
                    .h(px(31.0))
                    .child(new_conversation),
            )
    }

    fn subpage_header(&self, title: &'static str, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(46.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                Self::nav_icon_button("sidebar-back", "back", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.activity_open = false;
                        this.archived_open = false;
                        cx.notify();
                    },
                )),
            )
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.sidebar_title_text)
                    .child(title),
            )
    }

    fn profile_button(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("sidebar-profile")
            .h(px(38.0))
            .mx(px(8.0))
            .mb(px(5.0))
            .px(px(8.0))
            .rounded(px(ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(14.0))
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(
                div()
                    .size(px(22.0))
                    .rounded_full()
                    .bg(theme.control)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(8.0))
                    .child("RI"),
            )
            .child(div().flex_1().child("rita"))
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.profile_menu_open = !this.profile_menu_open;
                cx.notify();
            }))
    }

    fn profile_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        self.menu_shell("profile-menu", SIDEBAR_WIDTH - 16.0, theme)
            .child(
                Self::menu_item("profile-settings", "设置", "profile-settings", theme, true)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.profile_menu_open = false;
                        cx.emit(OpenSettings);
                        cx.notify();
                    })),
            )
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.scroll_to_bottom) {
            self.scroll.scroll_to_bottom();
        }
        if std::mem::take(&mut self.search_focus_pending) {
            self.search_input.focus_handle(cx).focus(window, cx);
        }
        if std::mem::take(&mut self.rename_focus_pending) {
            self.rename_input.focus_handle(cx).focus(window, cx);
        }
        let theme = Theme::for_mode(self.mode);
        let mut sidebar = div()
            .id("sidebar")
            .relative()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .pt(px(SIDEBAR_TITLEBAR_SAFE_TOP))
            .flex()
            .flex_col()
            .font_family(".SystemUIFont")
            // The app shell owns the sidebar material so it is composited only
            // once over the native blur. Painting the same translucent tint in
            // this content view would compound its alpha and make the settled
            // sidebar appear opaque in both light and dark themes.
            .text_color(theme.sidebar_text);
        if self.activity_open {
            sidebar = sidebar
                .child(self.subpage_header("活动", theme, cx))
                .child(self.activity_content(theme, window, cx));
        } else if self.archived_open {
            sidebar = sidebar
                .child(self.subpage_header("已归档", theme, cx))
                .child(self.archived_content(theme, window, cx));
        } else {
            sidebar = sidebar
                .child(self.header(theme, cx))
                .child(self.workspace_content(theme, window, cx))
                .child(self.profile_button(theme, cx));
        }

        if let Some(project_id) = self.project_menu_id.as_deref()
            && let Some(project) = self
                .snapshot
                .projects
                .iter()
                .find(|project| project.project_id == project_id)
        {
            let left = self.menu_origin.0.clamp(8.0, SIDEBAR_WIDTH - 198.0);
            let top = self.menu_origin.1.max(42.0);
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.project_context_menu(project, theme, cx)),
            ));
        }
        if let Some(thread_id) = self.thread_menu_id.as_deref()
            && let Some(thread) = self.snapshot.thread(thread_id)
        {
            let left = self.menu_origin.0.clamp(8.0, SIDEBAR_WIDTH - 222.0);
            let top = self.menu_origin.1.max(42.0);
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .child(self.thread_context_menu(thread, theme, cx)),
            ));
        }
        if self.projects_section_menu_open {
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(56.0))
                    .top(px(220.0))
                    .child(self.projects_options_menu(theme, cx)),
            ));
        }
        if self.profile_menu_open {
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(8.0))
                    .bottom(px(43.0))
                    .child(self.profile_menu(theme, cx)),
            ));
        }
        if self.search_open {
            sidebar = sidebar.child(deferred(
                div()
                    .absolute()
                    .left(px(SIDEBAR_WIDTH + 32.0))
                    .top(px(64.0))
                    .child(self.search_panel(theme, cx)),
            ));
        }
        sidebar
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    use async_channel::{Receiver, Sender};
    use gpui::{Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, px, size};

    use super::{ROW_HEIGHT, ROW_RADIUS, SIDEBAR_TITLEBAR_SAFE_TOP, SIDEBAR_WIDTH, SidebarView};
    use crate::{
        agent::{
            AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentEvent,
            AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile, AgentRequest, AgentRun,
            AgentThreadSettings, Page, PageRequest, Project, ThreadActivity, ThreadListRequest,
            ThreadSummary, WorkspaceResult,
        },
        theme::ThemeMode,
        workspace::WorkspaceStore,
    };

    fn response<T: Send + 'static>(value: T) -> Receiver<T> {
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.send_blocking(value);
        receiver
    }

    struct SidebarBackend {
        _events: Sender<AgentConnectionEvent>,
        event_receiver: Receiver<AgentConnectionEvent>,
    }

    impl SidebarBackend {
        fn new() -> Arc<Self> {
            let (events, event_receiver) = async_channel::unbounded();
            Arc::new(Self {
                _events: events,
                event_receiver,
            })
        }
    }

    impl AgentBackend for SidebarBackend {
        fn capabilities(&self) -> AgentCapabilities {
            AgentCapabilities::new([
                AgentCapability::ProjectList,
                AgentCapability::ThreadList,
                AgentCapability::ThreadSectionList,
            ])
        }

        fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
            self.event_receiver.clone()
        }

        fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
            response(Err("not used".to_owned()))
        }

        fn load_permission_profiles(
            &self,
            _cwd: PathBuf,
        ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
            response(Err("not used".to_owned()))
        }

        fn update_thread_permissions(
            &self,
            _thread_id: String,
            _cwd: PathBuf,
            _mode: AgentPermissionMode,
        ) -> Receiver<Result<AgentThreadSettings, String>> {
            response(Err("not used".to_owned()))
        }

        fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
            response(Ok(Page::single(vec![Project {
                project_id: "project-stable-id".to_owned(),
                name: "Stable project".to_owned(),
                roots: vec![PathBuf::from("/tmp/stable-project")],
                created_at: 1,
                updated_at: 2,
                recency_at: Some(3),
                position: 0,
            }])))
        }

        fn list_threads(
            &self,
            request: ThreadListRequest,
        ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
            let threads = (!request.archived).then(|| ThreadSummary {
                thread_id: "thread-stable-id".to_owned(),
                title: "Stable thread".to_owned(),
                preview: "Stable thread".to_owned(),
                cwd: PathBuf::from("/tmp/stable-project"),
                project_id: Some("project-stable-id".to_owned()),
                section: None,
                created_at: 1,
                updated_at: 2,
                recency_at: Some(3),
                activity: ThreadActivity::Idle,
            });
            response(Ok(Page::single(threads.into_iter().collect())))
        }

        fn list_thread_sections(
            &self,
            _page: PageRequest,
        ) -> Receiver<WorkspaceResult<Page<crate::agent::ThreadSection>>> {
            response(Ok(Page::single(Vec::new())))
        }

        fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
            let (sender, receiver) = async_channel::unbounded::<AgentEvent>();
            drop(sender);
            AgentRun::new(receiver, None)
        }
    }

    #[test]
    fn store_backed_sidebar_selects_the_rendered_thread_by_stable_id() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let preferences = std::env::temp_dir()
            .join(format!(
                "gpui-sidebar-ui-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
            .join("preferences.json");
        let store =
            WorkspaceStore::with_preferences_path(SidebarBackend::new(), preferences.clone());
        store.refresh_all();
        let deadline = Instant::now() + Duration::from_secs(3);
        while store.snapshot().loading.recent {
            assert!(Instant::now() < deadline, "sidebar fixture did not load");
            std::thread::sleep(Duration::from_millis(5));
        }

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| SidebarView::new(ThemeMode::Dark, false, store, cx),
        );
        window.draw();
        // 46 px titlebar safe area + 38 px brand + 31 px new-chat row,
        // followed by the restored 4-row navigation block, project heading,
        // project row, and the first server-backed thread row.
        window.simulate_click(point(px(80.0), px(340.0)), MouseButton::Left);
        assert_eq!(
            window.read(|sidebar, _| sidebar.selected_thread_id.clone()),
            Some("thread-stable-id".to_owned())
        );
        assert_eq!(SIDEBAR_WIDTH, 240.0);
        assert_eq!(SIDEBAR_TITLEBAR_SAFE_TOP, 46.0);
        assert_eq!(ROW_HEIGHT, 30.0);
        assert_eq!(ROW_RADIUS, 12.5);
        if let Some(parent) = preferences.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
