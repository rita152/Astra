use super::*;
use crate::agent::{
    AgentConnectionEvent, AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile,
    AgentRequest, AgentRun, AgentThreadSettings, SideConversationRequest, WorkspaceResult,
};
use gpui::prelude::*;
use std::sync::Mutex;

#[derive(Default)]
struct Backend {
    openings: Mutex<
        Vec<(
            SideConversationRequest,
            async_channel::Sender<WorkspaceResult<ThreadId>>,
        )>,
    >,
    closed: Mutex<Vec<ThreadId>>,
    listeners: Mutex<Vec<async_channel::Sender<AgentConnectionEvent>>>,
}
impl AgentBackend for Backend {
    fn subscribe_connection_events(&self) -> async_channel::Receiver<AgentConnectionEvent> {
        let (sender, receiver) = async_channel::unbounded();
        self.listeners.lock().unwrap().push(sender);
        receiver
    }
    fn load_model_catalog(&self) -> async_channel::Receiver<Result<AgentModelCatalog, String>> {
        async_channel::bounded(1).1
    }
    fn load_permission_profiles(
        &self,
        _: PathBuf,
    ) -> async_channel::Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        async_channel::bounded(1).1
    }
    fn update_thread_permissions(
        &self,
        _: String,
        _: PathBuf,
        _: AgentPermissionMode,
    ) -> async_channel::Receiver<Result<AgentThreadSettings, String>> {
        async_channel::bounded(1).1
    }
    fn run_prompt(&self, _: AgentRequest) -> AgentRun {
        panic!("UI lifetime tests must not send a model request")
    }
    fn open_side_conversation(
        &self,
        request: SideConversationRequest,
    ) -> async_channel::Receiver<WorkspaceResult<ThreadId>> {
        let (sender, receiver) = async_channel::bounded(1);
        self.openings.lock().unwrap().push((request, sender));
        receiver
    }
    fn close_side_conversation(
        &self,
        id: ThreadId,
    ) -> async_channel::Receiver<WorkspaceResult<()>> {
        self.closed.lock().unwrap().push(id);
        let (sender, receiver) = async_channel::bounded(1);
        sender.send_blocking(Ok(())).unwrap();
        receiver
    }
}

fn setup() -> (gpui::TestApp, Arc<Backend>, Entity<SideChatPanel>) {
    let mut app = gpui::TestApp::new();
    let backend = Arc::new(Backend::default());
    let parent =
        app.new_entity(|cx| ComposerView::new_with_backend(ThemeMode::Dark, backend.clone(), cx));
    app.update_entity(&parent, |parent, cx| {
        parent.set_workspace_context("/tmp/side-parent".into(), None, Some("parent".into()), cx)
    });
    let panel = app
        .new_entity(|cx| SideChatPanel::new(parent, backend.clone(), ThemeMode::Dark, false, cx));
    (app, backend, panel)
}
fn complete_open(app: &mut gpui::TestApp, backend: &Backend, index: usize, id: &str) {
    backend.openings.lock().unwrap()[index]
        .1
        .send_blocking(Ok(id.into()))
        .unwrap();
    app.run_until_parked();
}

#[test]
fn closing_a_loading_side_tab_cleans_up_a_late_fork_result() {
    let (mut app, backend, panel) = setup();
    app.update_entity(&panel, |panel, cx| panel.new_chat(cx));
    app.update_entity(&panel, |panel, cx| panel.request_close(1, cx));
    assert!(app.read_entity(&panel, |panel, _| panel.is_empty()));
    complete_open(&mut app, &backend, 0, "late-side");
    assert_eq!(*backend.closed.lock().unwrap(), vec!["late-side"]);
    assert_eq!(
        backend.openings.lock().unwrap()[0].0.parent_thread_id,
        "parent"
    );
}

