//! Shared application state. The login in progress rides in a signed cookie, so
//! it survives a restart.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::MySqlPool;

use crate::config::Config;

/// Longer than one long poll, short enough that a sleeping Mac stops the picker
/// promising an answer.
const WORKER_ALIVE: Duration = Duration::from_secs(90);

/// The worker is single-threaded and does not poll while preloading (~130 s at
/// worst), which can outlast [`WORKER_ALIVE`].
const PRELOAD_GRACE: Duration = Duration::from_secs(180);

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    /// In memory: a fact about now, re-learnt within one poll after a restart.
    worker_seen: Arc<Mutex<Option<Instant>>>,
    /// Wakes a waiting worker when a job is queued. Per pod, so the worker also
    /// re-checks on a timer.
    job_queued: Arc<tokio::sync::Notify>,
    /// The day's system prompt to preload; best-effort.
    warm_system: Arc<Mutex<Option<String>>>,
    /// When a worker took a preload, starting its silent window. Cleared on its
    /// next poll, so it never keeps a dead worker alive past [`PRELOAD_GRACE`].
    warm_taken: Arc<Mutex<Option<Instant>>>,
    /// The feed's text, parsed per request so "upcoming" is decided then.
    bins: Arc<Mutex<Option<(Instant, String)>>>,
    /// So a held long poll answers now rather than outlasting the shutdown grace.
    stopping: Arc<tokio::sync::watch::Sender<bool>>,
}

/// The feed asks for daily; an hour still catches a same-morning correction.
const BINS_TTL: Duration = Duration::from_secs(3600);

impl AppState {
    pub fn new(pool: MySqlPool, cfg: Config, http: reqwest::Client) -> Self {
        Self {
            pool,
            cfg: Arc::new(cfg),
            http,
            worker_seen: Arc::new(Mutex::new(None)),
            job_queued: Arc::new(tokio::sync::Notify::new()),
            warm_system: Arc::new(Mutex::new(None)),
            warm_taken: Arc::new(Mutex::new(None)),
            bins: Arc::new(Mutex::new(None)),
            stopping: Arc::new(tokio::sync::watch::Sender::new(false)),
        }
    }

    pub fn begin_shutdown(&self) {
        self.stopping.send_replace(true);
    }

    pub fn stopping(&self) -> tokio::sync::watch::Receiver<bool> {
        self.stopping.subscribe()
    }

    pub fn cached_bins(&self) -> Option<String> {
        self.bins
            .lock()
            .expect("bins cache poisoned")
            .as_ref()
            .filter(|(at, _)| at.elapsed() < BINS_TTL)
            .map(|(_, ics)| ics.clone())
    }

    pub fn cache_bins(&self, ics: String) {
        *self.bins.lock().expect("bins cache poisoned") = Some((Instant::now(), ics));
    }

    pub fn notify_job_queued(&self) {
        self.job_queued.notify_waiters();
    }

    /// A newer request replaces an unconsumed one: the prompt is the same all day.
    pub fn request_warm(&self, system: String) {
        *self.warm_system.lock().expect("warm system poisoned") = Some(system);
        self.job_queued.notify_waiters();
    }

    /// One preload per ask; starts the worker's silent window.
    pub fn take_warm(&self) -> Option<String> {
        let system = self
            .warm_system
            .lock()
            .expect("warm system poisoned")
            .take()?;
        *self.warm_taken.lock().expect("warm clock poisoned") = Some(Instant::now());
        Some(system)
    }

    /// Create this future before looking at the queue, or a job landing in between
    /// waits for the timer.
    pub fn job_queued(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.job_queued)
    }

    pub fn mark_worker_seen(&self) {
        *self.worker_seen.lock().expect("worker clock poisoned") = Some(Instant::now());
        // Polling again, so the preload is done.
        *self.warm_taken.lock().expect("warm clock poisoned") = None;
    }

    /// By observation, not configuration: "thinking…" means a worker is listening.
    pub fn worker_alive(&self) -> bool {
        let polled = self
            .worker_seen
            .lock()
            .expect("worker clock poisoned")
            .is_some_and(|t| t.elapsed() < WORKER_ALIVE);
        if polled {
            return true;
        }
        self.warm_taken
            .lock()
            .expect("warm clock poisoned")
            .is_some_and(|t| t.elapsed() < PRELOAD_GRACE)
    }
}
