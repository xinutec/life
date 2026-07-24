//! Shared application state + the short-lived OAuth `state` store.
//!
//! NOTE: the pending-OAuth map is in-memory (per process). That is fine for a
//! single-pod single-user deployment. The design doc (docs/design/overview.md
//! §3) calls for moving this to a DB table before running a 2nd replica.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::Rng;
use sqlx::MySqlPool;

use crate::config::Config;

const OAUTH_TTL: Duration = Duration::from_secs(600); // 10 minutes

pub struct PendingOauth {
    created: Instant,
    /// Internal path to redirect to after callback; allowlist-validated when used.
    pub return_to: Option<String>,
}

/// A worker seen less recently than this is assumed gone. Comfortably longer than
/// one long-poll cycle, so a worker that is merely between polls still counts as
/// alive; short enough that a Mac that went to sleep stops the picker from
/// promising an answer that isn't coming.
const WORKER_ALIVE: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    oauth: Arc<Mutex<HashMap<String, PendingOauth>>>,
    /// Last time the emotion-suggestion worker polled for work. In-memory on
    /// purpose: it is a fact about *now*, worthless after a restart, and a restart
    /// re-learns it within one poll.
    worker_seen: Arc<Mutex<Option<Instant>>>,
    /// Wakes a waiting worker the instant a suggestion job is queued, so a note
    /// is picked up as you finish writing it rather than at the next tick. A
    /// hint, not the mechanism: the worker also re-checks on a slow timer, which
    /// is what covers a job queued by a *different* process (this signal, like
    /// the OAuth map above, is per-pod).
    job_queued: Arc<tokio::sync::Notify>,
    /// A pending "warm the model" request: the day's system prompt, set when a
    /// check-in note starts being written. The worker preloads it — loading the
    /// weights and building the day's prefix cache — so the real suggestion a
    /// moment later is warm instead of paying the ~60s cold load then. In-memory
    /// and best-effort: a missed or stale warm just means the old, cold timing.
    warm_system: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new(pool: MySqlPool, cfg: Config, http: reqwest::Client) -> Self {
        Self {
            pool,
            cfg: Arc::new(cfg),
            http,
            oauth: Arc::new(Mutex::new(HashMap::new())),
            worker_seen: Arc::new(Mutex::new(None)),
            job_queued: Arc::new(tokio::sync::Notify::new()),
            warm_system: Arc::new(Mutex::new(None)),
        }
    }

    /// A suggestion job was just queued.
    pub fn notify_job_queued(&self) {
        self.job_queued.notify_waiters();
    }

    /// Ask the worker to preload the model for this day's system prompt. Wakes the
    /// poll on the same signal as a job, so the load starts within a second of the
    /// note's first keystroke. A newer request simply replaces an unconsumed one —
    /// the system is the same all day, so the latest is as good as the first.
    pub fn request_warm(&self, system: String) {
        *self.warm_system.lock().expect("warm system poisoned") = Some(system);
        self.job_queued.notify_waiters();
    }

    /// Take the pending warm request, if any, clearing it — one preload per ask.
    pub fn take_warm(&self) -> Option<String> {
        self.warm_system
            .lock()
            .expect("warm system poisoned")
            .take()
    }

    /// Wait for the next queued job. The caller must create this future BEFORE
    /// looking at the queue, or a job that lands in between would be missed until
    /// the fallback timer.
    pub fn job_queued(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.job_queued)
    }

    /// The emotion worker just asked for work.
    pub fn mark_worker_seen(&self) {
        *self.worker_seen.lock().expect("worker clock poisoned") = Some(Instant::now());
    }

    /// Is there a worker to compute suggestions? Answered by observation rather
    /// than configuration, so the picker's "thinking…" reflects a machine that is
    /// actually listening, not merely a token that was set once.
    pub fn worker_alive(&self) -> bool {
        self.worker_seen
            .lock()
            .expect("worker clock poisoned")
            .is_some_and(|t| t.elapsed() < WORKER_ALIVE)
    }

    /// Mint a new opaque `state` token and remember its pending entry.
    pub fn create_oauth_state(&self, return_to: Option<String>) -> String {
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        let state = hex::encode(bytes);
        let mut map = self.oauth.lock().expect("oauth map poisoned");
        map.retain(|_, v| v.created.elapsed() < OAUTH_TTL);
        map.insert(
            state.clone(),
            PendingOauth {
                created: Instant::now(),
                return_to,
            },
        );
        state
    }

    /// Consume a `state` token exactly once. None if unknown or expired.
    pub fn consume_oauth_state(&self, state: &str) -> Option<PendingOauth> {
        let mut map = self.oauth.lock().expect("oauth map poisoned");
        let entry = map.remove(state)?;
        if entry.created.elapsed() > OAUTH_TTL {
            return None;
        }
        Some(entry)
    }
}
