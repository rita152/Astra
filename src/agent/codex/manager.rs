//! Application-owned connection generations, startup, and shutdown.

mod catalog;
mod connection;
mod dispatch;
mod events;
mod protocol;
mod side_conversation;
mod steer;
mod transport;
mod turn;
mod workspace;

use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Receiver;
use serde_json::json;

use crate::agent::{AgentCapabilities, AgentCapability, AgentConnectionEvent};
use connection::{Connection, ConnectionState};
use events::ConnectionEventHub;
use transport::{AppServerSpawner, RealAppServerSpawner, SharedJsonWriter};

#[derive(Default)]
struct ManagerState {
    current: Option<Arc<Connection>>,
    starting: bool,
    reaping: bool,
    start_attempt: u64,
    last_start_error: Option<(u64, String)>,
    shutdown: bool,
}

struct ManagerInner {
    spawner: Arc<dyn AppServerSpawner>,
    state: Mutex<ManagerState>,
    connection_ready: Condvar,
    connection_events: Mutex<ConnectionEventHub>,
    shutdown_once: AtomicBool,
    // Retain retired identities until app exit so an expired in-memory thread
    // can never accidentally fall through to disk-based thread/resume.
    temporary_threads: Mutex<HashMap<String, side_conversation::TemporaryThread>>,
}

impl ManagerInner {
    fn ensure_connection(self: &Arc<Self>) -> Result<Arc<Connection>> {
        let mut waited_for = None;
        let attempt = loop {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?;
            if state.shutdown {
                bail!("Codex app-server manager 已关闭");
            }
            if let Some(connection) = &state.current
                && !state.starting
                && !state.reaping
                && !connection.failed.load(Ordering::Acquire)
            {
                return Ok(connection.clone());
            }
            if state.reaping {
                state = self
                    .connection_ready
                    .wait(state)
                    .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?;
                drop(state);
                continue;
            }
            if let Some(waited_attempt) = waited_for
                && let Some((failed_attempt, error)) = &state.last_start_error
                && *failed_attempt == waited_attempt
            {
                bail!(error.clone());
            }
            if state.starting {
                waited_for = Some(state.start_attempt);
                state = self
                    .connection_ready
                    .wait(state)
                    .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?;
                drop(state);
                continue;
            }
            state.starting = true;
            state.start_attempt = state.start_attempt.wrapping_add(1);
            state.last_start_error = None;
            break state.start_attempt;
        };

        let result = self.start_generation(attempt);
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?;
        state.starting = false;
        match &result {
            Ok(connection) => {
                if state
                    .current
                    .as_ref()
                    .is_none_or(|current| current.generation != connection.generation)
                {
                    state.current = Some(connection.clone());
                }
                state.last_start_error = None;
            }
            Err(error) => {
                state.last_start_error = Some((attempt, format!("{error:#}")));
                if state
                    .current
                    .as_ref()
                    .is_some_and(|connection| connection.generation == attempt)
                {
                    state.current = None;
                }
            }
        }
        self.connection_ready.notify_all();
        result
    }

    fn start_generation(self: &Arc<Self>, generation: u64) -> Result<Arc<Connection>> {
        let spawned = self.spawner.spawn()?;
        let writer = SharedJsonWriter::new(spawned.writer, Arc::downgrade(self), generation);
        let connection = Arc::new(Connection {
            generation,
            writer,
            process: spawned.process,
            next_request_id: AtomicU64::new(1),
            pending_rpcs: Mutex::new(HashMap::new()),
            completed_steer_rpcs: Mutex::new(Default::default()),
            state: Mutex::new(ConnectionState::default()),
            lifecycle_lock: Mutex::new(()),
            settings_lock: Mutex::new(()),
            failed: AtomicBool::new(false),
            manager: Arc::downgrade(self),
        });
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?;
            if state.shutdown {
                drop(state);
                connection.writer.close();
                connection.process.terminate_and_wait()?;
                bail!("Codex app-server manager 已关闭");
            }
            state.current = Some(connection.clone());
        }

        self.publish_runtime(
            generation,
            crate::agent::AgentRuntimeObservation::GenerationStarted,
        )?;
        let manager = Arc::downgrade(self);
        let reader_connection = connection.clone();
        std::thread::spawn(move || {
            ManagerInner::reader_loop(manager, reader_connection, spawned.reader);
        });