#[test]
fn side_tabs_preserve_messages_across_switches_and_cancelled_close() {
    let (mut app, backend, panel) = setup();
    app.update_entity(&panel, |panel, cx| panel.new_chat(cx));
    complete_open(&mut app, &backend, 0, "side-a");
    app.update_entity(&panel, |panel, cx| {
        panel.tabs[0].composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(false, cx)
        });
        panel.new_chat(cx);
    });
    complete_open(&mut app, &backend, 1, "side-b");
    app.update_entity(&panel, |panel, cx| {
        panel.set_visible(false, cx);
        panel.set_visible(true, cx);
        panel.activate(1, cx);
        panel.request_close(1, cx);
        assert_eq!(panel.close_confirmation, Some(1));
        assert!(panel.dismiss_transient(cx));
        assert_eq!(panel.tabs.len(), 2);
        assert!(panel.tabs[0].composer.read(cx).has_messages());
        assert!(!panel.parent.read(cx).has_messages());
    });
    assert!(backend.closed.lock().unwrap().is_empty());
    app.update_entity(&panel, |panel, cx| {
        panel.request_close(1, cx);
        panel.confirm_close(cx);
    });
    assert_eq!(*backend.closed.lock().unwrap(), vec!["side-a"]);
    assert_eq!(app.read_entity(&panel, |panel, _| panel.active), Some(2));
    assert!(
        backend
            .openings
            .lock()
            .unwrap()
            .iter()
            .all(|(request, _)| request.parent_thread_id == "parent")
    );
}

#[test]
fn side_thread_expiration_retains_readable_content_and_other_tabs() {
    let (mut app, backend, panel) = setup();
    app.update_entity(&panel, |panel, cx| panel.new_chat(cx));
    complete_open(&mut app, &backend, 0, "side-a");
    app.update_entity(&panel, |panel, cx| {
        panel.tabs[0].composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(false, cx)
        });
        panel.new_chat(cx);
    });
    complete_open(&mut app, &backend, 1, "side-b");
    for sender in backend.listeners.lock().unwrap().iter() {
        sender
            .send_blocking(AgentConnectionEvent::ThreadClosed {
                thread_id: "side-a".into(),
            })
            .unwrap();
    }
    app.run_until_parked();
    app.read_entity(&panel, |panel, cx| {
        assert!(panel.tabs[0].error.is_some());
        assert!(panel.tabs[0].composer.read(cx).has_messages());
        assert!(panel.tabs[1].error.is_none());
        assert_eq!(panel.active, Some(2));
    });
}

struct DialogHost(Entity<SideChatPanel>);
impl gpui::Render for DialogHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay = self
            .0
            .update(cx, |panel, cx| panel.render_overlay(window, cx));
        gpui::div()
            .size_full()
            .relative()
            .child(self.0.clone())
            .when_some(overlay, |root, overlay| root.child(overlay))
    }
}

#[test]
fn side_close_dialog_keyboard_is_scoped_and_cancellation_keeps_the_chat() {
    let (mut app, backend, panel) = setup();
    app.update(super::init);
    app.update_entity(&panel, |panel, cx| panel.new_chat(cx));
    complete_open(&mut app, &backend, 0, "side-a");
    app.update_entity(&panel, |panel, cx| {
        panel.tabs[0].composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(false, cx)
        });
        panel.request_close(1, cx);
    });
    let mut window = app.open_window(|_, _| DialogHost(panel.clone()));
    window.draw();
    window.simulate_keystrokes("tab space");
    assert!(app.read_entity(&panel, |panel, _| panel.remember_close));
    window.simulate_keystrokes("tab enter");
    app.read_entity(&panel, |panel, _| {
        assert!(panel.close_confirmation.is_none());
        assert_eq!(panel.tabs.len(), 1);
        assert!(!panel.skip_confirmation);
    });
    app.update_entity(&panel, |panel, cx| panel.request_close(1, cx));
    window.draw();
    window.simulate_keystrokes("escape");
    assert!(app.read_entity(&panel, |panel, _| panel.close_confirmation.is_none()));
    assert!(backend.closed.lock().unwrap().is_empty());
}
