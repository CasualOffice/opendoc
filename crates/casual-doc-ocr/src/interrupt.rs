//! Cancel and time budgets a provider polls between model runs (PDFR-I7).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Why a provider stopped early.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    /// The host cancelled: the caller abandons the whole import (PDFR-I3).
    Cancelled,
    /// The host's time budget ran out: the caller keeps what it has and
    /// reports the rest as not recognised.
    OutOfTime,
}

/// Polled by a provider between model runs. Implementations must be cheap
/// (an atomic load, a clock read).
pub trait Interrupt: Send + Sync {
    /// `Some` to stop now.
    fn poll(&self) -> Option<Stop>;
}

/// Never stops.
#[derive(Clone, Copy, Debug, Default)]
pub struct Never;

impl Interrupt for Never {
    fn poll(&self) -> Option<Stop> {
        None
    }
}

/// A cancel flag shared between the host and the work (cloning shares it).
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A fresh, uncancelled token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation; every clone observes it.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Interrupt for CancelToken {
    fn poll(&self) -> Option<Stop> {
        self.is_cancelled().then_some(Stop::Cancelled)
    }
}

/// A cancel token plus a wall-clock budget, for native hosts. Not available in
/// `wasm32-unknown-unknown`, which has no `Instant`; a browser host implements
/// [`Interrupt`] over its own clock.
#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Debug)]
pub struct Deadline {
    cancel: CancelToken,
    end: std::time::Instant,
}

#[cfg(not(target_family = "wasm"))]
impl Deadline {
    /// Stops with [`Stop::OutOfTime`] once `budget` has elapsed from now, and
    /// with [`Stop::Cancelled`] when `cancel` fires (cancel wins).
    #[must_use]
    pub fn after(budget: std::time::Duration, cancel: CancelToken) -> Self {
        Self {
            cancel,
            end: std::time::Instant::now() + budget,
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl Interrupt for Deadline {
    fn poll(&self) -> Option<Stop> {
        self.cancel
            .poll()
            .or_else(|| (std::time::Instant::now() >= self.end).then_some(Stop::OutOfTime))
    }
}
