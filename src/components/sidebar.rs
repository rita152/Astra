use std::{
    path::Path,
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, App, Bounds, ContentMask, Context, Div, FocusHandle, Hsla,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Render, ScrollDelta, ScrollHandle, ScrollWheelEvent, ShapedLine, Task, TextAlign, TextRun,
    Transformation, Window, canvas, deferred, div, point, prelude::*, px, quad, radians, size,
};

use crate::{
    components::icons::{chevron, icon},
    theme::{Theme, ThemeMode},
};

pub struct OpenSettings;
pub struct OpenProjectCreation;

impl gpui::EventEmitter<OpenSettings> for SidebarView {}
impl gpui::EventEmitter<OpenProjectCreation> for SidebarView {}

const PROJECTS: &[(&str, &[&str])] = &[
    ("oh-my-pi", &["分析项目中的 Web 工具"]),
    ("飞行器设计大赛", &["编写智能集群项目报告初稿"]),
    ("deepseek-harness", &["列出全部内置工具"]),
    ("chatgpt", &["尝试连接 ChatGPT CDP"]),
    (
        "coda",
        &[
            "统一外挂能力包使用体验",
            "对齐本地 Plugin 使用回路",
            "Implement Agent Plugins client",
            "实现 Codex hooks 系统",
            "实现 web_search 和 fetch 功能",
            "展开显示",
        ],
    ),
    ("pi", &[]),
    (
        "codex",
        &[
            "同步上游最新提交",
            "列出 Codex 内置工具",
            "确认 multiagent_v2 模型自定义支持",
            "统计项目业务代码",
            "确认核心 agentLoop 架构",
            "展开显示",
        ],
    ),
    (
        "LAG",
        &[
            "实现 M1C 多战术空战",
            "实现空战 RL M-1 至 M1B",
            "调研空战 RL 创新并制定实验方案",
            "搭建多无人机空战 RL 环境",
            "评估多无人机空战 RL 预训练方案",
        ],
    ),
    ("语音输入法", &[]),
    ("LAG_创新", &[]),
];

const RECENTS: &[&str] = &[
    "DAgger与行为克隆对比",
    "请你尝试使用SSH来连接local_server/server409这两个服务器，我现在连接出现了问题，请你分辨问题出在哪里",
    "统计 coda 目录存储占用",
    "查找CDP打开ChatGPT方法",
    "了解 Codex 长时间实验处理方式",
    "查找长程实验监控机制",
    "查找 ChatGPT App 的 CDP 用法",
    "评估agent压缩效果",
    "查找 OpenAI Agents Extension 协议",
    "查找 Codex reset 时段",
];

const SCROLLBAR_IDLE_DELAY: Duration = Duration::from_millis(650);
const MOUSE_WHEEL_SMOOTHING_TIME: Duration = Duration::from_millis(45);
const MOUSE_WHEEL_SETTLE_DISTANCE: f32 = 0.35;
const MOUSE_WHEEL_MAX_PENDING_LINES: f32 = 5.0;
const SIDEBAR_DEFAULT_WIDTH: f32 = 256.125;
const SIDEBAR_MIN_WIDTH: f32 = 240.0;
const SIDEBAR_MAX_WIDTH: f32 = 520.0;
// Match the platform fonts reported by the reference app: body copy resolves
// to SF/PingFang Regular, section labels to PingFang Medium, and the product
// title to OpenAI Sans at 600. GPUI uses the system face for the title, so its
// color is calibrated separately from body copy in the theme.
const SIDEBAR_BODY_FONT_WEIGHT: gpui::FontWeight = gpui::FontWeight::NORMAL;
const SIDEBAR_SECTION_FONT_WEIGHT: gpui::FontWeight = gpui::FontWeight::MEDIUM;
const SIDEBAR_TITLE_FONT_WEIGHT: gpui::FontWeight = gpui::FontWeight::SEMIBOLD;
const MAIN_MIN_WIDTH: f32 = 320.0;
const MARQUEE_HOVER_DELAY: Duration = Duration::from_millis(350);
const MARQUEE_SPEED: f32 = 28.0;
const PROJECT_THREAD_TITLE_INSETS: f32 = 120.0;
const RECENT_THREAD_TITLE_INSETS: f32 = 96.0;
const THREAD_ACTION_RAIL_INSETS: f32 = 51.0;
const ACTIVITY_NAV_BLOCK_HEIGHT: f32 = 140.0;
const ACTIVITY_SECTION_GAP: f32 = 16.0;
const ACTIVITY_HEADING_HEIGHT: f32 = 30.0;
const ACTIVITY_ROW_HEIGHT: f32 = 54.0;
const ACTIVITY_ROW_GAP: f32 = 1.0;
// The reference app applies `pe-14` to the title line while its 52 px action
// rail is visible. The extra 4 px keeps the scrolling title clear of the rail.
const ACTIVITY_TITLE_BASE_INSETS: f32 = 44.0;
const ACTIVITY_ACTION_TITLE_PADDING: f32 = 56.0;
const ACTIVITY_TITLE_FADE_IN: f32 = 8.0;
const ACTIVITY_TITLE_FADE_OUT: f32 = 16.0;

fn activity_section_height(row_count: usize) -> f32 {
    ACTIVITY_HEADING_HEIGHT
        + row_count as f32 * ACTIVITY_ROW_HEIGHT
        + row_count.saturating_sub(1) as f32 * ACTIVITY_ROW_GAP
}

fn activity_title_viewport_width(sidebar_width: f32, show_actions: bool) -> f32 {
    (sidebar_width
        - ACTIVITY_TITLE_BASE_INSETS
        - if show_actions {
            ACTIVITY_ACTION_TITLE_PADDING
        } else {
            0.0
        })
    .max(0.0)
}

fn sidebar_thread_title_viewport_width(sidebar_width: f32, flat: bool, show_actions: bool) -> f32 {
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
    (sidebar_width - insets).max(0.0)
}

fn faded_sidebar_text_color(color: Hsla, fade: f32) -> Hsla {
    color.alpha(color.a * fade)
}