        if let Err(error) = connection.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "gpui_chat_clone",
                    "title": "GPUI Chat Clone",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true,
                    "requestAttestation": false,
                    "optOutNotificationMethods": super::runtime::OPT_OUT_NOTIFICATION_METHODS
                }
            }),
        ) {
            self.fail_generation(generation, format!("initialize 失败：{error:#}"));
            return Err(error).context("initialize 失败");
        }
        connection
            .send_message(json!({ "method": "initialized", "params": {} }))
            .context("发送 initialized 通知失败")?;
        Ok(connection)
    }

    fn fail_generation(&self, generation: u64, message: String) {
        let connection = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            let Some(connection) = state.current.as_ref() else {
                return;
            };
            if connection.generation != generation || connection.failed.swap(true, Ordering::AcqRel)
            {
                return;
            }
            let connection = connection.clone();
            state.reaping = true;
            connection
        };
        connection.writer.close();
        let reap_error = connection.process.terminate_and_wait().err();
        let message = match &reap_error {
            Some(error) => {
                format!("{message}；回收 Codex app-server generation {generation} 失败：{error:#}")
            }
            None => message,
        };
        connection.fail_all(&message);
        let _ = self.publish_runtime(
            generation,
            crate::agent::AgentRuntimeObservation::Disconnected,
        );
        let closed_temporary = self
            .temporary_threads
            .lock()
            .map(|mut threads| {
                threads
                    .iter_mut()
                    .filter_map(|(id, thread)| {
                        if thread.generation == generation && !thread.closed {
                            thread.closed = true;
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for thread_id in closed_temporary {
            self.publish_connection_event(AgentConnectionEvent::ThreadClosed { thread_id });
        }
        if let Ok(mut state) = self.state.lock() {
            if state
                .current
                .as_ref()
                .is_some_and(|current| current.generation == generation)
            {
                state.current = None;
            }
            state.reaping = false;
            if reap_error.is_some() {
                state.shutdown = true;
            }
        }
        self.connection_ready.notify_all();
    }

    fn shutdown(&self) {
        if self.shutdown_once.swap(true, Ordering::AcqRel) {
            return;
        }
        let generation = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            state.shutdown = true;
            state
                .current
                .as_ref()
                .map(|connection| connection.generation)
        };
        self.connection_ready.notify_all();
        if let Some(generation) = generation {
            self.fail_generation(
                generation,
                "Codex app-server manager 正在关闭；active operation 已终止".to_owned(),
            );
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        while state.starting || state.reaping {
            state = match self.connection_ready.wait(state) {
                Ok(state) => state,
                Err(_) => return,
            };
        }
    }
}

impl Drop for ManagerInner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Owns one long-lived Codex app-server transport for an application run.
/// Clones share the same process, request registry, reader loop and generation.
#[derive(Clone)]
pub struct CodexAppServerManager {
    inner: Arc<ManagerInner>,
}

impl Default for CodexAppServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexAppServerManager {
    pub fn new() -> Self {
        Self::with_spawner(Arc::new(RealAppServerSpawner))
    }

    fn with_spawner(spawner: Arc<dyn AppServerSpawner>) -> Self {
        Self {
            inner: Arc::new(ManagerInner {
                spawner,
                state: Mutex::new(ManagerState::default()),
                connection_ready: Condvar::new(),
                connection_events: Mutex::new(ConnectionEventHub::default()),
                shutdown_once: AtomicBool::new(false),
                temporary_threads: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    pub(super) fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.inner
            .connection_events
            .lock()
            .map(|mut hub| hub.subscribe())
            .unwrap_or_else(|_| async_channel::unbounded().1)
    }

    pub(super) fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([
            AgentCapability::ProjectList,
            AgentCapability::ProjectCreate,
            AgentCapability::ProjectUpdate,
            AgentCapability::ProjectDelete,
            AgentCapability::ProjectMove,
            AgentCapability::ThreadList,
            AgentCapability::ThreadSearch,
            AgentCapability::ThreadRead,
            AgentCapability::ThreadTurnsList,
            AgentCapability::ThreadItemsList,
            AgentCapability::ThreadRename,
            AgentCapability::ThreadArchive,
            AgentCapability::ThreadUnarchive,
            AgentCapability::ThreadDelete,
            AgentCapability::ThreadMetadataUpdate,
            AgentCapability::ThreadSectionList,
            AgentCapability::ThreadSectionCreate,
            AgentCapability::ThreadSectionMove,
            AgentCapability::SideConversation,
        ])
    }
}

#[cfg(test)]
mod tests;
