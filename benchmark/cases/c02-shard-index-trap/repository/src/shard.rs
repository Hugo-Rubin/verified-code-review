//! A single storage shard.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shard {
    id: String,
    healthy: bool,
}

impl Shard {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            healthy: true,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub fn set_healthy(&mut self, healthy: bool) {
        self.healthy = healthy;
    }
}