fn activity_title_canvas(
    title: &'static str,
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
                    title.to_owned().into(),
                    px(14.0),
                    &[TextRun {
                        len: title.len(),
                        font: font.clone(),
                        // `Hsla::alpha` replaces alpha rather than multiplying
                        // it. Preserve the sidebar foreground opacity so canvas
                        // titles do not become fully opaque and look bolder than
                        // adjacent DOM-like text rows.
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
                (0..ACTIVITY_TITLE_FADE_IN as usize)
                    .map(|index| shape((index as f32 + 0.5) / ACTIVITY_TITLE_FADE_IN))
                    .collect::<Vec<_>>()
            });
            let right = overflows.then(|| {
                (0..ACTIVITY_TITLE_FADE_OUT as usize)
                    .map(|index| shape(1.0 - (index as f32 + 0.5) / ACTIVITY_TITLE_FADE_OUT))
                    .collect::<Vec<_>>()
            });
            (opaque, left, right)
        },
        move |bounds, (opaque, left, right): (ShapedLine, _, _), window, cx| {
            let origin = point(
                bounds.origin.x + px(ACTIVITY_TITLE_FADE_IN - scroll_offset),
                bounds.origin.y,
            );
            let paint =
                |line: &ShapedLine, mask: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                        line.paint(origin, px(20.0), TextAlign::Left, None, window, cx)
                            .expect("activity title glyphs should paint")
                    });
                };

            if let (Some(left), Some(right)) = (left.as_ref(), right.as_ref()) {
                let center_left = bounds.origin.x + px(ACTIVITY_TITLE_FADE_IN);
                let center_right = bounds.right() - px(ACTIVITY_TITLE_FADE_OUT);
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
                    let x = bounds.right() - px(ACTIVITY_TITLE_FADE_OUT) + px(index as f32);
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
    .ml(px(-ACTIVITY_TITLE_FADE_IN))
    .flex_1()
}
// profile-menu HTML: --text-sm: 13px and
// --text-sm--line-height: calc(1.25 / .875) = 10 / 7.
const PROFILE_MENU_TEXT_SIZE: f32 = 13.0;
const PROFILE_MENU_LINE_HEIGHT: f32 = PROFILE_MENU_TEXT_SIZE * 10.0 / 7.0;
const PROFILE_MENU_ROW_HEIGHT: f32 = PROFILE_MENU_LINE_HEIGHT + 10.0;
const PROFILE_TRIGGER_TEXT_SIZE: f32 = 14.0;
const PROFILE_TRIGGER_LINE_HEIGHT: f32 = 21.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SectionHeaderIcon {
    Chevron,
    Menu,
    Add,
    NewChat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectsLayout {
    Grouped,
    Flat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectsSort {
    Priority,
    Recent,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivityThread {
    Project(usize, usize),
    Recent(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MouseWheelScrollPlan {
    immediate_offset: f32,
    target_offset: f32,
    reversed: bool,
}

fn mouse_wheel_scroll_plan(
    current: f32,
    queued_target: Option<f32>,
    delta: f32,
    max_offset: f32,
    line_height: f32,
) -> MouseWheelScrollPlan {
    let max_offset = max_offset.max(0.0);
    let queued_target = queued_target.unwrap_or(current).clamp(-max_offset, 0.0);
    let pending = queued_target - current;
    let reversed = pending.abs() > f32::EPSILON
        && delta.abs() > f32::EPSILON
        && pending.signum() != delta.signum();
    let base = if reversed { current } else { queued_target };
    let target_offset = (base + delta).clamp(-max_offset, 0.0);

    // Preserve every wheel tick, but never leave more than a short tail for
    // the animation to catch up. Excess distance is applied immediately when
    // a fast wheel burst would otherwise build visible input latency.
    let max_pending = line_height.abs().max(1.0) * MOUSE_WHEEL_MAX_PENDING_LINES;
    let pending_from_current = target_offset - current;
    let immediate_offset = if pending_from_current.abs() > max_pending {
        target_offset - pending_from_current.signum() * max_pending
    } else {
        current
    }
    .clamp(-max_offset, 0.0);

    MouseWheelScrollPlan {
        immediate_offset,
        target_offset,
        reversed,
    }
}

fn marquee_offset(scroll_distance: f32, elapsed: Duration, reduce_motion: bool) -> f32 {
    if reduce_motion || scroll_distance <= 0.0 || elapsed <= MARQUEE_HOVER_DELAY {
        return 0.0;
    }

    let travel_time = scroll_distance / MARQUEE_SPEED;
    let travel_elapsed = (elapsed - MARQUEE_HOVER_DELAY).as_secs_f32();
    let progress = (travel_elapsed / travel_time).clamp(0.0, 1.0);
    scroll_distance * marquee_ease(progress)
}

fn marquee_duration(scroll_distance: f32) -> Duration {
    MARQUEE_HOVER_DELAY + Duration::from_secs_f32(scroll_distance.max(0.0) / MARQUEE_SPEED)
}

fn marquee_ease(progress: f32) -> f32 {
    // The reference expands cubic-bezier(.5, .6, .7, 1) into a sampled CSS
    // `linear()` function. Solve the same curve here because GPUI animates the
    // translation frame-by-frame rather than through CSS.
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

fn smoothed_scroll_position(current: f32, target: f32, elapsed: Duration) -> f32 {
    // A first-order ease-out is interruptible and cannot overshoot. Let a
    // delayed frame catch up using real elapsed time instead of stretching the
    // animation after a hitch; the cap only avoids turning a long suspension
    // into an unexplained teleport.
    let elapsed = elapsed.as_secs_f32().clamp(1.0 / 240.0, 0.1);
    let smoothing_time = MOUSE_WHEEL_SMOOTHING_TIME.as_secs_f32();
    let progress = 1.0 - (-elapsed / smoothing_time).exp();
    current + (target - current) * progress
}

fn sidebar_width_limit(viewport_width: f32) -> f32 {
    SIDEBAR_MAX_WIDTH
        .min(viewport_width - MAIN_MIN_WIDTH)
        .max(SIDEBAR_MIN_WIDTH)
}

pub struct SidebarView {
    mode: ThemeMode,
    scroll: ScrollHandle,
    activity_scroll: ScrollHandle,
    activity_scroll_to: Option<f32>,
    scroll_to_bottom: bool,
    scrollbar_visible: bool,
    scrollbar_hide_deadline: Option<Instant>,
    scrollbar_hide_loop_running: bool,
    scrollbar_hide_task: Option<Task<()>>,
    mouse_wheel_target: Option<f32>,
    mouse_wheel_animation_running: bool,
    mouse_wheel_last_frame: Option<Instant>,
    projects_collapsed: bool,
    project_collapsed: Vec<bool>,
    project_show_all: Vec<bool>,
    created_projects: Vec<String>,
    selected_created_project: Option<usize>,
    selected_project: usize,
    selected_thread: Option<(usize, usize)>,
    selected_recent: Option<usize>,
    pinned_threads: Vec<Vec<bool>>,
    archived_threads: Vec<Vec<bool>>,
    pinned_collapsed: bool,
    pinned_heading_hovered: bool,
    pinned_menu_button_hovered: bool,
    pinned_menu_open: bool,
    pinned_menu_focus: Option<FocusHandle>,
    pinned_menu_focused_item: Option<usize>,
    pinned_sort: ProjectsSort,
    open_project_menu: Option<usize>,
    project_menu_focus: Option<FocusHandle>,
    project_menu_focused_item: Option<usize>,
    pinned_projects: Vec<bool>,
    pinned_created_projects: Vec<bool>,
    removed_projects: Vec<bool>,
    projects_section_menu_open: bool,
    projects_menu_focus: Option<FocusHandle>,
    projects_menu_focused_item: Option<usize>,
    projects_layout: ProjectsLayout,
    projects_sort: ProjectsSort,
    projects_heading_hovered: bool,
    project_creation_trigger_open: bool,
    hovered_section_icon: Option<SectionHeaderIcon>,
    hovered_thread: Option<(usize, usize)>,
    recents_collapsed: bool,
    recents_heading_hovered: bool,
    hovered_recents_icon: Option<SectionHeaderIcon>,
    hovered_recent_thread: Option<usize>,
    marquee_started_at: Option<Instant>,
    marquee_animation_ends_at: Option<Instant>,
    marquee_animation_running: bool,
    pinned_recents: Vec<bool>,
    archived_recents: Vec<bool>,
    sidebar_width: f32,
    sidebar_resize_dragging: bool,
    sidebar_resize_hovered: bool,
    sidebar_resize_pointer_offset: f32,
    profile_menu_open: bool,
    activity_open: bool,
    activity_button_hovered: bool,
    activity_button_pressed: bool,
    activity_running_thread: Option<ActivityThread>,
}

impl SidebarView {
    pub fn new(mode: ThemeMode, scroll_to_bottom: bool) -> Self {
        let row_state = || {
            PROJECTS
                .iter()
                .map(|(_, rows)| vec![false; rows.len()])
                .collect()
        };
        Self {
            mode,
            scroll: ScrollHandle::new(),
            activity_scroll: ScrollHandle::new(),
            activity_scroll_to: None,
            scroll_to_bottom,
            scrollbar_visible: false,
            scrollbar_hide_deadline: None,
            scrollbar_hide_loop_running: false,
            scrollbar_hide_task: None,
            mouse_wheel_target: None,
            mouse_wheel_animation_running: false,
            mouse_wheel_last_frame: None,
            projects_collapsed: false,
            project_collapsed: PROJECTS
                .iter()
                .map(|(name, _)| matches!(*name, "pi" | "语音输入法" | "LAG_创新"))
                .collect(),
            project_show_all: vec![false; PROJECTS.len()],
            created_projects: Vec::new(),
            selected_created_project: None,
            selected_project: 4,
            selected_thread: None,
            selected_recent: None,
            pinned_threads: row_state(),
            archived_threads: row_state(),
            pinned_collapsed: false,
            pinned_heading_hovered: false,
            pinned_menu_button_hovered: false,
            pinned_menu_open: false,
            pinned_menu_focus: None,
            pinned_menu_focused_item: None,
            // The reference app initially uses manual ordering for pinned chats.
            pinned_sort: ProjectsSort::Manual,
            open_project_menu: None,
            project_menu_focus: None,
            project_menu_focused_item: None,
            pinned_projects: vec![false; PROJECTS.len()],
            pinned_created_projects: Vec::new(),
            removed_projects: vec![false; PROJECTS.len()],
            projects_section_menu_open: false,
            projects_menu_focus: None,
            projects_menu_focused_item: None,
            projects_layout: ProjectsLayout::Grouped,
            projects_sort: ProjectsSort::Priority,
            projects_heading_hovered: false,
            project_creation_trigger_open: false,
            hovered_section_icon: None,
            hovered_thread: None,
            recents_collapsed: false,
            recents_heading_hovered: false,
            hovered_recents_icon: None,
            hovered_recent_thread: None,
            marquee_started_at: None,
            marquee_animation_ends_at: None,
            marquee_animation_running: false,
            pinned_recents: vec![false; RECENTS.len()],
            archived_recents: vec![false; RECENTS.len()],
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
            sidebar_resize_dragging: false,
            sidebar_resize_hovered: false,
            sidebar_resize_pointer_offset: 0.0,
            profile_menu_open: false,
            activity_open: false,
            activity_button_hovered: false,
            activity_button_pressed: false,
            activity_running_thread: None,
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    pub fn width(&self) -> f32 {
        self.sidebar_width
    }

    pub fn set_profile_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.profile_menu_open = open;
        cx.notify();
    }

    pub fn set_activity_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.activity_open = open;
        self.cancel_mouse_wheel_animation();
        self.hovered_thread = None;
        self.hovered_recent_thread = None;
        self.stop_marquee();
        self.profile_menu_open = false;
        if open {
            self.activity_running_thread = self
                .selected_thread
                .map(|(project, row)| ActivityThread::Project(project, row))
                .or_else(|| self.selected_recent.map(ActivityThread::Recent))
                .or_else(|| self.fallback_activity_thread());
            self.activity_scroll.set_offset(point(px(0.0), px(0.0)));
        }
        cx.notify();
    }

    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.activity_scroll_to = Some(offset.max(0.0));
        cx.notify();
    }

    pub fn set_activity_hovered_recent_for_capture(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if index < RECENTS.len() {
            self.hovered_thread = None;
            self.hovered_recent_thread = Some(index);
            cx.notify();
        }
    }

    pub fn set_projects_section_menu_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.projects_section_menu_open = open;
        self.projects_menu_focused_item = None;
        self.pinned_menu_open = false;
        self.pinned_menu_focused_item = None;
        self.open_project_menu = None;
        self.profile_menu_open = false;
        if open && let Some(focus) = self.projects_menu_focus.as_ref() {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    pub fn open_projects_section_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.projects_section_menu_open = true;
        self.projects_menu_focused_item = None;
        self.pinned_menu_open = false;
        self.pinned_menu_focused_item = None;
        self.open_project_menu = None;
        self.profile_menu_open = false;
        cx.notify();
    }

    pub fn set_project_creation_trigger_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.project_creation_trigger_open = open;
        if open {
            self.open_project_menu = None;
            self.pinned_menu_open = false;
            self.pinned_menu_focused_item = None;
            self.projects_section_menu_open = false;
            self.profile_menu_open = false;
        }
        cx.notify();
    }

    pub fn add_local_project(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        if let Some(index) = self
            .created_projects
            .iter()
            .position(|project| project == name)
        {
            self.selected_created_project = Some(index);
        } else if let Some((index, _)) = PROJECTS
            .iter()
            .enumerate()
            .find(|(_, (project, _))| *project == name)
        {
            self.selected_project = index;
            self.selected_created_project = None;
        } else {
            self.created_projects.push(name.to_owned());
            self.pinned_created_projects.push(false);
            self.selected_created_project = Some(self.created_projects.len() - 1);
        }
        self.selected_thread = None;
        self.selected_recent = None;
        cx.notify();
    }

    pub fn close_transient_menus(&mut self, cx: &mut Context<Self>) {
        let changed = self.pinned_menu_open
            || self.projects_section_menu_open
            || self.open_project_menu.is_some()
            || self.profile_menu_open;
        self.pinned_menu_open = false;
        self.pinned_menu_focused_item = None;
        self.projects_section_menu_open = false;
        self.projects_menu_focused_item = None;
        self.open_project_menu = None;
        self.project_menu_focused_item = None;
        self.profile_menu_open = false;
        if changed {
            cx.notify();
        }
    }

    #[cfg(test)]
    pub fn projects_section_menu_is_open(&self) -> bool {
        self.projects_section_menu_open
    }

    #[cfg(test)]
    pub fn project_menu_is_open(&self) -> bool {
        self.open_project_menu.is_some()
    }

    #[cfg(test)]
    pub fn pinned_menu_is_open(&self) -> bool {
        self.pinned_menu_open
    }

    fn reveal_scrollbar(&mut self, cx: &mut Context<Self>) {
        let was_visible = self.scrollbar_visible;
        self.scrollbar_visible = true;
        self.scrollbar_hide_deadline = Some(cx.background_executor().now() + SCROLLBAR_IDLE_DELAY);

        // A single trailing-edge timer serves the entire gesture. In
        // particular, a 120 Hz trackpad stream no longer allocates and cancels
        // one task for every pixel event.
        if !self.scrollbar_hide_loop_running {
            self.scrollbar_hide_loop_running = true;
            self.scrollbar_hide_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    let deadline = match this.read_with(cx, |this, _| this.scrollbar_hide_deadline)
                    {
                        Ok(Some(deadline)) => deadline,
                        _ => return,
                    };
                    let now = cx.background_executor().now();
                    if deadline > now {
                        cx.background_executor()
                            .timer(deadline.saturating_duration_since(now))
                            .await;
                        continue;
                    }

                    let finished = this
                        .update(cx, |this, cx| {
                            let now = cx.background_executor().now();
                            if this
                                .scrollbar_hide_deadline
                                .is_some_and(|deadline| deadline > now)
                            {
                                return false;
                            }

                            this.scrollbar_visible = false;
                            this.scrollbar_hide_deadline = None;
                            this.scrollbar_hide_loop_running = false;
                            cx.notify();
                            true
                        })
                        .unwrap_or(true);
                    if finished {
                        return;
                    }
                }
            }));
        }

        if !was_visible {
            cx.notify();
        }
    }

    fn cancel_mouse_wheel_animation(&mut self) {
        self.mouse_wheel_target = None;
        self.mouse_wheel_animation_running = false;
        self.mouse_wheel_last_frame = None;
    }

    fn active_scroll(&self) -> &ScrollHandle {
        if self.activity_open {
            &self.activity_scroll
        } else {
            &self.scroll
        }
    }

    fn set_scroll_y(&self, y: f32) {
        let scroll = self.active_scroll();
        let offset = scroll.offset();
        scroll.set_offset(point(offset.x, px(y)));
    }

    fn handle_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.delta {
            ScrollDelta::Pixels(delta) => {
                // Precise devices (trackpads and precision mice) already carry
                // the OS gesture curve and momentum. Cancel any coarse-wheel
                // tail, then let the parent GPUI scroll container consume the
                // exact pixel delta and its touch phase natively.
                self.cancel_mouse_wheel_animation();
                if delta.y != px(0.0) {
                    self.reveal_scrollbar(cx);
                    if self.activity_open {
                        cx.notify();
                    }
                }
            }
            ScrollDelta::Lines(delta) => {
                if delta.y == 0.0 {
                    return;
                }

                // This listener lives on the content child, so it runs before
                // the parent's built-in scroller during bubble dispatch. Lines
                // are exclusively owned here; Pixels keep propagating above.
                cx.stop_propagation();

                let current = f32::from(self.active_scroll().offset().y);
                let max_offset = f32::from(self.active_scroll().max_offset().y).max(0.0);
                if max_offset <= 0.0 {
                    self.cancel_mouse_wheel_animation();
                    return;
                }

                let line_height = f32::from(window.line_height()).max(1.0);
                let pixel_delta = line_height * delta.y;
                self.reveal_scrollbar(cx);

                if cx.reduce_motion() {
                    self.cancel_mouse_wheel_animation();
                    let target = (current + pixel_delta).clamp(-max_offset, 0.0);
                    if target != current {
                        self.set_scroll_y(target);
                        cx.notify();
                    }
                    return;
                }

                let plan = mouse_wheel_scroll_plan(
                    current,
                    self.mouse_wheel_target,
                    pixel_delta,
                    max_offset,
                    line_height,
                );

                if plan.immediate_offset != current {
                    self.set_scroll_y(plan.immediate_offset);
                    cx.notify();
                }

                if (plan.target_offset - plan.immediate_offset).abs() <= MOUSE_WHEEL_SETTLE_DISTANCE
                {
                    self.set_scroll_y(plan.target_offset);
                    self.cancel_mouse_wheel_animation();
                    cx.notify();
                    return;
                }

                self.mouse_wheel_target = Some(plan.target_offset);
                if plan.reversed {
                    self.mouse_wheel_last_frame = Some(cx.background_executor().now());
                }

                if !self.mouse_wheel_animation_running {
                    self.mouse_wheel_animation_running = true;
                    self.mouse_wheel_last_frame = Some(cx.background_executor().now());
                    cx.on_next_frame(window, |this, window, cx| {
                        this.advance_mouse_wheel_animation(window, cx)
                    });
                }
            }
        }
    }

    fn advance_mouse_wheel_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.mouse_wheel_animation_running {
            return;
        }

        let Some(mut target) = self.mouse_wheel_target else {
            self.cancel_mouse_wheel_animation();
            return;
        };

        // Content and viewport size may change while an animation is active.
        // Re-clamp on every frame so a stale target cannot request frames
        // forever after a resize or content removal.
        let max_offset = f32::from(self.active_scroll().max_offset().y).max(0.0);
        target = target.clamp(-max_offset, 0.0);
        self.mouse_wheel_target = Some(target);

        let current = f32::from(self.active_scroll().offset().y).clamp(-max_offset, 0.0);
        let now = cx.background_executor().now();
        let elapsed = self
            .mouse_wheel_last_frame
            .replace(now)
            .map_or(Duration::from_millis(16), |last_frame| {
                now.saturating_duration_since(last_frame)
            });
        let remaining = (target - current).abs();

        let next = if cx.reduce_motion() || remaining <= MOUSE_WHEEL_SETTLE_DISTANCE {
            target
        } else {
            smoothed_scroll_position(current, target, elapsed)
        };
        self.set_scroll_y(next);

        if next == target || (target - next).abs() <= MOUSE_WHEEL_SETTLE_DISTANCE {
            self.set_scroll_y(target);
            self.cancel_mouse_wheel_animation();
        }
        cx.notify();

        if self.mouse_wheel_animation_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_mouse_wheel_animation(window, cx)
            });
        }
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
            || (self.hovered_thread.is_none() && self.hovered_recent_thread.is_none())
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
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pinned_menu_focus.is_none() {
            self.pinned_menu_focus = Some(cx.focus_handle().tab_stop(true));
        }
        if self.projects_menu_focus.is_none() {
            self.projects_menu_focus = Some(cx.focus_handle().tab_stop(true));
        }
        if self.project_menu_focus.is_none() {
            self.project_menu_focus = Some(cx.focus_handle().tab_stop(true));
        }
        if std::mem::take(&mut self.scroll_to_bottom) {
            self.scroll.scroll_to_bottom();
        }
        if let Some(offset) = self.activity_scroll_to.take() {
            self.activity_scroll.set_offset(point(px(0.0), px(-offset)));
        }
        let on_scroll = cx.listener(Self::handle_scroll_wheel);
        let scroll = self.active_scroll().clone();
        self.sidebar(
            Theme::for_mode(self.mode),
            &scroll,
            self.scrollbar_visible,
            on_scroll,
            window,
            cx,
        )
    }
}

fn nav_row(label: &'static str, glyph: &'static str, theme: Theme) -> impl IntoElement {
    div()
        // Hover refinements need a stable element identity so GPUI can retain
        // their state between frames. Without it, every mouse move requests a
        // repaint against a fresh hitbox, which makes the row flicker or lag.
        .id(glyph)
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
        .when(label == "新对话", |row| {
            row.child(icon("quick-chat", theme.sidebar_text_muted.into()))
        })
}

fn sidebar_header_icon_button(name: &'static str, theme: Theme) -> impl IntoElement {
    div()
        .id(("sidebar-header-icon", name.len()))
        .size(px(24.0))
        .flex_none()
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |style| style.bg(theme.sidebar_hover))
        .child(icon(name, theme.sidebar_text_muted.into()).size(px(16.0)))
}

fn action_icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    theme: Theme,
) -> gpui::Stateful<Div> {
    let glyph_icon = if glyph == "pin" {
        icon(glyph, theme.sidebar_icon_muted.into())
            .with_transformation(Transformation::translate(point(px(1.0), px(0.0))))
    } else {
        icon(glyph, theme.sidebar_icon_muted.into())
    };

    div()
        .id(id)
        .size(px(24.0))
        .flex_none()
        .rounded(px(12.5))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |style| style.bg(theme.sidebar_hover))
        .child(glyph_icon)
}

fn section_header_icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    theme: Theme,
    highlighted: bool,
    icon_size: f32,
) -> gpui::Stateful<Div> {
    let color = if highlighted {
        theme.text
    } else {
        theme.sidebar_icon_muted
    };

    div()
        .id(id)
        .size(px(24.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(icon(glyph, color.into()).size(px(icon_size)))
}

fn projects_menu_label(label: &'static str, theme: Theme) -> Div {
    div()
        // GPUI snaps each stacked row independently. A 25.5625px layout box
        // reproduces the reference's 26.5625px CSS row after native text
        // rasterization without accumulating a pixel at each group label.
        .h(px(25.5625))
        .px(px(8.0))
        .py(px(4.0))
        .text_size(px(13.0))
        .line_height(px(PROFILE_MENU_LINE_HEIGHT))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.sidebar_text_muted)
        .child(label)
}

fn profile_menu_item(
    label: &'static str,
    glyph: &'static str,
    trailing: Option<&'static str>,
    theme: Theme,
) -> gpui::Stateful<Div> {
    div()
        .id(label)
        .h(px(PROFILE_MENU_ROW_HEIGHT))
        .px(px(8.0))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_color(theme.sidebar_text)
        .cursor_pointer()
        .hover(move |style| style.bg(theme.sidebar_hover))
        .child(
            icon(glyph, theme.sidebar_text.into())
                .size(px(16.0))
                .opacity(0.75),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .whitespace_nowrap()
                .child(label),
        )
        .when_some(trailing, |row, trailing| {
            row.child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .when(label == "设置", |shortcut| {
                        shortcut.text_size(px(12.0)).line_height(px(16.0))
                    })
                    .text_color(theme.sidebar_text_muted)
                    .child(trailing),
            )
        })
}

