use std::sync::Arc;

pub type TaskFn = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct TaskListeners {
    pub(crate) before_execute: Box<dyn Fn(usize) + Send + Sync>,
    pub(crate) after_execute: Box<dyn Fn(usize) + Send + Sync>,
}

pub struct Task {
    id: usize,
    task_fn: TaskFn,
    listeners: Arc<TaskListeners>,
}

impl Task {
    pub(crate) fn create(id: usize, task_fn: TaskFn, listeners: Arc<TaskListeners>) -> Self {
        Self {
            id,
            task_fn,
            listeners,
        }
    }

    pub(crate) fn run(self) {
        let listeners = self.listeners;
        (listeners.before_execute)(self.id);
        (self.task_fn)();
        (listeners.after_execute)(self.id);
    }
}
