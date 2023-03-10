use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::Receiver;

use crate::{task::Task, ThreadFactory};

/// A worker holds a thread handler and a receiver.
pub(crate) struct Worker {
    pub(crate) handle: JoinHandle<()>,
    pub(crate) receiver: Receiver<Task>,
    pub(crate) keep_alive_time: Duration,
    pub(crate) thread_factory: Arc<ThreadFactory>,
}

fn create_core_work_thread(
    thread_factory: Arc<ThreadFactory>,
    receiver: Receiver<Task>,
    task: Task,
) -> JoinHandle<()> {
    let builder = thread_factory();
    builder
        .spawn(move || {
            task.run();
            while let Ok(task) = receiver.recv() {
                task.run();
            }
        })
        .expect("failed to spawn a thread.")
}

fn create_work_thread(
    thread_factory: Arc<ThreadFactory>,
    receiver: Receiver<Task>,
    keep_alive_time: Duration,
    task: Task,
) -> JoinHandle<()> {
    let builder = thread_factory();
    builder
        .spawn(move || {
            task.run();
            while let Ok(task) = receiver.recv_timeout(keep_alive_time) {
                task.run();
            }
        })
        .expect("failed to spawn a thread.")
}

impl Worker {
    #[must_use]
    pub fn new(
        is_core: bool,
        keep_alive_time: Duration,
        thread_factory: Arc<ThreadFactory>,
        receiver: Receiver<Task>,
        task: Task,
    ) -> Self {
        Worker {
            keep_alive_time,
            thread_factory: thread_factory.clone(),
            receiver: receiver.clone(),
            handle: if is_core {
                create_core_work_thread(thread_factory, receiver, task)
            } else {
                create_work_thread(thread_factory, receiver, keep_alive_time, task)
            },
        }
    }

    /// Creates a new thread for the worker. This will make the `self`
    /// worker available again.
    pub(crate) fn restart(&mut self, task: Task) {
        debug_assert!(self.is_finished());
        self.handle = create_work_thread(
            self.thread_factory.clone(),
            self.receiver.clone(),
            self.keep_alive_time,
            task,
        );
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    #[inline]
    pub(crate) fn join(self) -> thread::Result<()> {
        if self.handle.thread().id() != thread::current().id() {
            self.handle.join()?
        }
        Ok(())
    }
}