fn profile_menu(theme: Theme, cx: &mut Context<SidebarView>) -> gpui::Stateful<Div> {
    div()
        .id("profile-menu")
        .absolute()
        .left(px(8.0))
        .right(px(8.0))
        .bottom(px(43.0))
        .p(px(4.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.profile_menu_surface)
        .shadow(vec![
            gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                .blur_radius(px(16.0))
                .spread_radius(px(-4.0)),
        ])
        .font_family(".SystemUIFont")
        .text_size(px(PROFILE_MENU_TEXT_SIZE))
        .line_height(px(PROFILE_MENU_LINE_HEIGHT))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.sidebar_text)
        .child(
            div()
                .id("profile-menu-account")
                .h(px(30.0))
                .px(px(8.0))
                .rounded(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                // Radix focuses the first menu item when this captured menu
                // opens; its `focus:bg-primary-ghost-hover` is therefore
                // visible in both reference themes.
                .bg(theme.profile_menu_focus)
                .cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
                .child(
                    div()
                        .size(px(20.0))
                        .flex_none()
                        .rounded_full()
                        .bg(theme.control)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(7.0))
                        .text_color(theme.text)
                        .child("RI"),
                )
                .child("rita"),
        )
        .child(
            div()
                .h(px(9.0))
                .px(px(8.0))
                .py(px(4.0))
                .child(div().h(px(1.0)).w_full().bg(theme.border)),
        )
        .child(profile_menu_item(
            "使用情况",
            "profile-usage",
            Some("剩余 81%"),
            theme,
        ))
        .child(profile_menu_item("隐藏宠物", "profile-pet", None, theme))
        .child(profile_menu_item("邀请好友", "profile-invite", None, theme))
        .child(
            profile_menu_item("设置", "profile-settings", Some("⌘,"), theme).on_click(cx.listener(
                |this, _, _, cx| {
                    this.profile_menu_open = false;
                    cx.emit(OpenSettings);
                    cx.notify();
                },
            )),
        )
        .child(profile_menu_item("退出登录", "profile-logout", None, theme))
}

impl SidebarView {
    fn pinned_entries(&self) -> Vec<ActivityThread> {
        let mut entries = Vec::new();
        for (project, (_, rows)) in PROJECTS.iter().enumerate() {
            for (row, title) in rows.iter().enumerate() {
                let entry = ActivityThread::Project(project, row);
                if *title != "展开显示"
                    && self.pinned_threads[project][row]
                    && !self.archived_threads[project][row]
                {
                    entries.push(entry);
                }
            }
        }
        for index in 0..RECENTS.len() {
            if self.pinned_recents[index] && !self.archived_recents[index] {
                entries.push(ActivityThread::Recent(index));
            }
        }

        // The existing project data does not carry priority or update-time
        // metadata. The reference preserves the visible order for the three
        // captured threads in every mode, so keep this source order stable
        // instead of inventing timestamps or priority values.
        entries
    }

    fn select_pinned_menu_item(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pinned_sort = match index {
            0 => ProjectsSort::Priority,
            1 => ProjectsSort::Recent,
            2 => ProjectsSort::Manual,
            _ => return,
        };
        self.pinned_menu_open = false;
        self.pinned_menu_focused_item = None;
        cx.notify();
    }

    fn set_pinned_menu_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.pinned_menu_open = open;
        self.pinned_menu_focused_item = None;
        self.projects_section_menu_open = false;
        self.projects_menu_focused_item = None;
        self.open_project_menu = None;
        self.profile_menu_open = false;
        if open && let Some(focus) = self.pinned_menu_focus.as_ref() {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn handle_pinned_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !self.pinned_menu_open {
            if matches!(key, "enter" | "space" | "down") {
                self.set_pinned_menu_open(true, window, cx);
                if key == "down" {
                    self.pinned_menu_focused_item = Some(0);
                }
                cx.stop_propagation();
            }
            return;
        }

        match key {
            "down" => {
                self.pinned_menu_focused_item = Some(
                    self.pinned_menu_focused_item
                        .map_or(0, |index| (index + 1) % 3),
                );
            }
            "up" => {
                self.pinned_menu_focused_item = Some(
                    self.pinned_menu_focused_item
                        .map_or(2, |index| (index + 2) % 3),
                );
            }
            "home" => self.pinned_menu_focused_item = Some(0),
            "end" => self.pinned_menu_focused_item = Some(2),
            "enter" | "space" => {
                if let Some(index) = self.pinned_menu_focused_item {
                    self.select_pinned_menu_item(index, cx);
                }
            }
            "escape" | "tab" => {
                self.pinned_menu_open = false;
                self.pinned_menu_focused_item = None;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn pinned_menu_item(
        &self,
        index: usize,
        label: &'static str,
        checked: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.pinned_menu_focused_item == Some(index);
        div()
            .id(("pinned-menu-item", index))
            .h(px(28.5625))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(PROFILE_MENU_LINE_HEIGHT))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.sidebar_text)
            .when(focused, |item| item.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.pinned_menu_focused_item = Some(index);
                    if let Some(focus) = this.pinned_menu_focus.as_ref() {
                        focus.focus(window, cx);
                    }
                    cx.notify();
                } else if this.pinned_menu_focused_item == Some(index) {
                    this.pinned_menu_focused_item = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_pinned_menu_item(index, cx);
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!checked, |check| check.invisible())
                    .child(
                        icon("check", theme.text.into())
                            .size(px(16.0))
                            .opacity(if focused { 1.0 } else { 0.75 }),
                    ),
            )
            .child(div().min_w(px(0.0)).flex_1().child(label))
    }

