//! A bounded connection pool.

use std::sync::{Arc, Mutex};

#[derive(Debug, PartialEq, Eq)]
pub enum PoolError {
    /// All permits are in use.
    Exhausted,
}

/// A handle to a leased connection. Releasing it returns the permit.
pub struct Conn {
    active: Arc<Mutex<usize>>,
    released: bool,
}

impl Conn {
    pub fn release(mut self) {
        self.released = true;
        let mut n = self.active.lock().expect("pool counter poisoned");
        *n -= 1;
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if !self.released {
            let mut n = self.active.lock().expect("pool counter poisoned");
            *n -= 1;
        }
    }
}

/// Hands out at most `max` concurrent connections.
pub struct Pool {
    active: Arc<Mutex<usize>>,
    max: usize,
}

impl Pool {
    pub fn new(max: usize) -> Self {
        Self {
            active: Arc::new(Mutex::new(0)),
            max,
        }
    }

    /// Number of connections currently leased out.
    pub fn active(&self) -> usize {
        *self.active.lock().expect("pool counter poisoned")
    }

    /// Lease a connection, or fail if the pool is at capacity.
    pub fn acquire(&self) -> Result<Conn, PoolError> {
        let mut n = self.active.lock().expect("pool counter poisoned");
        *n += 1;

        if *n > self.max {
            return Err(PoolError::Exhausted);
        }

        drop(n);
        Ok(Conn {
            active: Arc::clone(&self.active),
            released: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leases_up_to_the_limit() {
        let pool = Pool::new(2);
        let a = pool.acquire().unwrap();
        let b = pool.acquire().unwrap();
        assert_eq!(pool.active(), 2);
        a.release();
        b.release();
        assert_eq!(pool.active(), 0);
    }

    #[test]
    fn refuses_beyond_the_limit() {
        let pool = Pool::new(1);
        let a = pool.acquire().unwrap();
        assert!(matches!(pool.acquire(), Err(PoolError::Exhausted)));
        a.release();
    }
}
