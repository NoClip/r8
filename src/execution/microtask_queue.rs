//! Safe Rust reimplementation of Google V8's `src/execution/microtask-queue.h`.
//!
//! Implements the ECMAScript microtask queue for asynchronous tasks,
//! `queueMicrotask` callbacks, Promise reactions, and microtask checkpoint execution.

use crate::objects::{JSFunction, JSObject, JSValue};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

thread_local! {
    static CURRENT_QUEUE: RefCell<Option<Rc<RefCell<MicrotaskQueue>>>> = const { RefCell::new(None) };
}

/// An individual microtask scheduled to execute in a future microtask turn.
pub enum Microtask {
    /// A generic JS callback with arguments (e.g. `queueMicrotask(cb)`).
    Callable {
        func: Rc<JSFunction>,
        args: Vec<JSValue>,
    },
    /// A Promise reaction task to be evaluated during microtask checkpoint.
    PromiseReaction {
        handler: Option<Rc<JSFunction>>,
        argument: JSValue,
        downstream: Option<Rc<RefCell<JSObject>>>,
        is_rejection: bool,
    },
    /// A native Rust closure callback.
    Custom(Box<dyn FnOnce(&Rc<RefCell<MicrotaskQueue>>) + 'static>),
}

/// A FIFO queue managing ECMAScript microtasks and checkpoint execution.
#[derive(Default)]
pub struct MicrotaskQueue {
    queue: VecDeque<Microtask>,
}

impl MicrotaskQueue {
    /// Creates a new empty microtask queue.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Sets the active thread-local microtask queue for the current isolate realm.
    pub fn set_current(queue: Option<Rc<RefCell<MicrotaskQueue>>>) {
        CURRENT_QUEUE.with(|q| {
            *q.borrow_mut() = queue;
        });
    }

    /// Gets a clone of the current thread-local microtask queue.
    pub fn current() -> Option<Rc<RefCell<MicrotaskQueue>>> {
        CURRENT_QUEUE.with(|q| q.borrow().clone())
    }

    /// Enqueues a microtask into the current active microtask queue.
    pub fn enqueue_current(task: Microtask) -> bool {
        if let Some(q) = Self::current() {
            q.borrow_mut().enqueue(task);
            true
        } else {
            false
        }
    }

    /// Runs all microtasks in the current active queue if available.
    pub fn run_current_microtasks() -> usize {
        if let Some(q) = Self::current() {
            Self::run_microtasks(&q).unwrap_or(0)
        } else {
            0
        }
    }

    /// Pushes a task to the back of this microtask queue.
    pub fn enqueue(&mut self, task: Microtask) {
        self.queue.push_back(task);
    }

    /// Returns the number of pending tasks in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns true if there are no pending microtasks.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Drains and executes all pending microtasks until the queue is completely exhausted.
    ///
    /// Adheres to V8's microtask checkpoint specification: tasks scheduled by other microtasks
    /// are drained during the same microtask turn. Enforces an upper limit of 100,000 iterations
    /// to safely abort infinite microtask loops.
    pub fn run_microtasks(this_rc: &Rc<RefCell<Self>>) -> Result<usize, String> {
        let prev_current = Self::current();
        Self::set_current(Some(this_rc.clone()));

        let mut executed = 0;
        const MAX_ITERATIONS: usize = 100_000;

        loop {
            let next_task = {
                let mut borrowed = this_rc.borrow_mut();
                borrowed.queue.pop_front()
            };

            let task = match next_task {
                Some(t) => t,
                None => break,
            };

            executed += 1;
            if executed > MAX_ITERATIONS {
                Self::set_current(prev_current);
                return Err("RangeError: Maximum call stack or microtask iterations exceeded (infinite microtask loop detected)".to_string());
            }

            match task {
                Microtask::Callable { func, args } => {
                    let _ = func.call(&JSValue::Undefined, &args);
                }
                Microtask::PromiseReaction {
                    handler,
                    argument,
                    downstream,
                    is_rejection,
                } => {
                    if let Some(func) = handler {
                        match func.call(&JSValue::Undefined, &[argument]) {
                            Ok(res) => {
                                if let Some(down) = downstream {
                                    crate::builtins::promise::resolve_promise_internal(
                                        &down,
                                        res,
                                        this_rc,
                                    );
                                }
                            }
                            Err(err) => {
                                if let Some(down) = downstream {
                                    crate::builtins::promise::reject_promise_internal(
                                        &down,
                                        JSValue::String(err),
                                        this_rc,
                                    );
                                }
                            }
                        }
                    } else if let Some(down) = downstream {
                        if is_rejection {
                            crate::builtins::promise::reject_promise_internal(
                                &down,
                                argument,
                                this_rc,
                            );
                        } else {
                            crate::builtins::promise::resolve_promise_internal(
                                &down,
                                argument,
                                this_rc,
                            );
                        }
                    }
                }
                Microtask::Custom(cb) => {
                    cb(this_rc);
                }
            }
        }

        Self::set_current(prev_current);
        Ok(executed)
    }
}
