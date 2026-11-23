//! One repository's inbox: its rows, subscribers and schedule, and the reads that fill the rows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tddy_github::trunk_feed::RepoSlug;

use crate::feed::FeedMemory;
use crate::in_flight::{FlightGuard, InFlight};
use crate::pacing::Pacing;
use crate::rows::{fetch, system_now_ms, Fetched, Row, RowValue, Tally};
use crate::subscriptions::Subscribers;
use crate::{Cached, InboxError, InboxEvent, RepoReaders, RowKey};

/// How long a row stays fresh when no subscriber's feed is there to invalidate it.
const FEEDLESS_ROW_TTL_MS: u64 = 30_000;

pub(crate) struct RepoState {
    repo: RepoSlug,
    pub rows: HashMap<RowKey, Row>,
    pub subscribers: Subscribers,
    pub pacing: Pacing,
    pub feed: FeedMemory,
}

impl RepoState {
    fn fresh(&self, key: &RowKey, now_ms: u64) -> Option<Cached<RowValue>> {
        let row = self.rows.get(key)?;
        let kept_fresh_by_feed = self.subscribers.is_listening();
        let young = now_ms.saturating_sub(row.fetched_at_unix_ms) < FEEDLESS_ROW_TTL_MS;
        (!row.stale && (kept_fresh_by_feed || young)).then(|| row.cached())
    }

    /// Keep what a read produced.
    fn store(&mut self, key: &RowKey, fetched: Fetched, now_ms: u64) -> Cached<RowValue> {
        let row = Row {
            value: fetched.value,
            fetched_at_unix_ms: now_ms,
            etags: fetched.etags,
            stale: false,
        };
        let cached = row.cached();
        self.rows.insert(key.clone(), row);
        cached
    }

    /// Cached rows holding check runs that have not finished.
    pub(crate) fn pending_check_rows(&self) -> Vec<RowKey> {
        self.rows
            .iter()
            .filter(|(_, row)| row.has_pending_checks())
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// The cached commit lists of pull request `number`, one per head read.
    pub(crate) fn commit_rows_of(&self, number: u64) -> Vec<RowKey> {
        self.rows
            .keys()
            .filter(|key| matches!(key, RowKey::PrCommits { number: n, .. } if *n == number))
            .cloned()
            .collect()
    }

    /// Mark rows stale and tell their subscribers. Rows in `announce` are announced whether or not
    /// they are cached; rows in `if_cached` only when they are.
    pub(crate) fn invalidate(&mut self, announce: &[RowKey], if_cached: &[RowKey]) {
        let cached: Vec<&RowKey> = if_cached
            .iter()
            .filter(|key| self.rows.contains_key(*key))
            .collect();
        for key in announce.iter().chain(cached) {
            if let Some(row) = self.rows.get_mut(key) {
                row.stale = true;
            }
            self.subscribers.publish(&InboxEvent {
                repo: self.repo.clone(),
                kind: key.event_kind(),
                row: key.clone(),
            });
        }
    }

    pub(crate) fn announce_changed(&mut self, key: &RowKey) {
        self.subscribers.publish(&InboxEvent {
            repo: self.repo.clone(),
            kind: key.event_kind(),
            row: key.clone(),
        });
    }
}

pub(crate) struct RepoInbox {
    pub repo: RepoSlug,
    readers: Arc<dyn RepoReaders>,
    state: Mutex<RepoState>,
    flights: InFlight<RowKey>,
}

impl RepoInbox {
    pub(crate) fn new(repo: RepoSlug, readers: Arc<dyn RepoReaders>) -> Self {
        Self {
            state: Mutex::new(RepoState {
                repo: repo.clone(),
                rows: HashMap::new(),
                subscribers: Subscribers::default(),
                pacing: Pacing::default(),
                feed: FeedMemory::default(),
            }),
            repo,
            readers,
            flights: InFlight::new(),
        }
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, RepoState> {
        self.state.lock().expect("repository state poisoned")
    }

    pub(crate) fn readers(&self) -> &dyn RepoReaders {
        &*self.readers
    }

    /// What is cached for `key`, fresh or not.
    pub(crate) fn peek(&self, key: &RowKey) -> Option<Cached<RowValue>> {
        self.state().rows.get(key).map(Row::cached)
    }

    /// The row's value: from the cache while fresh, else revalidated with one request however many
    /// readers ask at once. A spent quota answers from the cache, however stale.
    pub(crate) async fn read(&self, key: &RowKey) -> Result<Cached<RowValue>, InboxError> {
        if let Some(cached) = self.state().fresh(key, system_now_ms()) {
            return Ok(cached);
        }
        let _turn = self.flights.acquire(key.clone()).await;
        let now_ms = system_now_ms();
        let (stale, exhausted_until_ms) = {
            let state = self.state();
            if let Some(cached) = state.fresh(key, now_ms) {
                return Ok(cached);
            }
            (
                state.rows.get(key).map(Row::cached),
                state
                    .pacing
                    .paused_at(now_ms)
                    .then(|| state.pacing.exhausted_until_ms())
                    .flatten(),
            )
        };
        if let Some(reset_ms) = exhausted_until_ms {
            return stale.ok_or(InboxError::RateLimited {
                reset_at_unix_s: reset_ms / 1000,
            });
        }
        let mut tally = Tally::default();
        let (cached, _changed) = self.revalidate(key, &mut tally).await?;
        Ok(cached)
    }

    /// Read `key` now, sending its stored ETags; says whether its value changed. The caller holds
    /// the row's turn.
    pub(crate) async fn revalidate(
        &self,
        key: &RowKey,
        tally: &mut Tally,
    ) -> Result<(Cached<RowValue>, bool), InboxError> {
        let reader = self.readers.reader_for(&self.repo)?;
        let previous = self.state().rows.get(key).cloned();
        let outcome = fetch(&*reader, &self.repo, key, previous.as_ref(), tally).await;
        let mut state = self.state();
        state.pacing.observe_rate_limit(tally.rate_limit);
        let fetched = outcome?;
        let changed = fetched.changed;
        Ok((state.store(key, fetched, system_now_ms()), changed))
    }

    /// Wait for `key`'s turn on GitHub.
    pub(crate) async fn take_turn(&self, key: &RowKey) -> FlightGuard<RowKey> {
        self.flights.acquire(key.clone()).await
    }
}
