//! One-shot worker contracts; no app-server process or GPUI window is started.

use super::{CodexAppServerManager, ManagerInner};
use crate::agent::{AgentConfigError, AgentConfigErrorKind};
use async_channel::{Receiver, TryRecvError};
use serde_json::json;
use std::{
    cell::Cell,
    sync::{
        Arc, Weak,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);

// Bound waits so a broken worker fails the test instead of hanging the suite.
fn receive<T>(receiver: &Receiver<T>) -> Result<T, TryRecvError> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match receiver.try_recv() {
            Err(TryRecvError::Empty) => {
                assert!(Instant::now() < deadline, "worker did not deliver or close");
                thread::sleep(Duration::from_millis(1));
            }
            result => return result,
        }
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready() {
        assert!(Instant::now() < deadline, "worker did not release its state");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn delivers_exactly_one_result_on_a_bounded_channel() {
    let manager = CodexAppServerManager::new();
    let receiver = manager.spawn_one_shot_call(|_| vec![3, 1, 3]);

    assert_eq!(receiver.capacity(), Some(1));
    assert_eq!(receive(&receiver).unwrap(), vec![3, 1, 3]);
    assert!(matches!(receive(&receiver), Err(TryRecvError::Closed)));
}

#[test]
fn executes_off_the_calling_thread() {
    let manager = CodexAppServerManager::new();
    let caller = thread::current().id();
    let receiver = manager.spawn_one_shot_call(|_| thread::current().id());

    assert_ne!(receive(&receiver).unwrap(), caller);
}

#[test]
fn starts_eagerly_without_waiting_for_a_receiver_or_completion() {
    let manager = CodexAppServerManager::new();
    let (started, start) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let receiver = manager.spawn_one_shot_call(move |_| {
        started.send(()).unwrap();
        resume.recv_timeout(TIMEOUT).unwrap();
        42
    });

    start.recv_timeout(TIMEOUT).unwrap();
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    release.send(()).unwrap();
    assert_eq!(receive(&receiver).unwrap(), 42);
}

#[test]
fn forwards_string_errors_without_retry_or_reformatting() {
    let manager = CodexAppServerManager::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let message = "读取失败：outer\n\nCaused by:\n    inner";
    let receiver = manager.spawn_one_shot_call(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>(message.to_owned())
    });

    assert_eq!(receive(&receiver).unwrap(), Err(message.to_owned()));
    assert!(matches!(receive(&receiver), Err(TryRecvError::Closed)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn preserves_structured_configuration_errors() {
    let manager = CodexAppServerManager::new();
    for (outcome_unknown, data) in [(false, None), (true, Some(json!({"generation": 7})))] {
        let expected_data = data.clone();
        let receiver = manager.spawn_one_shot_call(move |_| {
            Err::<(), _>(AgentConfigError {
                kind: AgentConfigErrorKind::Protocol,
                message: "配置回执未确认".into(),
                data,
                outcome_unknown,
            })
        });
        let error = receive(&receiver).unwrap().unwrap_err();

        assert!(matches!(error.kind, AgentConfigErrorKind::Protocol));
        assert_eq!(error.message, "配置回执未确认");
        assert_eq!(error.data, expected_data);
        assert_eq!(error.outcome_unknown, outcome_unknown);
    }
}

#[test]
fn accepts_fn_once_captures_and_send_only_results() {
    // Deliberately neither Clone nor Sync: only ownership and Send are needed.
    struct Payload(Cell<u32>);

    let manager = CodexAppServerManager::new();
    let payload = Payload(Cell::new(7));
    let receiver = manager.spawn_one_shot_call(move |_| {
        payload.0.set(9);
        payload
    });

    assert_eq!(receive(&receiver).unwrap().0.get(), 9);
}

#[test]
fn shares_and_retains_manager_state_during_the_operation() {
    let manager = CodexAppServerManager::new();
    let weak = Arc::downgrade(&manager.inner);
    let expected = weak.clone();
    let (started, start) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let receiver = manager.spawn_one_shot_call(move |worker| {
        started.send(()).unwrap();
        resume.recv_timeout(TIMEOUT).unwrap();
        assert!(std::ptr::eq(Arc::as_ptr(&worker.inner), expected.as_ptr()));
        let state = worker.inner.state.lock().unwrap();
        assert!(state.current.is_none());
        state.start_attempt
    });

    start.recv_timeout(TIMEOUT).unwrap();
    drop(manager);
    assert!(weak.upgrade().is_some());
    release.send(()).unwrap();
    assert_eq!(receive(&receiver).unwrap(), 0);
    wait_until(|| weak.upgrade().is_none());
}

struct DiscardedResult {
    manager: Weak<ManagerInner>,
    dropped: mpsc::SyncSender<bool>,
}

impl Drop for DiscardedResult {
    fn drop(&mut self) {
        // A failed send must discard its result before releasing the manager.
        let _ = self.dropped.try_send(self.manager.upgrade().is_some());
    }
}

#[test]
fn dropped_receiver_does_not_cancel_retry_or_release_manager_before_delivery() {
    let manager = CodexAppServerManager::new();
    let weak = Arc::downgrade(&manager.inner);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (release, resume) = mpsc::sync_channel(1);
    let (dropped, drop_observed) = mpsc::sync_channel(1);
    let receiver = manager.spawn_one_shot_call(move |worker| {
        resume.recv_timeout(TIMEOUT).unwrap();
        observed.fetch_add(1, Ordering::SeqCst);
        DiscardedResult {
            manager: Arc::downgrade(&worker.inner),
            dropped,
        }
    });

    drop(receiver);
    drop(manager);
    release.send(()).unwrap();
    assert!(drop_observed.recv_timeout(TIMEOUT).unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    wait_until(|| weak.upgrade().is_none());
}

#[test]
fn unread_result_does_not_keep_the_worker_or_manager_alive() {
    let manager = CodexAppServerManager::new();
    let weak = Arc::downgrade(&manager.inner);
    let receiver = manager.spawn_one_shot_call(|_| 17);

    drop(manager);
    wait_until(|| weak.upgrade().is_none());
    assert_eq!(receive(&receiver).unwrap(), 17);
    assert!(matches!(receive(&receiver), Err(TryRecvError::Closed)));
}

#[test]
fn unrelated_calls_do_not_serialize_behind_a_blocked_operation() {
    let manager = CodexAppServerManager::new();
    let (started, start) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let first = manager.spawn_one_shot_call(move |_| {
        started.send(()).unwrap();
        resume.recv_timeout(TIMEOUT).unwrap();
        1
    });

    start.recv_timeout(TIMEOUT).unwrap();
    let second = manager.spawn_one_shot_call(|_| 2);
    assert_eq!(receive(&second).unwrap(), 2);
    assert!(matches!(first.try_recv(), Err(TryRecvError::Empty)));
    release.send(()).unwrap();
    assert_eq!(receive(&first).unwrap(), 1);
}

#[test]
fn panic_closes_the_channel_without_a_synthetic_response_or_retry() {
    let manager = CodexAppServerManager::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let receiver: Receiver<()> = manager.spawn_one_shot_call(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        panic!("intentional one-shot worker panic");
    });

    assert!(matches!(receive(&receiver), Err(TryRecvError::Closed)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
