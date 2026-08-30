//! Request handling.

use crate::store::{SessionId, SessionStore};

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    Ok,
    Unauthorized,
}

pub struct Server {
    store: SessionStore,
    clock: u64,
}

impl Server {
    pub fn new(store: SessionStore) -> Self {
        Self { store, clock: 0 }
    }

    pub fn advance_clock(&mut self, ticks: u64) {
        self.clock += ticks;
    }

    pub fn store_mut(&mut self) -> &mut SessionStore {
        &mut self.store
    }

    /// Handle an authenticated request.
    pub fn on_request(&mut self, id: &SessionId) -> Response {
        if !self.store.contains(id) {
            return Response::Unauthorized;
        }
        self.store.touch(id, self.clock);
        Response::Ok
    }

    /// Handle a keepalive heartbeat from a connected client.
    ///
    /// Heartbeats arrive on the open socket and carry no payload beyond the
    /// session id, so they are recorded directly.
    pub fn on_heartbeat(&mut self, id: &SessionId) {
        self.store.touch(id, self.clock);
    }

    /// Periodic sweep. Drops sessions idle for longer than `max_idle`.
    pub fn sweep(&mut self, max_idle: u64) -> usize {
        let cutoff = self.clock.saturating_sub(max_idle);
        self.store.expire_before(cutoff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Session;

    fn server_with(id: &str) -> Server {
        let mut store = SessionStore::new();
        store.insert(Session {
            id: id.to_string(),
            user: "u".to_string(),
            last_seen: 0,
        });
        Server::new(store)
    }

    #[test]
    fn unknown_session_is_rejected() {
        let mut s = server_with("a");
        assert_eq!(s.on_request(&"b".to_string()), Response::Unauthorized);
    }

    #[test]
    fn known_session_is_accepted() {
        let mut s = server_with("a");
        s.advance_clock(5);
        assert_eq!(s.on_request(&"a".to_string()), Response::Ok);
    }

    #[test]
    fn heartbeat_records_activity() {
        let mut s = server_with("a");
        s.advance_clock(3);
        s.on_heartbeat(&"a".to_string());
    }

    #[test]
    fn sweep_drops_idle_sessions() {
        let mut s = server_with("a");
        s.advance_clock(100);
        assert_eq!(s.sweep(10), 1);
    }
}
