use std::time::{Duration, Instant};

/// Fixed-window counter, the same semantics as the HTTP `RateLimiter` in
/// `wog-middleware` (N events per window, reset when the window ends) but for
/// a single owner, so no map and no locking: each WebSocket connection keeps
/// its own.
#[derive(Debug, Clone)]
pub struct WindowLimiter {
    max: u32,
    window: Duration,
    used: u32,
    reset_at: Instant,
}

impl WindowLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self {
            max,
            window,
            used: 0,
            reset_at: Instant::now() + window,
        }
    }

    /// Count one event. `false` when the window's budget is already spent.
    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        if now >= self.reset_at {
            self.used = 0;
            self.reset_at = now + self.window;
        }
        if self.used < self.max {
            self.used += 1;
            true
        } else {
            false
        }
    }
}
