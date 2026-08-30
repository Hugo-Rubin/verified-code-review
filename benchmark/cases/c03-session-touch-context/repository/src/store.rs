//! In-memory session storage.

use std::collections::HashMap;

pub type SessionId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub user: String,
    /// Monotonic tick of the last observed activity.
    pub last_seen: u64,
}

#[derive(Default)]
pub struct SessionStore {
    sessions: HashMap<SessionId, Session>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, session: Session) {
        self.sessions.insert(session.id.clone(), session);
    }

    pub fn contains(&self, id: &SessionId) -> bool {
        self.sessions.contains_key(id)
    }

    pub fn get(&self, id: &SessionId) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// Drop a session. Returns whether one was present.
    pub fn remove(&mut self, id: &SessionId) -> bool {
        self.sessions.remove(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Record activity on a session.
    ///
    /// Callers check `contains` first, so the session is known to be present.
    pub fn touch(&mut self, id: &SessionId, now: u64) {
        let session = self.sessions.get_mut(id).unwrap();
        session.last_seen = now;
    }

    /// Drop every session whose last activity is older than `cutoff`.
    pub fn expire_before(&mut self, cutoff: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| s.last_seen >= cutoff);
        before - self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, last_seen: u64) -> Session {
        Session {
            id: id.to_string(),
            user: "u".to_string(),
            last_seen,
        }
    }

    #[test]
    fn touch_updates_last_seen() {
        let mut store = SessionStore::new();
        store.insert(session("a", 1));
        if store.contains(&"a".to_string()) {
            store.touch(&"a".to_string(), 99);
        }
        assert_eq!(store.get(&"a".to_string()).unwrap().last_seen, 99);
    }

    #[test]
    fn expire_drops_stale_sessions() {
        let mut store = SessionStore::new();
        store.insert(session("a", 1));
        store.insert(session("b", 10));
        assert_eq!(store.expire_before(5), 1);
        assert_eq!(store.len(), 1);
    }
}
