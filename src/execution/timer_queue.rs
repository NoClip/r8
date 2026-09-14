//! Safe Rust reimplementation of host timer scheduling and event loop macrotasks.
//!
//! Provides `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, and
//! deterministic virtual timer execution for the isolate runtime.

use crate::execution::microtask_queue::MicrotaskQueue;
use crate::objects::{JSFunction, JSValue};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static CURRENT_TIMER_QUEUE: RefCell<Option<Rc<RefCell<TimerQueue>>>> = const { RefCell::new(None) };
}

/// Representation of an active host timer.
#[derive(Clone)]
pub struct Timer {
    pub id: u32,
    pub callback: Rc<JSFunction>,
    pub args: Vec<JSValue>,
    pub target_time_ms: u64,
    pub interval_ms: Option<u64>,
    pub cancelled: bool,
}

/// A priority queue and manager for asynchronous host timers.
pub struct TimerQueue {
    timers: Vec<Timer>,
    next_id: u32,
    current_time_ms: u64,
}

impl Default for TimerQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerQueue {
    /// Creates a new empty timer queue initialized at virtual time 0.
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            next_id: 1,
            current_time_ms: 0,
        }
    }

    /// Sets the active thread-local timer queue for the current isolate realm.
    pub fn set_current(queue: Option<Rc<RefCell<TimerQueue>>>) {
        CURRENT_TIMER_QUEUE.with(|q| {
            *q.borrow_mut() = queue;
        });
    }

    /// Gets a clone of the current thread-local timer queue.
    pub fn current() -> Option<Rc<RefCell<TimerQueue>>> {
        CURRENT_TIMER_QUEUE.with(|q| q.borrow().clone())
    }

    /// Returns the current virtual clock timestamp in milliseconds.
    pub fn current_time(&self) -> u64 {
        self.current_time_ms
    }

    /// Schedules a single-shot timer.
    pub fn set_timeout(&mut self, callback: Rc<JSFunction>, delay_ms: u64, args: Vec<JSValue>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let target_time_ms = self.current_time_ms.saturating_add(delay_ms);
        self.timers.push(Timer {
            id,
            callback,
            args,
            target_time_ms,
            interval_ms: None,
            cancelled: false,
        });
        id
    }

    /// Schedules a repeating interval timer.
    pub fn set_interval(&mut self, callback: Rc<JSFunction>, interval_ms: u64, args: Vec<JSValue>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let interval = interval_ms.max(1);
        let target_time_ms = self.current_time_ms.saturating_add(interval);
        self.timers.push(Timer {
            id,
            callback,
            args,
            target_time_ms,
            interval_ms: Some(interval),
            cancelled: false,
        });
        id
    }

    /// Cancels a pending timer by its identifier.
    pub fn clear_timer(&mut self, id: u32) {
        for timer in &mut self.timers {
            if timer.id == id {
                timer.cancelled = true;
            }
        }
    }

    /// Returns the number of active, non-cancelled timers.
    pub fn active_timer_count(&self) -> usize {
        self.timers.iter().filter(|t| !t.cancelled).count()
    }

    /// Advances the virtual clock by `ms` milliseconds and executes all due timers.
    /// Drains microtasks after each macrotask callback invocation adhering to V8/HTML spec.
    pub fn advance_time(this_rc: &Rc<RefCell<Self>>, ms: u64) -> usize {
        let target_clock = this_rc.borrow().current_time_ms.saturating_add(ms);
        Self::run_until(this_rc, target_clock)
    }

    /// Runs all pending timers up to the specified target clock timestamp.
    pub fn run_until(this_rc: &Rc<RefCell<Self>>, target_clock: u64) -> usize {
        let mut executed_count = 0;
        let max_iterations = 10_000;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                break;
            }

            // Find next eligible timer whose target_time_ms <= target_clock
            let next_timer_opt = {
                let mut borrowed = this_rc.borrow_mut();
                // Clean up cancelled timers
                borrowed.timers.retain(|t| !t.cancelled);

                let mut min_idx: Option<usize> = None;
                for (idx, t) in borrowed.timers.iter().enumerate() {
                    if t.target_time_ms <= target_clock {
                        match min_idx {
                            None => min_idx = Some(idx),
                            Some(cur) => {
                                if t.target_time_ms < borrowed.timers[cur].target_time_ms {
                                    min_idx = Some(idx);
                                }
                            }
                        }
                    }
                }

                min_idx.map(|idx| borrowed.timers.remove(idx))
            };

            let timer = match next_timer_opt {
                Some(t) => t,
                None => break,
            };

            if timer.cancelled {
                continue;
            }

            // Advance clock to timer target if it's greater than current
            {
                let mut borrowed = this_rc.borrow_mut();
                if timer.target_time_ms > borrowed.current_time_ms {
                    borrowed.current_time_ms = timer.target_time_ms;
                }
            }

            // Execute macrotask callback
            let _ = timer.callback.call(&JSValue::Undefined, &timer.args);
            executed_count += 1;

            // Drain microtasks after the macrotask turn
            MicrotaskQueue::run_current_microtasks();

            // If it's an interval and wasn't cancelled during callback execution, reschedule
            if let Some(interval) = timer.interval_ms {
                let mut borrowed = this_rc.borrow_mut();
                let is_cancelled = borrowed.timers.iter().any(|t| t.id == timer.id && t.cancelled);
                if !is_cancelled && !timer.cancelled {
                    let next_target = timer.target_time_ms.saturating_add(interval);
                    borrowed.timers.push(Timer {
                        id: timer.id,
                        callback: timer.callback.clone(),
                        args: timer.args.clone(),
                        target_time_ms: next_target,
                        interval_ms: Some(interval),
                        cancelled: false,
                    });
                }
            }
        }

        // Set final target clock
        {
            let mut borrowed = this_rc.borrow_mut();
            if target_clock > borrowed.current_time_ms {
                borrowed.current_time_ms = target_clock;
            }
        }

        executed_count
    }

    /// Runs all pending timers to completion or until no timers remain.
    pub fn run_all(this_rc: &Rc<RefCell<Self>>) -> usize {
        let max_clock = this_rc
            .borrow()
            .timers
            .iter()
            .filter(|t| !t.cancelled)
            .map(|t| t.target_time_ms)
            .max()
            .unwrap_or(0);
        Self::run_until(this_rc, max_clock)
    }
}
