//! A tiny single-threaded executor: what `Root::spawn` runs futures on. A task is polled
//! when it is spawned and again whenever its waker fires (a network answer arrived). It
//! never runs on its own: the page's exports call [`run`] after anything that could wake
//! a task.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};

use crate::Root;

type Task = Pin<Box<dyn Future<Output = ()>>>;

pub(crate) struct Tasks {
    next: u64,
    map: HashMap<u64, Task>,
    ready: Arc<Mutex<Vec<u64>>>,
    running: bool,
}

impl Tasks {
    pub fn new() -> Tasks {
        Tasks {
            next: 0,
            map: HashMap::new(),
            ready: Arc::new(Mutex::new(Vec::new())),
            running: false,
        }
    }

    pub fn add(&mut self, fut: Task) {
        self.next += 1;
        self.map.insert(self.next, fut);
        self.ready.lock().unwrap().push(self.next);
    }

    /// How many tasks have not finished.
    pub fn pending(&self) -> usize {
        self.map.len()
    }
}

struct TaskWaker {
    id: u64,
    ready: Arc<Mutex<Vec<u64>>>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.ready.lock().unwrap().push(self.id);
    }
}

/// Clears the `running` flag even if a task panics.
struct Running<'a>(&'a Root);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.inner.tasks.borrow_mut().running = false;
    }
}

/// Polls every task that is ready, until none is. Tasks spawned meanwhile are polled too.
/// A call while this is already running (a task spawning a task) returns at once: the
/// outer loop picks the new task up.
pub(crate) fn run(root: &Root) {
    {
        let mut t = root.inner.tasks.borrow_mut();
        if t.running {
            return;
        }
        t.running = true;
    }
    let _running = Running(root);
    let _entered = root.enter();
    loop {
        let (ids, ready) = {
            let t = root.inner.tasks.borrow();
            let ids = std::mem::take(&mut *t.ready.lock().unwrap());
            (ids, t.ready.clone())
        };
        if ids.is_empty() {
            break;
        }
        for id in ids {
            // Out of the map while it runs, so it may spawn (and so borrow the map).
            let task = root.inner.tasks.borrow_mut().map.remove(&id);
            let Some(mut task) = task else { continue };
            let waker = Waker::from(Arc::new(TaskWaker {
                id,
                ready: ready.clone(),
            }));
            let mut cx = Context::from_waker(&waker);
            if let Poll::Pending = task.as_mut().poll(&mut cx) {
                root.inner.tasks.borrow_mut().map.insert(id, task);
            }
        }
    }
}