    fn pinned_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("pinned-menu")
            .w(px(172.0))
            .p(px(4.0))
            .m(px(1.0))
            .rounded(px(15.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_color(theme.sidebar_text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.open_project_menu = None;
                this.project_menu_focused_item = None;
                cx.notify();
            }))
            .child(projects_menu_label("置顶聊天排序方式", theme))
            .child(self.pinned_menu_item(
                0,
                "优先级",
                self.pinned_sort == ProjectsSort::Priority,
                theme,
                cx,
            ))
            .child(self.pinned_menu_item(
                1,
                "最近更新",
                self.pinned_sort == ProjectsSort::Recent,
                theme,
                cx,
            ))
            .child(self.pinned_menu_item(
                2,
                "手动排序",
                self.pinned_sort == ProjectsSort::Manual,
                theme,
                cx,
            ))
    }

    fn select_projects_menu_item(&mut self, index: usize, cx: &mut Context<Self>) {
        match index {
            0 => self.projects_layout = ProjectsLayout::Grouped,
            1 => self.projects_layout = ProjectsLayout::Flat,
            2 => self.projects_sort = ProjectsSort::Priority,
            3 => self.projects_sort = ProjectsSort::Recent,
            4 => self.projects_sort = ProjectsSort::Manual,
            _ => return,
        }
        self.projects_section_menu_open = false;
        self.projects_menu_focused_item = None;
        self.open_project_menu = None;
        cx.notify();
    }

    fn handle_projects_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if !self.projects_section_menu_open {
            if matches!(key, "enter" | "space" | "down") {
                self.set_projects_section_menu_open(true, window, cx);
                if key == "down" {
                    self.projects_menu_focused_item = Some(0);
                }
                cx.stop_propagation();
            }
            return;
        }

        match key {
            "down" => {
                self.projects_menu_focused_item = Some(
                    self.projects_menu_focused_item
                        .map_or(0, |index| (index + 1) % 5),
                );
            }
            "up" => {
                self.projects_menu_focused_item = Some(
                    self.projects_menu_focused_item
                        .map_or(4, |index| (index + 4) % 5),
                );
            }
            "home" => self.projects_menu_focused_item = Some(0),
            "end" => self.projects_menu_focused_item = Some(4),
            "enter" | "space" => {
                if let Some(index) = self.projects_menu_focused_item {
                    self.select_projects_menu_item(index, cx);
                }
            }
            "escape" => {
                self.projects_section_menu_open = false;
                self.projects_menu_focused_item = None;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn projects_menu_item(
        &self,
        index: usize,
        label: &'static str,
        checked: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.projects_menu_focused_item == Some(index);
        div()
            .id(("projects-menu-item", index))
            .h(px(28.5625))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(PROFILE_MENU_LINE_HEIGHT))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.sidebar_text)
            .when(focused, |item| item.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.projects_menu_focused_item = Some(index);
                    if let Some(focus) = this.projects_menu_focus.as_ref() {
                        focus.focus(window, cx);
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_projects_menu_item(index, cx);
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!checked, |check| check.invisible())
                    .child(
                        icon("check", theme.text.into())
                            .size(px(16.0))
                            .opacity(if focused { 1.0 } else { 0.75 }),
                    ),
            )
            .child(div().min_w(px(0.0)).flex_1().child(label))
    }

    fn projects_section_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("projects-section-menu")
            .w(px(172.0))
            .p(px(4.0))
            .m(px(1.0))
            .rounded(px(15.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                gpui::BoxShadow::new(px(0.0), px(8.0), theme.profile_menu_shadow.into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_color(theme.sidebar_text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(projects_menu_label("整理侧边栏", theme))
            .child(self.projects_menu_item(
                0,
                "按项目",
                self.projects_layout == ProjectsLayout::Grouped,
                theme,
                cx,
            ))
            .child(self.projects_menu_item(
                1,
                "在一个列表中",
                self.projects_layout == ProjectsLayout::Flat,
                theme,
                cx,
            ))
            .child(projects_menu_label("聊天排序方式", theme))
            .child(self.projects_menu_item(
                2,
                "优先级",
                self.projects_sort == ProjectsSort::Priority,
                theme,
                cx,
            ))
            .child(self.projects_menu_item(
                3,
                "最近更新",
                self.projects_sort == ProjectsSort::Recent,
                theme,
                cx,
            ))
            .child(self.projects_menu_item(
                4,
                "手动排序",
                self.projects_sort == ProjectsSort::Manual,
                theme,
                cx,
            ))
    }

    pub fn open_project_menu_for_capture(&mut self, project_index: usize, cx: &mut Context<Self>) {
        if project_index < PROJECTS.len() && !self.removed_projects[project_index] {
            self.open_project_menu = Some(project_index);
            self.project_menu_focused_item = None;
            self.pinned_menu_open = false;
            self.projects_section_menu_open = false;
            self.profile_menu_open = false;
            cx.notify();
        }
    }

    fn set_project_menu_open(
        &mut self,
        project: usize,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_project_menu = open.then_some(project);
        self.project_menu_focused_item = None;
        self.pinned_menu_open = false;
        self.pinned_menu_focused_item = None;
        self.projects_section_menu_open = false;
        self.projects_menu_focused_item = None;
        self.profile_menu_open = false;
        if open && let Some(focus) = self.project_menu_focus.as_ref() {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn select_project_menu_item(&mut self, project: usize, item: usize, cx: &mut Context<Self>) {
        if project >= PROJECTS.len() {
            let created = project - PROJECTS.len();
            if created >= self.created_projects.len() {
                return;
            }
            match item {
                0 => {
                    self.pinned_created_projects[created] = !self.pinned_created_projects[created];
                }
                1..=4 => {}
                5 => {
                    self.created_projects.remove(created);
                    self.pinned_created_projects.remove(created);
                    self.selected_created_project =
                        self.selected_created_project.and_then(|index| {
                            if index == created {
                                None
                            } else if index > created {
                                Some(index - 1)
                            } else {
                                Some(index)
                            }
                        });
                }
                _ => return,
            }
            self.open_project_menu = None;
            self.project_menu_focused_item = None;
            cx.notify();
            return;
        }
        match item {
            0 => self.pinned_projects[project] = !self.pinned_projects[project],
            // Edit, Reveal in Finder, and Create permanent worktree are host-backed
            // actions in the reference. This static replica has no invented paths
            // or host service, so selecting them faithfully dismisses the native
            // menu without fabricating external state.
            1..=3 => {}
            4 => {
                for archived in &mut self.archived_threads[project] {
                    *archived = true;
                }
                if self
                    .selected_thread
                    .is_some_and(|(owner, _)| owner == project)
                {
                    self.selected_thread = None;
                }
            }
            5 => {
                self.removed_projects[project] = true;
                if self.selected_project == project {
                    self.selected_thread = None;
                }
            }
            _ => return,
        }
        self.open_project_menu = None;
        self.project_menu_focused_item = None;
        cx.notify();
    }

    fn handle_project_menu_key(
        &mut self,
        project: usize,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.open_project_menu != Some(project) {
            if matches!(key, "enter" | "space" | "down") {
                self.set_project_menu_open(project, true, window, cx);
                if key == "down" {
                    self.project_menu_focused_item = Some(0);
                }
                cx.stop_propagation();
            }
            return;
        }
        const ITEM_COUNT: usize = 6;
        match key {
            "down" => {
                self.project_menu_focused_item = Some(
                    self.project_menu_focused_item
                        .map_or(0, |index| (index + 1) % ITEM_COUNT),
                );
            }
            "up" => {
                self.project_menu_focused_item = Some(
                    self.project_menu_focused_item
                        .map_or(ITEM_COUNT - 1, |index| {
                            (index + ITEM_COUNT - 1) % ITEM_COUNT
                        }),
                );
            }
            "home" => self.project_menu_focused_item = Some(0),
            "end" => self.project_menu_focused_item = Some(ITEM_COUNT - 1),
            "enter" | "space" => {
                if let Some(item) = self.project_menu_focused_item {
                    self.select_project_menu_item(project, item, cx);
                }
            }
            "escape" | "tab" => {
                self.open_project_menu = None;
                self.project_menu_focused_item = None;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn project_menu_item(
        &self,
        project: usize,
        item: usize,
        label: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.project_menu_focused_item == Some(item);
        let glyph = icon(glyph, theme.sidebar_text.into())
            .size(px(16.0))
            .when(glyph == "settings-edit", |icon| {
                icon.with_transformation(Transformation::translate(point(px(-0.4), px(0.0))))
            });
        div()
            .id(("project-context-menu-item", project * 8 + item))
            .h(px(25.0))
            .px(px(6.0))
            .rounded(px(5.0))
            .flex()
            .items_center()
            .gap(px(7.0))
            .cursor_pointer()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.sidebar_text)
            .when(focused, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.text.alpha(0.12)))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.project_menu_focused_item = Some(item);
                    if let Some(focus) = this.project_menu_focus.as_ref() {
                        focus.focus(window, cx);
                    }
                    cx.notify();
                } else if this.project_menu_focused_item == Some(item) {
                    this.project_menu_focused_item = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_project_menu_item(project, item, cx);
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph),
            )
            .child(label)
    }

    fn project_menu_separator(theme: Theme) -> Div {
        div()
            .h(px(9.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .child(div().h(px(0.5)).w_full().bg(theme.border))
    }

    fn project_context_menu(
        &self,
        project: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let pinned = if project < PROJECTS.len() {
            self.pinned_projects[project]
        } else {
            self.pinned_created_projects[project - PROJECTS.len()]
        };
        let pin_label = if pinned { "取消置顶" } else { "置顶" };
        div()
            .id(("project-context-menu", project))
            .w(px(190.0))
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
            .text_color(theme.sidebar_text)
            .when_some(self.project_menu_focus.as_ref(), |menu, focus| {
                menu.track_focus(focus)
            })
            .on_key_down(cx.listener(move |this, event, window, cx| {
                this.handle_project_menu_key(project, event, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.project_menu_item(project, 0, pin_label, "pin", theme, cx))
            .child(self.project_menu_item(project, 1, "编辑", "settings-edit", theme, cx))
            .child(Self::project_menu_separator(theme))
            .child(self.project_menu_item(
                project,
                2,
                "在 Finder 中显示",
                "project-reveal",
                theme,
                cx,
            ))
            .child(self.project_menu_item(
                project,
                3,
                "创建永久工作树",
                "project-worktree",
                theme,
                cx,
            ))
            .child(Self::project_menu_separator(theme))
            .child(self.project_menu_item(project, 4, "归档聊天", "archive", theme, cx))
            .child(Self::project_menu_separator(theme))
            .child(self.project_menu_item(project, 5, "移除项目", "close-dialog", theme, cx))
    }

    fn project_group(
        &self,
        project_index: usize,
        name: &'static str,
        rows: &'static [&'static str],
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let flat = self.projects_layout == ProjectsLayout::Flat;
        let collapsed = !flat && self.project_collapsed[project_index];
        let selected =
            self.selected_created_project.is_none() && self.selected_project == project_index;
        let menu_open = self.open_project_menu == Some(project_index);
        let group_name = format!("project-row-{project_index}");

        let menu_button = action_icon_button(
            ("project-menu-button", project_index),
            "more-horizontal",
            theme,
        )
        .when_some(self.project_menu_focus.as_ref(), |button, focus| {
            button.track_focus(focus)
        })
        .on_key_down(cx.listener(move |this, event, window, cx| {
            this.handle_project_menu_key(project_index, event, window, cx);
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let open = this.open_project_menu != Some(project_index);
            this.set_project_menu_open(project_index, open, window, cx);
        }));
        let new_chat_button =
            action_icon_button(("project-new-chat", project_index), "new-chat", theme).on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.selected_project = project_index;
                    this.selected_created_project = None;
                    this.selected_thread = None;
                    this.selected_recent = None;
                    this.open_project_menu = None;
                    cx.notify();
                }),
            );

        let trailing_actions = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!menu_open, |actions| {
                actions
                    .invisible()
                    .group_hover(group_name.clone(), |style| style.visible())
            })
            .child(menu_button)
            .child(new_chat_button);

        let heading = div()
            .id(("project-row", project_index))
            .group(group_name)
            .h(px(30.0))
            .w_full()
            .relative()
            .pl(px(1.0))
            .pr(px(6.0))
            .flex()
            .items_center()
            .rounded(px(12.5))
            .text_size(px(14.0))
            .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
            .text_color(theme.sidebar_text)
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .size(px(30.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("folder", theme.sidebar_text.into())),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(trailing_actions)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_project_menu = None;
                this.project_collapsed[project_index] = !this.project_collapsed[project_index];
                this.selected_created_project = None;
                cx.notify();
            }));

        let mut group = div()
            .id(("project-group", project_index))
            .relative()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .when(!flat, |group| group.child(heading));

        if !collapsed {
            let mut row_indices: Vec<_> = (0..rows.len()).collect();
            if self.projects_sort == ProjectsSort::Priority {
                row_indices.sort_by_key(|row| !self.pinned_threads[project_index][*row]);
            }
            for row_index in row_indices {
                let row = &rows[row_index];
                if self.archived_threads[project_index][row_index] {
                    continue;
                }
                if *row == "展开显示" {
                    if flat {
                        continue;
                    }
                    let show_all = self.project_show_all[project_index];
                    group = group.child(
                        div()
                            .id(("project-show-all", project_index))
                            .h(px(30.0))
                            .pl(px(32.0))
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .text_color(theme.sidebar_text_muted)
                            .cursor_pointer()
                            .hover(move |style| style.text_color(theme.sidebar_text))
                            .child(if show_all { "收起" } else { "展开显示" })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.project_show_all[project_index] =
                                    !this.project_show_all[project_index];
                                cx.notify();
                            })),
                    );
                    continue;
                }

                let selected_thread = self.selected_thread == Some((project_index, row_index));
                let pinned = self.pinned_threads[project_index][row_index];
                if pinned {
                    continue;
                }
                let thread_hovered = self.hovered_thread == Some((project_index, row_index));
                let show_thread_actions = pinned || thread_hovered;
                let title_viewport_width = sidebar_thread_title_viewport_width(
                    self.sidebar_width,
                    flat,
                    show_thread_actions,
                );
                let title_width = Self::thread_title_width(row, window);
                let title_overflows = title_width > title_viewport_width;
                let title_scroll_distance = if thread_hovered {
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
                let thread_group = format!("thread-row-{project_index}-{row_index}");
                let pin_button = action_icon_button(
                    format!("thread-pin-{project_index}-{row_index}"),
                    "pin",
                    theme,
                )
                .w(px(19.0))
                .h(px(20.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.pinned_threads[project_index][row_index] =
                        !this.pinned_threads[project_index][row_index];
                    cx.notify();
                }));
                let archive_button = action_icon_button(
                    format!("thread-archive-{project_index}-{row_index}"),
                    "archive",
                    theme,
                )
                .w(px(19.0))
                .h(px(20.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.archived_threads[project_index][row_index] = true;
                    if this.selected_thread == Some((project_index, row_index)) {
                        this.selected_thread = None;
                    }
                    cx.notify();
                }));
                let actions = div()
                    .absolute()
                    .right(px(8.0))
                    .top(px(5.0))
                    .flex()
                    .gap(px(8.0))
                    .when(!show_thread_actions, |actions| actions.invisible())
                    .child(pin_button)
                    .child(archive_button);
                let trailing_rail = div()
                    .ml(px(3.0))
                    .flex_none()
                    .when(show_thread_actions, |rail| rail.w(px(48.0)).min_w(px(48.0)))
                    .when(!show_thread_actions, |rail| rail.w(px(0.0)).min_w(px(0.0)));

                group = group.child(
                    div()
                        .id(format!("thread-row-{project_index}-{row_index}"))
                        .group(thread_group)
                        .h(px(30.0))
                        .pl(px(8.0))
                        .pr(px(5.0))
                        .flex()
                        .items_center()
                        .rounded(px(8.0))
                        .text_size(px(14.0))
                        .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
                        .text_color(theme.sidebar_text)
                        .relative()
                        .top(px(1.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .when(selected_thread, |row| row.bg(theme.sidebar_hover))
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
                                        .when(!flat, |title| {
                                            title.gap(px(8.0)).child(div().w(px(16.0)).flex_none())
                                        })
                                        .child(div().min_w(px(0.0)).flex_1().h(px(20.0)).child(
                                            activity_title_canvas(
                                                row,
                                                theme.sidebar_text.into(),
                                                title_scroll_offset,
                                                title_overflows,
                                            ),
                                        )),
                                )
                                .child(trailing_rail),
                        )
                        .child(actions)
                        .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                            let target = (project_index, row_index);
                            if *hovered {
                                if this.hovered_thread != Some(target) {
                                    this.hovered_thread = Some(target);
                                    this.hovered_recent_thread = None;
                                    let viewport_width = sidebar_thread_title_viewport_width(
                                        this.sidebar_width,
                                        flat,
                                        true,
                                    );
                                    this.start_marquee(row, viewport_width, window, cx);
                                    cx.notify();
                                }
                            } else if this.hovered_thread == Some(target) {
                                this.hovered_thread = None;
                                this.stop_marquee();
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_project = project_index;
                            this.selected_thread = Some((project_index, row_index));
                            this.selected_recent = None;
                            this.open_project_menu = None;
                            cx.notify();
                        })),
                );
            }
        }
        // Paint the popup after the thread rows so it remains above them.
        if menu_open {
            group = group
                .child(deferred(
                    self.project_context_menu(project_index, theme, cx)
                        .absolute()
                        // Electron opens the native menu at the pointer hotspot. The
                        // CDP-measured trigger center is 171.8 px from this group.
                        .left(px(171.5))
                        .top(px(15.0)),
                ))
                // A native NSMenu consumes a second press at the trigger point
                // and dismisses itself. Keep that hotspot above the GPUI popup.
                .child(deferred(
                    div()
                        .id(("project-menu-dismiss-hotspot", project_index))
                        .absolute()
                        .left(px(165.0))
                        .top(px(3.0))
                        .size(px(24.0))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_project_menu = None;
                            this.project_menu_focused_item = None;
                            cx.notify();
                        })),
                ));
        }
        group
    }

    fn created_project_row(
        &self,
        project_index: usize,
        name: String,
        theme: Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let menu_project = PROJECTS.len() + project_index;
        let selected = self.selected_created_project == Some(project_index);
        let menu_open = self.open_project_menu == Some(menu_project);
        let group_name = format!("created-project-row-{project_index}");
        let menu_button = action_icon_button(
            ("created-project-menu-button", project_index),
            "more-horizontal",
            theme,
        )
        .when_some(self.project_menu_focus.as_ref(), |button, focus| {
            button.track_focus(focus)
        })
        .on_key_down(cx.listener(move |this, event, window, cx| {
            this.handle_project_menu_key(menu_project, event, window, cx);
        }))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            let open = this.open_project_menu != Some(menu_project);
            this.set_project_menu_open(menu_project, open, window, cx);
        }));
        let new_chat_button = action_icon_button(
            ("created-project-new-chat", project_index),
            "new-chat",
            theme,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.selected_created_project = Some(project_index);
            this.selected_thread = None;
            this.selected_recent = None;
            this.open_project_menu = None;
            cx.notify();
        }));
        let trailing_actions = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(!menu_open, |actions| {
                actions
                    .invisible()
                    .group_hover(group_name.clone(), |style| style.visible())
            })
            .child(menu_button)
            .child(new_chat_button);

        let row = div()
            .id(("created-project-row", project_index))
            .group(group_name)
            .h(px(30.0))
            .w_full()
            .pl(px(1.0))
            .pr(px(6.0))
            .flex()
            .items_center()
            .rounded(px(12.5))
            .text_size(px(14.0))
            .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
            .text_color(theme.sidebar_text)
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .size(px(30.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon("folder", theme.sidebar_text.into())),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(trailing_actions)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_created_project = Some(project_index);
                this.selected_thread = None;
                this.selected_recent = None;
                this.open_project_menu = None;
                cx.notify();
            }));

        div()
            .id(("created-project-group", project_index))
            .relative()
            .child(row)
            .when(menu_open, |group| {
                group
                    .child(deferred(
                        self.project_context_menu(menu_project, theme, cx)
                            .absolute()
                            .left(px(171.5))
                            .top(px(15.0)),
                    ))
                    .child(deferred(
                        div()
                            .id(("created-project-menu-dismiss-hotspot", project_index))
                            .absolute()
                            .left(px(165.0))
                            .top(px(3.0))
                            .size(px(24.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.open_project_menu = None;
                                this.project_menu_focused_item = None;
                                cx.notify();
                            })),
                    ))
            })
    }

    fn pinned_thread_row(
        &self,
        entry: ActivityThread,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let title = Self::activity_title(entry);
        let hovered = self.activity_hovered(entry);
        let selected = match entry {
            ActivityThread::Project(project, row) => self.selected_thread == Some((project, row)),
            ActivityThread::Recent(index) => self.selected_recent == Some(index),
        };
        let title_viewport_width =
            sidebar_thread_title_viewport_width(self.sidebar_width, true, true);
        let title_width = Self::thread_title_width(title, window);
        let title_overflows = title_width > title_viewport_width;
        let title_scroll_offset = self.marquee_started_at.map_or(0.0, |started_at| {
            marquee_offset(
                if hovered {
                    (title_width - title_viewport_width).max(0.0)
                } else {
                    0.0
                },
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started_at),
                cx.reduce_motion(),
            )
        });
        let pin = action_icon_button(format!("pinned-thread-pin-{entry:?}"), "pin", theme)
            .w(px(19.0))
            .h(px(20.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                match entry {
                    ActivityThread::Project(project, row) => {
                        this.pinned_threads[project][row] = false
                    }
                    ActivityThread::Recent(index) => this.pinned_recents[index] = false,
                }
                if this.pinned_entries().is_empty() {
                    this.pinned_menu_open = false;
                    this.pinned_menu_focused_item = None;
                }
                cx.notify();
            }));
        let archive =
            action_icon_button(format!("pinned-thread-archive-{entry:?}"), "archive", theme)
                .w(px(19.0))
                .h(px(20.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    match entry {
                        ActivityThread::Project(project, row) => {
                            this.archived_threads[project][row] = true;
                            if this.selected_thread == Some((project, row)) {
                                this.selected_thread = None;
                            }
                        }
                        ActivityThread::Recent(index) => {
                            this.archived_recents[index] = true;
                            if this.selected_recent == Some(index) {
                                this.selected_recent = None;
                            }
                        }
                    }
                    if this.pinned_entries().is_empty() {
                        this.pinned_menu_open = false;
                        this.pinned_menu_focused_item = None;
                    }
                    cx.notify();
                }));
        let actions = div()
            .absolute()
            .right(px(8.0))
            .top(px(5.0))
            .flex()
            .gap(px(8.0))
            .child(pin)
            .child(archive);

        div()
            .id(format!("pinned-thread-row-{entry:?}"))
            .h(px(30.0))
            .pl(px(8.0))
            .pr(px(5.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .text_size(px(14.0))
            .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
            .text_color(theme.sidebar_text)
            .relative()
            .overflow_hidden()
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
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
                            .h(px(20.0))
                            .child(activity_title_canvas(
                                title,
                                theme.sidebar_text.into(),
                                title_scroll_offset,
                                title_overflows,
                            )),
                    )
                    .child(div().ml(px(3.0)).w(px(48.0)).min_w(px(48.0)).flex_none()),
            )
            .child(actions)
            .on_hover(cx.listener(move |this, is_hovered: &bool, window, cx| {
                if *is_hovered {
                    match entry {
                        ActivityThread::Project(project, row) => {
                            this.hovered_thread = Some((project, row));
                            this.hovered_recent_thread = None;
                        }
                        ActivityThread::Recent(index) => {
                            this.hovered_recent_thread = Some(index);
                            this.hovered_thread = None;
                        }
                    }
                    this.start_marquee(title, title_viewport_width, window, cx);
                } else if this.activity_hovered(entry) {
                    this.hovered_thread = None;
                    this.hovered_recent_thread = None;
                    this.stop_marquee();
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                match entry {
                    ActivityThread::Project(project, row) => {
                        this.selected_project = project;
                        this.selected_thread = Some((project, row));
                        this.selected_recent = None;
                    }
                    ActivityThread::Recent(index) => {
                        this.selected_thread = None;
                        this.selected_recent = Some(index);
                    }
                }
                this.open_project_menu = None;
                cx.notify();
            }))
    }

    fn pinned_section(
        &self,
        entries: &[ActivityThread],
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let show_actions = self.pinned_heading_hovered || self.pinned_menu_open;
        let chevron = icon("section-chevron", theme.sidebar_icon_muted.into())
            .size(px(14.0))
            .with_transformation(Transformation::rotate(radians(if self.pinned_collapsed {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.0
            })));
        let menu_button = section_header_icon_button(
            "pinned-section-menu-button",
            "more-horizontal",
            theme,
            self.pinned_menu_button_hovered,
            16.0,
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            if this.pinned_menu_button_hovered != *hovered {
                this.pinned_menu_button_hovered = *hovered;
                cx.notify();
            }
        }))
        .when_some(self.pinned_menu_focus.as_ref(), |button, focus| {
            button.track_focus(focus)
        })
        .on_key_down(cx.listener(Self::handle_pinned_menu_key))
        // Radix opens and closes this menu on pointer-down, before release.
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.set_pinned_menu_open(!this.pinned_menu_open, window, cx);
            }),
        )
        .on_click(|_, _, cx| cx.stop_propagation())
        .when(self.pinned_menu_open, |button| {
            button.child(deferred(
                self.pinned_menu(theme, cx)
                    .absolute()
                    .top(px(25.5))
                    .right(px(-1.0)),
            ))
        });
        let heading = div()
            .id("pinned-section-heading")
            .pl(px(8.0))
            .pr(px(2.0))
            .h(px(25.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .font_weight(SIDEBAR_SECTION_FONT_WEIGHT)
            .text_color(theme.sidebar_text_muted)
            .child(
                div()
                    .id("pinned-section-toggle")
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_default()
                    .child("置顶")
                    .child(
                        div()
                            .size(px(14.0))
                            .flex_none()
                            .when(!show_actions, |icon| icon.invisible())
                            .child(chevron),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pinned_collapsed = !this.pinned_collapsed;
                        this.pinned_menu_open = false;
                        this.pinned_menu_focused_item = None;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .when(!show_actions, |actions| actions.invisible())
                    .child(menu_button),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.pinned_heading_hovered != *hovered {
                    this.pinned_heading_hovered = *hovered;
                    cx.notify();
                }
            }));
        let mut section = div()
            // The sidebar section stack is visually lifted four pixels in the
            // reference; this also places the menu's inner surface at y=282
            // when the trigger starts at y=255.5.
            .relative()
            .top(px(-4.0))
            .px(px(8.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .child(heading);
        if !self.pinned_collapsed {
            for entry in entries.iter().copied() {
                section = section.child(self.pinned_thread_row(entry, theme, window, cx));
            }
        }
        section
    }

    fn native_scroll_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let pinned_entries = self.pinned_entries();
        let mut projects =
            div()
                .flex()
                .flex_col()
                .gap(px(if self.projects_layout == ProjectsLayout::Flat {
                    1.0
                } else {
                    10.0
                }));
        for (project_index, (name, rows)) in PROJECTS.iter().enumerate() {
            if self.removed_projects[project_index] {
                continue;
            }
            projects =
                projects.child(self.project_group(project_index, name, rows, theme, window, cx));
        }
        for (project_index, name) in self.created_projects.iter().cloned().enumerate() {
            projects =
                projects.child(self.created_project_row(project_index, name, theme, window, cx));
        }
        let show_recents_heading_icons = self.recents_heading_hovered;
        let recent_chevron_color = if self.hovered_recents_icon == Some(SectionHeaderIcon::Chevron)
        {
            theme.text
        } else {
            theme.sidebar_icon_muted
        };
        let recent_chevron = icon("section-chevron", recent_chevron_color.into())
            .size(px(14.0))
            .with_transformation(Transformation::rotate(radians(if self.recents_collapsed {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.0
            })));
        let recent_menu = section_header_icon_button(
            "recents-section-menu-button",
            "more-horizontal",
            theme,
            self.hovered_recents_icon == Some(SectionHeaderIcon::Menu),
            16.0,
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            let next = hovered.then_some(SectionHeaderIcon::Menu);
            if *hovered || this.hovered_recents_icon == Some(SectionHeaderIcon::Menu) {
                this.hovered_recents_icon = next;
                cx.notify();
            }
        }))
        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));
        let recent_new_chat = section_header_icon_button(
            "recents-new-chat-button",
            "new-chat",
            theme,
            self.hovered_recents_icon == Some(SectionHeaderIcon::NewChat),
            16.0,
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            let next = hovered.then_some(SectionHeaderIcon::NewChat);
            if *hovered || this.hovered_recents_icon == Some(SectionHeaderIcon::NewChat) {
                this.hovered_recents_icon = next;
                cx.notify();
            }
        }))
        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));
        let recent_heading = div()
            .id("recents-section-heading")
            .pl(px(8.0))
            .pr(px(2.0))
            .h(px(25.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .font_weight(SIDEBAR_SECTION_FONT_WEIGHT)
            .text_color(theme.sidebar_text_muted)
            .child(
                div()
                    .id("recents-section-toggle")
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_default()
                    .child("最近")
                    .child(
                        div()
                            .id("recents-section-chevron")
                            .size(px(14.0))
                            .flex_none()
                            .when(!show_recents_heading_icons, |chevron| chevron.invisible())
                            .child(recent_chevron)
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                let next = hovered.then_some(SectionHeaderIcon::Chevron);
                                if *hovered
                                    || this.hovered_recents_icon == Some(SectionHeaderIcon::Chevron)
                                {
                                    this.hovered_recents_icon = next;
                                    cx.notify();
                                }
                            })),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.recents_collapsed = !this.recents_collapsed;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .when(!show_recents_heading_icons, |actions| actions.invisible())
                    .child(recent_menu)
                    .child(recent_new_chat),
            )
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.recents_heading_hovered != *hovered {
                    this.recents_heading_hovered = *hovered;
                    if !*hovered {
                        this.hovered_recents_icon = None;
                    }
                    cx.notify();
                }
            }));

        let mut recents = div()
            .flex()
            .flex_col()
            .pt(px(10.0))
            .pb(px(13.0))
            .child(recent_heading);
        if !self.recents_collapsed {
            let mut recent_indices: Vec<_> = (0..RECENTS.len()).collect();
            if self.projects_sort == ProjectsSort::Priority {
                recent_indices.sort_by_key(|index| !self.pinned_recents[*index]);
            }
            for recent_index in recent_indices {
                let recent = &RECENTS[recent_index];
                if self.archived_recents[recent_index] {
                    continue;
                }
                let pinned = self.pinned_recents[recent_index];
                if pinned {
                    continue;
                }
                let hovered = self.hovered_recent_thread == Some(recent_index);
                let selected_recent = self.selected_recent == Some(recent_index);
                let show_actions = pinned || hovered;
                let title_viewport_width =
                    sidebar_thread_title_viewport_width(self.sidebar_width, true, show_actions);
                let title_width = Self::thread_title_width(recent, window);
                let title_overflows = title_width > title_viewport_width;
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
                let pin =
                    action_icon_button(format!("recent-thread-pin-{recent_index}"), "pin", theme)
                        .w(px(19.0))
                        .h(px(20.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.pinned_recents[recent_index] = !this.pinned_recents[recent_index];
                            cx.notify();
                        }));
                let archive = action_icon_button(
                    format!("recent-thread-archive-{recent_index}"),
                    "archive",
                    theme,
                )
                .w(px(19.0))
                .h(px(20.0))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.archived_recents[recent_index] = true;
                    this.hovered_recent_thread = None;
                    cx.notify();
                }));
                let actions = div()
                    .absolute()
                    .right(px(8.0))
                    .top(px(5.0))
                    .flex()
                    .gap(px(8.0))
                    .when(!show_actions, |actions| actions.invisible())
                    .child(pin)
                    .child(archive);
                let trailing_rail = div()
                    .ml(px(3.0))
                    .flex_none()
                    .when(show_actions, |rail| rail.w(px(48.0)).min_w(px(48.0)))
                    .when(!show_actions, |rail| rail.w(px(0.0)).min_w(px(0.0)));

                recents = recents.child(
                    div()
                        .id(("recent-thread-row", recent_index))
                        .h(px(30.0))
                        .pl(px(8.0))
                        .pr(px(5.0))
                        .flex()
                        .items_center()
                        .rounded(px(8.0))
                        .text_size(px(14.0))
                        .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
                        .text_color(theme.sidebar_text)
                        .relative()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.sidebar_hover))
                        .when(selected_recent, |row| row.bg(theme.sidebar_hover))
                        .child(
                            div()
                                .h_full()
                                .w_full()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .child(div().min_w(px(0.0)).flex_1().h(px(20.0)).child(
                                    activity_title_canvas(
                                        recent,
                                        theme.sidebar_text.into(),
                                        title_scroll_offset,
                                        title_overflows,
                                    ),
                                ))
                                .child(trailing_rail),
                        )
                        .child(actions)
                        .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                            if *hovered {
                                if this.hovered_recent_thread != Some(recent_index) {
                                    this.hovered_recent_thread = Some(recent_index);
                                    this.hovered_thread = None;
                                    let viewport_width = sidebar_thread_title_viewport_width(
                                        this.sidebar_width,
                                        true,
                                        true,
                                    );
                                    this.start_marquee(recent, viewport_width, window, cx);
                                    cx.notify();
                                }
                            } else if this.hovered_recent_thread == Some(recent_index) {
                                this.hovered_recent_thread = None;
                                this.stop_marquee();
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_thread = None;
                            this.selected_recent = Some(recent_index);
                            this.open_project_menu = None;
                            cx.notify();
                        })),
                );
            }
        }

        let show_heading_icons = self.projects_heading_hovered
            || self.projects_section_menu_open
            || self.project_creation_trigger_open;
        let chevron_highlighted = self.hovered_section_icon == Some(SectionHeaderIcon::Chevron);
        let chevron_color = if chevron_highlighted {
            theme.text
        } else {
            theme.sidebar_icon_muted
        };
        let section_chevron = icon("section-chevron", chevron_color.into())
            .size(px(14.0))
            .with_transformation(Transformation::rotate(radians(
                if self.projects_collapsed {
                    -std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                },
            )));
        let section_menu = section_header_icon_button(
            "projects-section-menu-button",
            "more-horizontal",
            theme,
            self.projects_section_menu_open
                || self.hovered_section_icon == Some(SectionHeaderIcon::Menu),
            16.0,
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            let next = hovered.then_some(SectionHeaderIcon::Menu);
            if *hovered || this.hovered_section_icon == Some(SectionHeaderIcon::Menu) {
                this.hovered_section_icon = next;
                cx.notify();
            }
        }))
        .when_some(self.projects_menu_focus.as_ref(), |button, focus| {
            button.track_focus(focus)
        })
        .on_key_down(cx.listener(Self::handle_projects_menu_key))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| cx.stop_propagation()),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            cx.stop_propagation();
            this.set_projects_section_menu_open(!this.projects_section_menu_open, window, cx);
        }))
        .when(self.projects_section_menu_open, |button| {
            button.child(deferred(
                self.projects_section_menu(theme, cx)
                    .absolute()
                    .top(px(25.5))
                    .left(px(0.0)),
            ))
        });
        let add_project = section_header_icon_button(
            "projects-add-button",
            "add",
            theme,
            self.project_creation_trigger_open
                || self.hovered_section_icon == Some(SectionHeaderIcon::Add),
            14.0,
        )
        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
            let next = hovered.then_some(SectionHeaderIcon::Add);
            if *hovered || this.hovered_section_icon == Some(SectionHeaderIcon::Add) {
                this.hovered_section_icon = next;
                cx.notify();
            }
        }))
        .on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            this.project_creation_trigger_open = true;
            cx.emit(OpenProjectCreation);
            cx.notify();
        }));

        let heading_actions = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .when(!show_heading_icons, |actions| actions.invisible())
            .child(section_menu)
            .child(add_project);

        let section_heading = div()
            .id("projects-section-heading")
            .relative()
            .top(px(-4.0))
            .pl(px(8.0))
            .pr(px(2.0))
            .h(px(25.0))
            .flex()
            .items_center()
            .text_size(px(14.0))
            .font_weight(SIDEBAR_SECTION_FONT_WEIGHT)
            .text_color(theme.sidebar_text_muted)
            .child(
                div()
                    .id("projects-section-toggle")
                    .min_w(px(0.0))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .cursor_default()
                    .child("项目")
                    .child(
                        div()
                            .id("projects-section-chevron")
                            .size(px(14.0))
                            .flex_none()
                            .when(!show_heading_icons, |chevron| chevron.invisible())
                            .child(section_chevron)
                            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                let next = hovered.then_some(SectionHeaderIcon::Chevron);
                                if *hovered
                                    || this.hovered_section_icon == Some(SectionHeaderIcon::Chevron)
                                {
                                    this.hovered_section_icon = next;
                                    cx.notify();
                                }
                            })),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.projects_collapsed = !this.projects_collapsed;
                        this.open_project_menu = None;
                        this.projects_section_menu_open = false;
                        cx.notify();
                    })),
            )
            .child(heading_actions)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.projects_heading_hovered != *hovered {
                    this.projects_heading_hovered = *hovered;
                    if !*hovered {
                        this.hovered_section_icon = None;
                    }
                    cx.notify();
                }
            }));

        let section = div().relative().px(px(8.0)).child(section_heading);

        div()
            .child(
                div()
                    .px(px(8.0))
                    .pb(px(21.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(nav_row("拉取请求", "pull-request", theme))
                    .child(nav_row("站点", "sites", theme))
                    .child(nav_row("已安排", "scheduled", theme))
                    .child(nav_row("插件", "plugins", theme)),
            )
            .when(!pinned_entries.is_empty(), |content| {
                content.child(self.pinned_section(&pinned_entries, theme, window, cx))
            })
            .child(section)
            .when(!self.projects_collapsed, |content| {
                content.child(div().px(px(8.0)).child(projects))
            })
            .child(div().px(px(8.0)).child(recents))
    }

    fn fallback_activity_thread(&self) -> Option<ActivityThread> {
        PROJECTS
            .get(self.selected_project)
            .and_then(|(_, rows)| {
                rows.iter()
                    .enumerate()
                    .find(|(row_index, title)| {
                        **title != "展开显示"
                            && !self.archived_threads[self.selected_project][*row_index]
                    })
                    .map(|(row_index, _)| ActivityThread::Project(self.selected_project, row_index))
            })
            .or_else(|| {
                RECENTS
                    .iter()
                    .enumerate()
                    .find(|(index, _)| !self.archived_recents[*index])
                    .map(|(index, _)| ActivityThread::Recent(index))
            })
    }

    fn selected_activity_thread(&self) -> Option<ActivityThread> {
        self.selected_thread
            .map(|(project, row)| ActivityThread::Project(project, row))
            .or_else(|| self.selected_recent.map(ActivityThread::Recent))
            .or(self.activity_running_thread)
    }

    fn activity_title(entry: ActivityThread) -> &'static str {
        match entry {
            ActivityThread::Project(project, row) => PROJECTS[project].1[row],
            ActivityThread::Recent(index) => RECENTS[index],
        }
    }

    fn activity_source(entry: ActivityThread) -> &'static str {
        match entry {
            ActivityThread::Project(project, _) => PROJECTS[project].0,
            ActivityThread::Recent(_) => "最近",
        }
    }

    fn activity_archived(&self, entry: ActivityThread) -> bool {
        match entry {
            ActivityThread::Project(project, row) => self.archived_threads[project][row],
            ActivityThread::Recent(index) => self.archived_recents[index],
        }
    }

    fn activity_pinned(&self, entry: ActivityThread) -> bool {
        match entry {
            ActivityThread::Project(project, row) => self.pinned_threads[project][row],
            ActivityThread::Recent(index) => self.pinned_recents[index],
        }
    }

    fn activity_hovered(&self, entry: ActivityThread) -> bool {
        match entry {
            ActivityThread::Project(project, row) => self.hovered_thread == Some((project, row)),
            ActivityThread::Recent(index) => self.hovered_recent_thread == Some(index),
        }
    }

    fn activity_priority_entries(&self) -> Vec<ActivityThread> {
        let mut entries = Vec::new();
        if let Some(running) = self
            .activity_running_thread
            .filter(|entry| !self.activity_archived(*entry))
        {
            entries.push(running);
        }
        for (project, (_, rows)) in PROJECTS.iter().enumerate() {
            for (row, title) in rows.iter().enumerate() {
                let entry = ActivityThread::Project(project, row);
                if *title != "展开显示"
                    && !self.activity_archived(entry)
                    && self.activity_pinned(entry)
                    && !entries.contains(&entry)
                {
                    entries.push(entry);
                }
            }
        }
        for index in 0..RECENTS.len() {
            let entry = ActivityThread::Recent(index);
            if !self.activity_archived(entry)
                && self.activity_pinned(entry)
                && !entries.contains(&entry)
            {
                entries.push(entry);
            }
        }
        entries
    }

    fn activity_today_entries(&self, priority: &[ActivityThread]) -> Vec<ActivityThread> {
        PROJECTS
            .iter()
            .enumerate()
            .filter_map(|(project, (_, rows))| {
                rows.iter().enumerate().find_map(|(row, title)| {
                    let entry = ActivityThread::Project(project, row);
                    (*title != "展开显示"
                        && !self.activity_archived(entry)
                        && !priority.contains(&entry))
                    .then_some(entry)
                })
            })
            .collect()
    }

    fn activity_yesterday_entries(
        &self,
        priority: &[ActivityThread],
        today: &[ActivityThread],
    ) -> Vec<ActivityThread> {
        let mut entries: Vec<_> = PROJECTS
            .iter()
            .enumerate()
            .flat_map(|(project, (_, rows))| {
                rows.iter().enumerate().filter_map(move |(row, title)| {
                    let entry = ActivityThread::Project(project, row);
                    (*title != "展开显示"
                        && !self.activity_archived(entry)
                        && !priority.contains(&entry)
                        && !today.contains(&entry))
                    .then_some(entry)
                })
            })
            .collect();
        entries.extend(RECENTS.iter().enumerate().filter_map(|(index, _)| {
            let entry = ActivityThread::Recent(index);
            (!self.activity_archived(entry) && !priority.contains(&entry)).then_some(entry)
        }));
        entries
    }

    fn activity_section_heading(
        &self,
        label: &'static str,
        with_actions: bool,
        theme: Theme,
    ) -> Div {
        div()
            .h(px(ACTIVITY_HEADING_HEIGHT))
            .pl(px(8.0))
            .pr(px(2.0))
            .flex_none()
            .flex()
            .items_center()
            .text_size(px(14.0))
            .line_height(px(21.0))
            .font_weight(SIDEBAR_SECTION_FONT_WEIGHT)
            .text_color(theme.sidebar_text_muted)
            .child(div().flex_1().py(px(2.0)).child(label))
            .when(with_actions, |heading| {
                heading.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .id("activity-options-button")
                                .size(px(24.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.sidebar_hover))
                                .child(
                                    icon("more-horizontal", theme.sidebar_icon_muted.into())
                                        .size(px(16.0)),
                                ),
                        )
                        .child(
                            div()
                                .id("activity-clear-read-button")
                                .size(px(24.0))
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.sidebar_hover))
                                .child(
                                    icon("activity-clear-read", theme.sidebar_icon_muted.into())
                                        .size(px(16.0)),
                                ),
                        ),
                )
            })
    }

    fn activity_thread_row(
        &self,
        entry: ActivityThread,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let title = Self::activity_title(entry);
        let source = Self::activity_source(entry);
        let selected = self.selected_activity_thread() == Some(entry);
        let hovered = self.activity_hovered(entry);
        let running = self.activity_running_thread == Some(entry);
        let show_actions = hovered;
        let title_viewport_width = activity_title_viewport_width(self.sidebar_width, show_actions);
        let title_width = Self::thread_title_width(title, window);
        let title_overflows = title_width > title_viewport_width;
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
        let pin = action_icon_button(format!("activity-pin-{entry:?}"), "pin", theme)
            .w(px(19.5))
            .h(px(20.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                match entry {
                    ActivityThread::Project(project, row) => {
                        this.pinned_threads[project][row] = !this.pinned_threads[project][row]
                    }
                    ActivityThread::Recent(index) => {
                        this.pinned_recents[index] = !this.pinned_recents[index]
                    }
                }
                cx.notify();
            }));
        let archive = action_icon_button(format!("activity-archive-{entry:?}"), "archive", theme)
            .w(px(19.5))
            .h(px(20.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                match entry {
                    ActivityThread::Project(project, row) => {
                        this.archived_threads[project][row] = true;
                        if this.selected_thread == Some((project, row)) {
                            this.selected_thread = None;
                        }
                    }
                    ActivityThread::Recent(index) => {
                        this.archived_recents[index] = true;
                        if this.selected_recent == Some(index) {
                            this.selected_recent = None;
                        }
                    }
                }
                if this.activity_running_thread == Some(entry) {
                    this.activity_running_thread = None;
                }
                cx.notify();
            }));
        let actions = div()
            .absolute()
            .right(px(5.0))
            .top(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .when(!show_actions, |actions| actions.invisible())
            .child(pin)
            .child(archive);
        let spinner = icon("dictation-spinner", theme.sidebar_icon_muted.into())
            .size(px(20.0))
            .with_animation(
                format!("activity-running-{entry:?}"),
                Animation::new(Duration::from_millis(800)).repeat(),
                |spinner, progress| {
                    spinner.with_transformation(Transformation::rotate(radians(
                        progress * std::f32::consts::TAU,
                    )))
                },
            );

        div()
            .id(format!("activity-thread-{entry:?}"))
            .h(px(ACTIVITY_ROW_HEIGHT))
            .min_h(px(ACTIVITY_ROW_HEIGHT))
            .pl(px(8.0))
            .pr(px(5.0))
            .pt(px(6.0))
            .pb(px(8.0))
            .relative()
            .flex()
            .items_center()
            .rounded(px(12.5))
            .cursor_pointer()
            .text_color(theme.sidebar_text)
            .hover(move |style| style.bg(theme.sidebar_hover))
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .self_stretch()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(2.0))
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .h(px(20.0))
                            .flex()
                            .when(show_actions, |title_row| {
                                title_row.pr(px(ACTIVITY_ACTION_TITLE_PADDING))
                            })
                            .child(activity_title_canvas(
                                title,
                                theme.sidebar_text.into(),
                                title_scroll_offset,
                                title_overflows,
                            )),
                    )
                    .child(
                        div()
                            .h(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(theme.sidebar_text_muted)
                            .child(icon("folder", theme.sidebar_icon_muted.into()).size(px(12.0)))
                            .child(source),
                    ),
            )
            .when(running && !show_actions, |row| {
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
            .child(actions)
            .on_hover(cx.listener(move |this, is_hovered: &bool, window, cx| {
                if *is_hovered {
                    match entry {
                        ActivityThread::Project(project, row) => {
                            this.hovered_thread = Some((project, row));
                            this.hovered_recent_thread = None;
                        }
                        ActivityThread::Recent(index) => {
                            this.hovered_recent_thread = Some(index);
                            this.hovered_thread = None;
                        }
                    }
                    this.start_marquee(
                        title,
                        activity_title_viewport_width(this.sidebar_width, true),
                        window,
                        cx,
                    );
                } else if this.activity_hovered(entry) {
                    this.hovered_thread = None;
                    this.hovered_recent_thread = None;
                    this.stop_marquee();
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                match entry {
                    ActivityThread::Project(project, row) => {
                        this.selected_project = project;
                        this.selected_thread = Some((project, row));
                        this.selected_recent = None;
                    }
                    ActivityThread::Recent(index) => {
                        this.selected_thread = None;
                        this.selected_recent = Some(index);
                    }
                }
                this.open_project_menu = None;
                cx.notify();
            }))
    }

    fn activity_scroll_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let priority = self.activity_priority_entries();
        let today = self.activity_today_entries(&priority);
        let yesterday = self.activity_yesterday_entries(&priority, &today);

        let mut content = div()
            .child(
                div()
                    .px(px(8.0))
                    .pb(px(21.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(nav_row("拉取请求", "pull-request", theme))
                    .child(nav_row("站点", "sites", theme))
                    .child(nav_row("已安排", "scheduled", theme))
                    .child(nav_row("插件", "plugins", theme)),
            )
            .child(
                div().px(px(8.0)).child(
                    div()
                        .relative()
                        .top(px(-4.0))
                        .flex()
                        .flex_col()
                        .gap(px(ACTIVITY_SECTION_GAP))
                        .child({
                            let mut section = div()
                                .flex()
                                .flex_col()
                                .gap(px(0.0))
                                .child(self.activity_section_heading("优先级", true, theme));
                            for (index, entry) in priority.into_iter().enumerate() {
                                if index > 0 {
                                    section =
                                        section.child(div().h(px(ACTIVITY_ROW_GAP)).flex_none());
                                }
                                section = section
                                    .child(self.activity_thread_row(entry, theme, window, cx));
                            }
                            section
                        })
                        .child({
                            let mut section = div()
                                .flex()
                                .flex_col()
                                .gap(px(0.0))
                                .child(self.activity_section_heading("今天", false, theme));
                            for (index, entry) in today.into_iter().enumerate() {
                                if index > 0 {
                                    section =
                                        section.child(div().h(px(ACTIVITY_ROW_GAP)).flex_none());
                                }
                                section = section
                                    .child(self.activity_thread_row(entry, theme, window, cx));
                            }
                            section
                        })
                        .child({
                            let mut section = div()
                                .flex()
                                .flex_col()
                                .gap(px(0.0))
                                .child(self.activity_section_heading("昨天", false, theme));
                            for (index, entry) in yesterday.into_iter().enumerate() {
                                if index > 0 {
                                    section =
                                        section.child(div().h(px(ACTIVITY_ROW_GAP)).flex_none());
                                }
                                section = section
                                    .child(self.activity_thread_row(entry, theme, window, cx));
                            }
                            section
                        }),
                ),
            );
        content = content.pb(px(13.0));
        content
    }

    fn activity_sticky_heading(&self) -> Option<(&'static str, f32)> {
        if !self.activity_open {
            return None;
        }
        let scroll = -f32::from(self.activity_scroll.offset().y);
        if scroll < ACTIVITY_NAV_BLOCK_HEIGHT {
            return None;
        }
        let priority_count = self.activity_priority_entries().len() as f32;
        let today_count = self
            .activity_today_entries(&self.activity_priority_entries())
            .len() as f32;
        let priority_height = activity_section_height(priority_count as usize);
        let today_start = priority_height + ACTIVITY_SECTION_GAP;
        let today_height = activity_section_height(today_count as usize);
        let yesterday_start = today_start + today_height + ACTIVITY_SECTION_GAP;
        let within = scroll - ACTIVITY_NAV_BLOCK_HEIGHT;
        let (label, next_start) = if within < today_start {
            ("优先级", Some(today_start))
        } else if within < yesterday_start {
            ("今天", Some(yesterday_start))
        } else {
            ("昨天", None)
        };
        let top = next_start
            .map(|next| (next - within - ACTIVITY_HEADING_HEIGHT).min(0.0))
            .unwrap_or(0.0);
        Some((label, top))
    }
}

fn scrollbar_geometry(
    track_height: f32,
    viewport_height: f32,
    max_offset: f32,
    current_offset: f32,
) -> (f32, f32) {
    let content_height = viewport_height + max_offset;
    let thumb_height = (track_height * viewport_height / content_height)
        .max(25.0)
        .min(track_height);
    let thumb_offset = if max_offset > 0.0 {
        current_offset / max_offset * (track_height - thumb_height)
    } else {
        0.0
    };
    (thumb_height, thumb_offset)
}

pub fn sidebar_scrollbar(
    scroll_handle: &ScrollHandle,
    theme: Theme,
    visible: bool,
) -> impl IntoElement {
    let scroll_handle = scroll_handle.clone();

    canvas(
        move |track_bounds, _, _| {
            if !visible {
                return None;
            }

            // The scrollable sibling is prepainted first, so these are the
            // final bounds and clamped offset for this exact frame.
            let viewport_height = f32::from(scroll_handle.bounds().size.height);
            let track_height = f32::from(track_bounds.size.height);
            let max_offset = f32::from(scroll_handle.max_offset().y).max(0.0);
            if viewport_height <= 0.0 || track_height <= 0.0 || max_offset <= 0.0 {
                return None;
            }

            let current_offset = -f32::from(scroll_handle.offset().y).clamp(-max_offset, 0.0);
            let (thumb_height, thumb_offset) =
                scrollbar_geometry(track_height, viewport_height, max_offset, current_offset);
            Some(Bounds {
                origin: point(
                    track_bounds.origin.x,
                    track_bounds.origin.y + px(thumb_offset),
                ),
                size: size(track_bounds.size.width, px(thumb_height)),
            })
        },
        move |_, thumb_bounds: Option<Bounds<Pixels>>, window, _| {
            if let Some(thumb_bounds) = thumb_bounds {
                window.paint_quad(quad(
                    thumb_bounds,
                    px(4.0),
                    theme.scrollbar_thumb,
                    px(0.0),
                    theme.scrollbar_thumb,
                    Default::default(),
                ));
            }
        },
    )
    .absolute()
    .top(px(3.0))
    .bottom(px(3.0))
    .right(px(4.0))
    .w(px(8.0))
}

impl SidebarView {
    fn sidebar(
        &self,
        theme: Theme,
        scroll_handle: &ScrollHandle,
        scrollbar_visible: bool,
        on_scroll: impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        // Put the custom listener on the content child. GPUI dispatches bubble
        // listeners child-first, so coarse Lines can be intercepted before the
        // parent scroller while precise Pixels continue into the native path.
        let scroll_content = if self.activity_open {
            self.activity_scroll_content(theme, window, cx)
        } else {
            self.native_scroll_content(theme, window, cx)
        }
        .w_full()
        // The reference uses a classic 15px scrollbar gutter. GPUI's
        // custom scrollbar overlays the content, so reserve the rounded
        // 16px gutter explicitly to keep every trailing rail aligned.
        .pr(px(16.0))
        .min_h_full()
        .on_scroll_wheel(on_scroll);

        let activity_button_fill = if self.activity_button_pressed {
            None
        } else if self.activity_open {
            Some(theme.accent.alpha(0.10))
        } else if self.activity_button_hovered {
            Some(theme.sidebar_hover)
        } else {
            None
        };
        let activity_icon_color = if self.activity_open {
            theme.accent
        } else {
            theme.sidebar_icon_muted
        };
        let activity_button = div()
            .id("sidebar-activity-button")
            .size(px(24.0))
            .flex_none()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when_some(activity_button_fill, |button, fill| button.bg(fill))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if this.activity_button_hovered != *hovered {
                    this.activity_button_hovered = *hovered;
                    if !*hovered {
                        this.activity_button_pressed = false;
                    }
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.activity_button_pressed = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.activity_button_pressed = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.activity_button_pressed = false;
                    cx.notify();
                }),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_activity_open(!this.activity_open, cx);
            }))
            .child(icon("activity", activity_icon_color.into()).size(px(16.0)));
        let sticky_heading = self.activity_sticky_heading();

        let resize_entity = cx.entity();
        let resize_line_visible = self.sidebar_resize_hovered || self.sidebar_resize_dragging;
        let resize_line_width = 1.0;
        let resize_line_color = if self.sidebar_resize_dragging {
            theme.sidebar_resize_active
        } else {
            theme.sidebar_resize_hover
        };
        let resize_handle = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let mouse_down_entity = resize_entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, _, _, cx| {
                    if event.button != MouseButton::Left || !bounds.contains(&event.position) {
                        return;
                    }
                    mouse_down_entity.update(cx, |this, cx| {
                        this.sidebar_resize_dragging = true;
                        this.sidebar_resize_hovered = true;
                        this.sidebar_resize_pointer_offset =
                            f32::from(event.position.x) - this.sidebar_width;
                        cx.notify();
                    });
                });

                let mouse_move_entity = resize_entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.sidebar_resize_dragging {
                            let limit =
                                sidebar_width_limit(f32::from(window.viewport_size().width));
                            let next_width = (f32::from(event.position.x)
                                - this.sidebar_resize_pointer_offset)
                                .clamp(SIDEBAR_MIN_WIDTH, limit);
                            if (this.sidebar_width - next_width).abs() > f32::EPSILON {
                                this.sidebar_width = next_width;
                                changed = true;
                            }
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

                let mouse_up_entity = resize_entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    mouse_up_entity.update(cx, |this, cx| {
                        if this.sidebar_resize_dragging {
                            this.sidebar_resize_dragging = false;
                            this.sidebar_resize_hovered = bounds.contains(&event.position);
                            cx.notify();
                        }
                    });
                });

                if resize_line_visible {
                    let line_bounds = Bounds {
                        origin: point(
                            bounds.origin.x + px(8.0 - resize_line_width * 0.5),
                            bounds.origin.y,
                        ),
                        size: size(px(resize_line_width), bounds.size.height),
                    };
                    window.paint_quad(quad(
                        line_bounds,
                        px(0.0),
                        resize_line_color,
                        px(0.0),
                        resize_line_color,
                        Default::default(),
                    ));
                }
            },
        )
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(-8.0))
        .w(px(16.0))
        .cursor_col_resize();

        div()
            .w(px(self.sidebar_width))
            .min_w(px(self.sidebar_width))
            .h_full()
            // This mirrors CSS's -apple-system stack. CoreText then resolves
            // Latin glyphs to SF and Chinese glyphs to PingFang SC.
            .font_family(".SystemUIFont")
            .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
            .pt(px(46.0))
            .relative()
            .flex()
            .flex_col()
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
                            .font_weight(SIDEBAR_TITLE_FONT_WEIGHT)
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
                            .child(sidebar_header_icon_button("search", theme))
                            .child(activity_button),
                    ),
            )
            .child(
                div()
                    .relative()
                    .top(px(1.0))
                    .px(px(8.0))
                    .h(px(31.0))
                    .child(nav_row("新对话", "new-chat", theme)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(
                        div()
                            .id("sidebar-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .restrict_scroll_to_axis()
                            .scrollbar_width(px(0.0))
                            .track_scroll(scroll_handle)
                            .child(scroll_content),
                    )
                    .when_some(sticky_heading, |container, (label, top)| {
                        container.child(
                            div()
                                .absolute()
                                .top(px(top))
                                .left(px(8.0))
                                .right(px(23.0))
                                .h(px(ACTIVITY_HEADING_HEIGHT))
                                .bg(theme.surface_under)
                                .child(self.activity_section_heading(
                                    label,
                                    label == "优先级",
                                    theme,
                                )),
                        )
                    })
                    .child(sidebar_scrollbar(scroll_handle, theme, scrollbar_visible)),
            )
            .child(
                div()
                    .h(px(46.0))
                    .px(px(8.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("profile-menu-button")
                            .h(px(30.0))
                            .min_w(px(0.0))
                            .flex_1()
                            .px(px(8.0))
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_pointer()
                            .when(self.profile_menu_open, move |button| {
                                button.bg(theme.sidebar_hover)
                            })
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.profile_menu_open = !this.profile_menu_open;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .size(px(20.0))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(theme.control)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(7.0))
                                    .text_color(theme.text)
                                    .child("RI"),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .font_family(".SystemUIFont")
                                    .text_size(px(PROFILE_TRIGGER_TEXT_SIZE))
                                    .line_height(px(PROFILE_TRIGGER_LINE_HEIGHT))
                                    .font_weight(SIDEBAR_BODY_FONT_WEIGHT)
                                    .text_color(theme.sidebar_text)
                                    .child("rita"),
                            ),
                    )
                    .child(
                        div()
                            .id("help-menu-button")
                            .size(px(32.0))
                            .flex_none()
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .child(icon("help", theme.sidebar_icon_muted.into()).size(px(20.0))),
                    ),
            )
            .when(self.profile_menu_open, |sidebar| {
                sidebar.child(deferred(profile_menu(theme, cx)))
            })
            .child(resize_handle)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext, Bounds, MouseButton, ScrollDelta, ScrollWheelEvent, TestApp, TestAppContext,
        TestAppWindow, TouchPhase, WindowBounds, WindowOptions, hsla, point, px, size,
    };

    use super::{
        ACTIVITY_NAV_BLOCK_HEIGHT, ACTIVITY_SECTION_GAP, ACTIVITY_TITLE_FADE_IN,
        ACTIVITY_TITLE_FADE_OUT, ActivityThread, MARQUEE_HOVER_DELAY,
        MOUSE_WHEEL_MAX_PENDING_LINES, PROJECTS, ProjectsLayout, ProjectsSort, RECENTS,
        SCROLLBAR_IDLE_DELAY, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
        SectionHeaderIcon, SidebarView, activity_section_height, activity_title_viewport_width,
        faded_sidebar_text_color, marquee_duration, marquee_offset, mouse_wheel_scroll_plan,
        scrollbar_geometry, sidebar_thread_title_viewport_width, sidebar_width_limit,
        smoothed_scroll_position,
    };
    use crate::theme::ThemeMode;

    fn test_window_options(height: f32) -> WindowOptions {
        test_window_options_with_size(320.0, height)
    }

    fn test_window_options_with_size(width: f32, height: f32) -> WindowOptions {
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(width), px(height)),
            })),
            ..Default::default()
        }
    }

    fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<SidebarView>) -> usize {
        app.advance_clock(Duration::from_millis(16));
        let handle = window.handle();
        app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        })
    }

    fn drain_mouse_animation(app: &mut TestApp, window: &TestAppWindow<SidebarView>) {
        for _ in 0..120 {
            if !window.read(|sidebar, _| sidebar.mouse_wheel_animation_running) {
                return;
            }
            assert_eq!(simulate_next_frame(app, window), 1);
        }
        panic!("mouse-wheel animation did not settle");
    }

    #[test]
    fn scrollbar_tracks_the_real_scroll_range() {
        let viewport = 739.0;
        let track = viewport - 6.0;
        let content_height = 1558.0;
        let max_offset = content_height - viewport;
        let (thumb_height, top) = scrollbar_geometry(track, viewport, max_offset, 0.0);
        let (_, middle) = scrollbar_geometry(track, viewport, max_offset, max_offset / 2.0);
        let (_, bottom) = scrollbar_geometry(track, viewport, max_offset, max_offset);
        let travel = track - thumb_height;
        let expected_height = track * viewport / (viewport + max_offset);

        assert!((thumb_height - expected_height).abs() < 0.001);
        assert!((top - 0.0).abs() < 0.001);
        assert!((middle - travel / 2.0).abs() < 0.001);
        assert!((bottom - travel).abs() < 0.001);
    }

    #[test]
    fn marquee_matches_the_reference_delay_speed_and_stop_position() {
        let distance = 19.234_375;
        let duration = marquee_duration(distance);

        assert!((duration.as_secs_f32() - 1.037).abs() < 0.001);
        assert_eq!(marquee_offset(distance, MARQUEE_HOVER_DELAY, false), 0.0);
        assert_eq!(marquee_offset(distance, duration, false), distance);
        assert_eq!(
            marquee_offset(distance, duration + Duration::from_secs(1), false),
            distance
        );
    }

    #[test]
    fn marquee_only_moves_overflowing_text_and_honors_reduced_motion() {
        assert_eq!(marquee_offset(0.0, Duration::from_secs(10), false), 0.0);
        assert_eq!(marquee_offset(120.0, Duration::from_secs(10), true), 0.0);
    }

    #[test]
    fn canvas_thread_titles_preserve_the_sidebar_foreground_alpha() {
        let foreground = hsla(0.0, 0.0, 0.9, 0.85);

        assert!((faded_sidebar_text_color(foreground, 1.0).a - 0.85).abs() < f32::EPSILON);
        assert!((faded_sidebar_text_color(foreground, 0.5).a - 0.425).abs() < f32::EPSILON);
    }

    #[test]
    fn scrollbar_uses_the_measured_viewport_height() {
        let content_height = 1558.0;
        let small_max = content_height - 500.0;
        let large_max = content_height - 900.0;
        let small_track = 500.0 - 6.0;
        let large_track = 900.0 - 6.0;
        let (small_thumb, small_bottom) =
            scrollbar_geometry(small_track, 500.0, small_max, small_max);
        let (large_thumb, large_bottom) =
            scrollbar_geometry(large_track, 900.0, large_max, large_max);

        assert!(small_thumb < large_thumb);
        assert!((small_bottom - (small_track - small_thumb)).abs() < 0.001);
        assert!((large_bottom - (large_track - large_thumb)).abs() < 0.001);
    }

    #[test]
    fn mouse_wheel_easing_is_frame_rate_independent_and_never_overshoots() {
        let current = -120.0;
        let target = -240.0;
        let next = smoothed_scroll_position(current, target, Duration::from_millis(16));
        assert!(next < current && next > target);

        let one_frame = smoothed_scroll_position(0.0, -100.0, Duration::from_millis(16));
        let first_half = smoothed_scroll_position(0.0, -100.0, Duration::from_millis(8));
        let two_half_frames =
            smoothed_scroll_position(first_half, -100.0, Duration::from_millis(8));
        assert!((one_frame - two_half_frames).abs() < 0.001);
    }

    #[test]
    fn mouse_wheel_reverses_from_the_visible_position() {
        let plan = mouse_wheel_scroll_plan(-80.0, Some(-180.0), 30.0, 800.0, 20.0);
        assert!(plan.reversed);
        assert_eq!(plan.immediate_offset, -80.0);
        assert_eq!(plan.target_offset, -50.0);
    }

    #[test]
    fn fast_mouse_wheel_bursts_preserve_distance_but_bound_the_tail() {
        let line_height = 20.0;
        let plan = mouse_wheel_scroll_plan(0.0, Some(-160.0), -60.0, 800.0, line_height);

        assert_eq!(plan.target_offset, -220.0);
        assert_eq!(
            plan.target_offset - plan.immediate_offset,
            -line_height * MOUSE_WHEEL_MAX_PENDING_LINES
        );
    }

    #[test]
    fn project_section_and_project_rows_toggle_from_their_full_hit_areas() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_click(point(px(80.0), px(267.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.projects_collapsed));

        window.draw();
        window.simulate_click(point(px(80.0), px(267.0)), MouseButton::Left);
        assert!(!window.read(|sidebar, _| sidebar.projects_collapsed));

        window.draw();
        assert!(!window.read(|sidebar, _| sidebar.project_collapsed[0]));
        window.simulate_click(point(px(80.0), px(299.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.project_collapsed[0]));
    }

    #[test]
    fn sidebar_width_limit_matches_the_reference_clamp() {
        assert_eq!(sidebar_width_limit(500.0), SIDEBAR_MIN_WIDTH);
        assert_eq!(sidebar_width_limit(600.0), 280.0);
        assert_eq!(sidebar_width_limit(1_200.0), SIDEBAR_MAX_WIDTH);
    }

    #[test]
    fn sidebar_resize_handle_clamps_dragging_to_both_limits() {
        let mut app = TestApp::new();
        let mut window = app
            .open_window_with_options(test_window_options_with_size(900.0, 900.0), |_, _| {
                SidebarView::new(ThemeMode::Dark, false)
            });
        window.draw();

        window.simulate_mouse_down(point(px(256.0), px(100.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(800.0), px(100.0)));
        window.simulate_mouse_up(point(px(800.0), px(100.0)), MouseButton::Left);
        assert_eq!(
            window.read(|sidebar, _| sidebar.sidebar_width),
            SIDEBAR_MAX_WIDTH
        );
        assert!(!window.read(|sidebar, _| sidebar.sidebar_resize_dragging));

        window.draw();
        window.simulate_mouse_down(point(px(520.0), px(100.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(80.0), px(100.0)));
        window.simulate_mouse_up(point(px(80.0), px(100.0)), MouseButton::Left);
        assert_eq!(
            window.read(|sidebar, _| sidebar.sidebar_width),
            SIDEBAR_MIN_WIDTH
        );

        let mut small_app = TestApp::new();
        let mut small_window = small_app
            .open_window_with_options(test_window_options_with_size(600.0, 900.0), |_, _| {
                SidebarView::new(ThemeMode::Dark, false)
            });
        small_window.draw();
        small_window.simulate_mouse_down(point(px(256.0), px(100.0)), MouseButton::Left);
        small_window.simulate_mouse_move(point(px(500.0), px(100.0)));
        small_window.simulate_mouse_up(point(px(500.0), px(100.0)), MouseButton::Left);
        assert_eq!(small_window.read(|sidebar, _| sidebar.sidebar_width), 280.0);
    }

    #[test]
    fn collapsing_projects_keeps_the_recent_threads_in_the_scroll_content() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(600.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_click(point(px(80.0), px(267.0)), MouseButton::Left);
        window.draw();

        assert!(window.read(|sidebar, _| sidebar.projects_collapsed));
        assert!(window.read(|sidebar, _| f32::from(sidebar.scroll.max_offset().y) > 0.0));
    }

    #[test]
    fn project_heading_hover_explicitly_reveals_and_hides_its_icons() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_mouse_move(point(px(80.0), px(267.0)));
        assert!(window.read(|sidebar, _| sidebar.projects_heading_hovered));

        window.draw();
        window.simulate_mouse_move(point(px(300.0), px(267.0)));
        assert!(!window.read(|sidebar, _| sidebar.projects_heading_hovered));
    }

    #[test]
    fn project_and_thread_action_rails_match_reference_at_small_and_full_widths() {
        for width in [600.0, 1512.0] {
            let x_shift = (SIDEBAR_DEFAULT_WIDTH - sidebar_width_limit(width)).max(0.0);
            let mut app = TestApp::new();
            let mut window = app
                .open_window_with_options(test_window_options_with_size(width, 900.0), |_, _| {
                    SidebarView::new(ThemeMode::Dark, false)
                });
            window.draw();

            // Reference centers: section menu 191, add 219; project menu
            // 185, new-chat 215; thread pin 188.5, archive 215.5.
            window.simulate_mouse_move(point(px(191.0 - x_shift), px(267.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_section_icon),
                Some(SectionHeaderIcon::Menu)
            );
            window.simulate_mouse_move(point(px(219.0 - x_shift), px(267.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_section_icon),
                Some(SectionHeaderIcon::Add)
            );

            window.simulate_mouse_move(point(px(100.0), px(299.0)));
            window.draw();
            window.simulate_click(point(px(185.0 - x_shift), px(299.0)), MouseButton::Left);
            assert_eq!(window.read(|sidebar, _| sidebar.open_project_menu), Some(0));
            window.draw();
            window.simulate_click(point(px(185.0 - x_shift), px(299.0)), MouseButton::Left);
            assert_eq!(window.read(|sidebar, _| sidebar.open_project_menu), None);

            window.draw();
            window.simulate_click(point(px(215.0 - x_shift), px(299.0)), MouseButton::Left);
            assert_eq!(window.read(|sidebar, _| sidebar.selected_project), 0);

            window.simulate_mouse_move(point(px(100.0), px(331.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_thread),
                Some((0, 0))
            );
            window.draw();
            window.simulate_click(point(px(188.5 - x_shift), px(331.0)), MouseButton::Left);
            assert!(window.read(|sidebar, _| sidebar.pinned_threads[0][0]));
            window.draw();
            // Pinning moves the real thread into the top-level pinned section.
            window.simulate_click(point(px(215.5 - x_shift), px(295.0)), MouseButton::Left);
            assert!(window.read(|sidebar, _| sidebar.archived_threads[0][0]));
        }
    }

    #[test]
    fn project_menu_matches_native_toggle_keyboard_and_selection_behavior() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_mouse_move(point(px(100.0), px(299.0)));
        window.draw();
        window.simulate_click(point(px(185.0), px(299.0)), MouseButton::Left);
        assert_eq!(window.read(|sidebar, _| sidebar.open_project_menu), Some(0));

        window.draw();
        window.simulate_keystroke("down");
        assert_eq!(
            window.read(|sidebar, _| sidebar.project_menu_focused_item),
            Some(0)
        );
        window.simulate_keystroke("down");
        assert_eq!(
            window.read(|sidebar, _| sidebar.project_menu_focused_item),
            Some(1)
        );
        window.simulate_keystroke("end");
        assert_eq!(
            window.read(|sidebar, _| sidebar.project_menu_focused_item),
            Some(5)
        );
        window.simulate_keystroke("home");
        window.simulate_keystroke("enter");
        assert!(window.read(|sidebar, _| sidebar.pinned_projects[0]));
        assert_eq!(window.read(|sidebar, _| sidebar.open_project_menu), None);

        window.draw();
        window.simulate_click(point(px(185.0), px(299.0)), MouseButton::Left);
        window.draw();
        window.simulate_keystroke("escape");
        assert_eq!(window.read(|sidebar, _| sidebar.open_project_menu), None);
    }

    #[test]
    fn projects_section_menu_matches_radix_toggle_and_keyboard_behavior() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_mouse_move(point(px(191.0), px(267.0)));
        window.draw();
        window.simulate_click(point(px(191.0), px(267.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.projects_section_menu_open));
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_menu_focused_item),
            None
        );

        window.draw();
        window.simulate_keystroke("down");
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_menu_focused_item),
            Some(0)
        );
        window.simulate_keystroke("down");
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_menu_focused_item),
            Some(1)
        );
        window.simulate_keystroke("end");
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_menu_focused_item),
            Some(4)
        );
        window.simulate_keystroke("home");
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_menu_focused_item),
            Some(0)
        );
        window.simulate_keystroke("down");
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_layout),
            ProjectsLayout::Flat
        );
        assert!(!window.read(|sidebar, _| sidebar.projects_section_menu_open));

        window.draw();
        window.simulate_click(point(px(191.0), px(267.0)), MouseButton::Left);
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!window.read(|sidebar, _| sidebar.projects_section_menu_open));
        assert_eq!(
            window.read(|sidebar, _| sidebar.projects_sort),
            ProjectsSort::Priority
        );
    }

    #[test]
    fn pinning_moves_the_thread_and_pinned_menu_matches_radix_keyboard_behavior() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_mouse_move(point(px(100.0), px(331.0)));
        window.draw();
        window.simulate_click(point(px(188.5), px(331.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.pinned_threads[0][0]));
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_entries()),
            [ActivityThread::Project(0, 0)]
        );

        window.draw();
        window.simulate_mouse_move(point(px(219.0), px(267.0)));
        window.draw();
        window.simulate_click(point(px(219.0), px(267.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.pinned_menu_open));
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_menu_focused_item),
            None
        );

        window.simulate_keystroke("down");
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_menu_focused_item),
            Some(0)
        );
        window.simulate_keystroke("end");
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_menu_focused_item),
            Some(2)
        );
        window.simulate_keystroke("up");
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_menu_focused_item),
            Some(1)
        );
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|sidebar, _| sidebar.pinned_sort),
            ProjectsSort::Recent
        );
        assert!(!window.read(|sidebar, _| sidebar.pinned_menu_open));

        window.draw();
        window.simulate_click(point(px(219.0), px(267.0)), MouseButton::Left);
        window.simulate_keystroke("escape");
        assert!(!window.read(|sidebar, _| sidebar.pinned_menu_open));
    }

    #[test]
    fn selected_local_folder_is_reused_as_project_data_without_examples() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });

        window.update(|sidebar, _, cx| {
            sidebar.add_local_project(std::path::Path::new("/tmp/real-worktree"), cx)
        });
        assert_eq!(
            window.read(|sidebar, _| sidebar.created_projects.clone()),
            ["real-worktree".to_owned()]
        );
        assert_eq!(
            window.read(|sidebar, _| sidebar.selected_created_project),
            Some(0)
        );

        window.update(|sidebar, _, cx| {
            sidebar.add_local_project(std::path::Path::new("/tmp/real-worktree"), cx)
        });
        assert_eq!(window.read(|sidebar, _| sidebar.created_projects.len()), 1);
    }

    #[test]
    fn recent_heading_and_thread_actions_match_reference_at_small_and_full_widths() {
        for width in [600.0, 1512.0] {
            let x_shift = (SIDEBAR_DEFAULT_WIDTH - sidebar_width_limit(width)).max(0.0);
            let mut app = TestApp::new();
            let mut window = app
                .open_window_with_options(test_window_options_with_size(width, 900.0), |_, _| {
                    SidebarView::new(ThemeMode::Dark, false)
                });
            window.draw();

            // Collapse only the project tree so the independent recents
            // section occupies its stable reference position.
            window.simulate_click(point(px(80.0), px(267.0)), MouseButton::Left);
            window.draw();

            window.simulate_mouse_move(point(px(191.0 - x_shift), px(302.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_recents_icon),
                Some(SectionHeaderIcon::Menu)
            );
            window.simulate_mouse_move(point(px(219.0 - x_shift), px(302.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_recents_icon),
                Some(SectionHeaderIcon::NewChat)
            );

            window.simulate_mouse_move(point(px(100.0), px(330.0)));
            assert_eq!(
                window.read(|sidebar, _| sidebar.hovered_recent_thread),
                Some(0)
            );
            window.draw();
            window.simulate_click(point(px(188.5 - x_shift), px(330.0)), MouseButton::Left);
            assert!(window.read(|sidebar, _| sidebar.pinned_recents[0]));
            window.draw();
            window.simulate_click(point(px(215.5 - x_shift), px(295.0)), MouseButton::Left);
            assert!(window.read(|sidebar, _| sidebar.archived_recents[0]));

            window.draw();
            window.simulate_click(point(px(80.0), px(302.0)), MouseButton::Left);
            assert!(window.read(|sidebar, _| sidebar.recents_collapsed));
        }
    }

    #[test]
    fn profile_button_toggles_the_reference_menu() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(700.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        let trigger_center = point(px(80.0), px(677.0));
        assert!(!window.read(|sidebar, _| sidebar.profile_menu_open));
        window.simulate_click(trigger_center, MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.profile_menu_open));

        window.draw();
        window.simulate_click(trigger_center, MouseButton::Left);
        assert!(!window.read(|sidebar, _| sidebar.profile_menu_open));
    }

    #[gpui::test]
    fn scrollbar_uses_one_trailing_idle_window(cx: &mut TestAppContext) {
        let sidebar = cx.new(|_| SidebarView::new(ThemeMode::Dark, false));
        sidebar.update(cx, |sidebar, cx| sidebar.reveal_scrollbar(cx));
        cx.run_until_parked();
        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.scrollbar_visible));
        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.scrollbar_hide_loop_running));

        cx.executor().advance_clock(Duration::from_millis(400));
        sidebar.update(cx, |sidebar, cx| sidebar.reveal_scrollbar(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        assert!(sidebar.read_with(cx, |sidebar, _| sidebar.scrollbar_visible));

        cx.executor()
            .advance_clock(SCROLLBAR_IDLE_DELAY - Duration::from_millis(400));
        cx.run_until_parked();
        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.scrollbar_visible));
        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.scrollbar_hide_loop_running));
    }

    #[test]
    fn line_events_are_intercepted_before_gpui_native_scrolling() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(600.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        let line_height = window.update(|_, window, _| f32::from(window.line_height()));
        let (initial, max_offset) = window.read(|sidebar, _| {
            (
                f32::from(sidebar.scroll.offset().y),
                f32::from(sidebar.scroll.max_offset().y),
            )
        });
        assert!(max_offset > line_height * 3.0);
        let expected = initial - line_height * 3.0;

        window.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Lines(point(0.0, -3.0)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });

        window.read(|sidebar, _| {
            assert_eq!(f32::from(sidebar.scroll.offset().y), initial);
            assert!((sidebar.mouse_wheel_target.expect("mouse target") - expected).abs() < 0.01);
            assert!(sidebar.mouse_wheel_animation_running);
        });

        drain_mouse_animation(&mut app, &window);
        window.read(|sidebar, _| {
            assert!((f32::from(sidebar.scroll.offset().y) - expected).abs() < 0.01);
            assert!(sidebar.mouse_wheel_target.is_none());
            assert!(!sidebar.mouse_wheel_animation_running);
        });
    }

    #[test]
    fn precise_pixel_events_stay_native_and_cancel_mouse_animation() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(600.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Lines(point(0.0, -3.0)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        assert!(window.read(|sidebar, _| sidebar.mouse_wheel_animation_running));

        let before_pixels = window.read(|sidebar, _| f32::from(sidebar.scroll.offset().y));

        window.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Pixels(point(px(0.0), px(-7.25))),
            touch_phase: TouchPhase::Started,
            ..Default::default()
        });
        window.read(|sidebar, _| {
            assert!((f32::from(sidebar.scroll.offset().y) - (before_pixels - 7.25)).abs() < 0.01);
            assert!(sidebar.mouse_wheel_target.is_none());
            assert!(!sidebar.mouse_wheel_animation_running);
        });

        // The already queued mouse callback becomes a no-op after the precise
        // device takes ownership.
        let callback_count = simulate_next_frame(&mut app, &window);
        assert_eq!(callback_count, 1);
        assert!(
            (window.read(|sidebar, _| f32::from(sidebar.scroll.offset().y))
                - (before_pixels - 7.25))
                .abs()
                < 0.01
        );
    }

    #[test]
    fn precise_trackpad_gestures_keep_gpui_axis_locking() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(600.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        let event = |x: f32, y: f32, touch_phase| ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase,
            ..Default::default()
        };

        window.simulate_event(event(-20.0, -2.0, TouchPhase::Started));
        assert_eq!(window.read(|sidebar, _| sidebar.scroll.offset().y), px(0.0));
        window.simulate_event(event(-1.0, -5.0, TouchPhase::Moved));
        assert_eq!(window.read(|sidebar, _| sidebar.scroll.offset().y), px(0.0));
        window.simulate_event(event(-1.0, -10.0, TouchPhase::Moved));
        assert_eq!(
            window.read(|sidebar, _| sidebar.scroll.offset().y),
            px(-10.0)
        );
        window.simulate_event(event(0.0, 0.0, TouchPhase::Ended));
    }

    #[test]
    fn reduced_motion_applies_one_mouse_step_immediately() {
        let mut app = TestApp::new();
        app.update(|cx| cx.set_reduce_motion(true));
        let mut window = app.open_window_with_options(test_window_options(600.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();
        let line_height = window.update(|_, window, _| f32::from(window.line_height()));

        window.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Lines(point(0.0, -3.0)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        window.read(|sidebar, _| {
            assert!((f32::from(sidebar.scroll.offset().y) + line_height * 3.0).abs() < 0.01);
            assert!(sidebar.mouse_wheel_target.is_none());
            assert!(!sidebar.mouse_wheel_animation_running);
        });
    }

    #[gpui::test]
    fn mouse_animation_reclamps_after_viewport_growth(cx: &mut TestAppContext) {
        let (sidebar, cx) = cx.add_window_view(|_, _| SidebarView::new(ThemeMode::Dark, false));
        cx.simulate_resize(size(px(320.0), px(500.0)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let old_max = sidebar.read_with(cx, |sidebar, _| f32::from(sidebar.scroll.max_offset().y));
        assert!(old_max > 0.0);

        cx.simulate_event(ScrollWheelEvent {
            position: point(px(100.0), px(200.0)),
            delta: ScrollDelta::Lines(point(0.0, -1_000.0)),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        assert_eq!(
            sidebar.read_with(cx, |sidebar, _| sidebar.mouse_wheel_target),
            Some(-old_max)
        );

        cx.simulate_resize(size(px(320.0), px(1_300.0)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let new_max = sidebar.read_with(cx, |sidebar, _| f32::from(sidebar.scroll.max_offset().y));
        assert!(
            new_max >= 0.0 && new_max < old_max,
            "viewport growth must shrink the range: old={old_max}, new={new_max}"
        );

        for _ in 0..120 {
            if !sidebar.read_with(cx, |sidebar, _| sidebar.mouse_wheel_animation_running) {
                break;
            }
            cx.executor().advance_clock(Duration::from_millis(16));
            assert_eq!(cx.update(|window, cx| window.simulate_next_frame(cx)), 1);
            cx.run_until_parked();
        }
        sidebar.read_with(cx, |sidebar, _| {
            assert!((f32::from(sidebar.scroll.offset().y) + new_max).abs() < 0.01);
            assert!(sidebar.mouse_wheel_target.is_none());
            assert!(!sidebar.mouse_wheel_animation_running);
        });
    }

    #[test]
    fn activity_reorganizes_the_existing_thread_collections() {
        let mut sidebar = SidebarView::new(ThemeMode::Dark, false);
        sidebar.activity_running_thread = sidebar.fallback_activity_thread();
        let priority = sidebar.activity_priority_entries();
        let today = sidebar.activity_today_entries(&priority);
        let yesterday = sidebar.activity_yesterday_entries(&priority, &today);
        let expected_project_threads = PROJECTS
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .filter(|title| **title != "展开显示")
            .count();

        assert_eq!(
            priority.len() + today.len() + yesterday.len(),
            expected_project_threads + RECENTS.len()
        );
        assert!(
            priority
                .iter()
                .all(|entry| matches!(entry, ActivityThread::Project(_, _)))
        );
        assert_eq!(
            yesterday
                .iter()
                .filter(|entry| matches!(entry, ActivityThread::Recent(_)))
                .count(),
            RECENTS.len()
        );
    }

    #[test]
    fn activity_hover_reserves_the_reference_action_rail_from_the_title() {
        assert_eq!(activity_title_viewport_width(256.125, false), 212.125);
        assert_eq!(activity_title_viewport_width(256.125, true), 156.125);
        assert_eq!(
            activity_title_viewport_width(256.125, false)
                - activity_title_viewport_width(256.125, true),
            56.0
        );
        assert_eq!(ACTIVITY_TITLE_FADE_IN, 8.0);
        assert_eq!(ACTIVITY_TITLE_FADE_OUT, 16.0);
    }

    #[test]
    fn normal_sidebar_titles_use_the_full_width_until_actions_appear() {
        assert_eq!(
            sidebar_thread_title_viewport_width(256.125, false, false),
            187.125
        );
        assert_eq!(
            sidebar_thread_title_viewport_width(256.125, false, true),
            136.125
        );
        assert_eq!(
            sidebar_thread_title_viewport_width(256.125, true, false),
            211.125
        );
        assert_eq!(
            sidebar_thread_title_viewport_width(256.125, true, true),
            160.125
        );
    }

    #[gpui::test]
    fn activity_toggle_keeps_the_project_scroll_position(cx: &mut TestAppContext) {
        let sidebar = cx.new(|_| SidebarView::new(ThemeMode::Dark, false));
        sidebar.update(cx, |sidebar, cx| {
            sidebar.scroll.set_offset(point(px(0.0), px(-75.0)));
            sidebar.set_activity_open(true, cx);
            assert!(sidebar.activity_open);
            assert_eq!(sidebar.activity_scroll.offset().y, px(0.0));
            sidebar
                .activity_scroll
                .set_offset(point(px(0.0), px(-120.0)));
            sidebar.set_activity_open(false, cx);
            assert!(!sidebar.activity_open);
            assert_eq!(sidebar.scroll.offset().y, px(-75.0));
        });
    }

    #[test]
    fn activity_button_press_toggle_and_thread_jump_are_wired() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(test_window_options(900.0), |_, _| {
            SidebarView::new(ThemeMode::Dark, false)
        });
        window.draw();

        window.simulate_mouse_move(point(px(228.0), px(64.0)));
        assert!(window.read(|sidebar, _| sidebar.activity_button_hovered));
        window.simulate_mouse_down(point(px(228.0), px(64.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.activity_button_pressed));
        window.simulate_mouse_up(point(px(228.0), px(64.0)), MouseButton::Left);
        assert!(window.read(|sidebar, _| sidebar.activity_open));
        assert!(!window.read(|sidebar, _| sidebar.activity_button_pressed));

        window.draw();
        window.simulate_click(point(px(100.0), px(412.0)), MouseButton::Left);
        assert_eq!(
            window.read(|sidebar, _| sidebar.selected_thread),
            Some((0, 0))
        );
        assert_eq!(window.read(|sidebar, _| sidebar.selected_recent), None);

        window.simulate_click(point(px(228.0), px(64.0)), MouseButton::Left);
        assert!(!window.read(|sidebar, _| sidebar.activity_open));
        assert_eq!(
            window.read(|sidebar, _| sidebar.selected_thread),
            Some((0, 0))
        );
    }

    #[test]
    fn activity_group_titles_stick_and_push_at_measured_boundaries() {
        let mut sidebar = SidebarView::new(ThemeMode::Dark, false);
        sidebar.activity_open = true;
        sidebar.activity_running_thread = sidebar.fallback_activity_thread();
        sidebar
            .activity_scroll
            .set_offset(point(px(0.0), px(-150.0)));
        assert_eq!(sidebar.activity_sticky_heading(), Some(("优先级", 0.0)));

        let priority_end = ACTIVITY_NAV_BLOCK_HEIGHT
            + activity_section_height(sidebar.activity_priority_entries().len())
            + ACTIVITY_SECTION_GAP;
        sidebar
            .activity_scroll
            .set_offset(point(px(0.0), px(-(priority_end + 1.0))));
        assert_eq!(
            sidebar.activity_sticky_heading().map(|value| value.0),
            Some("今天")
        );
    }
}
