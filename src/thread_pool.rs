use crate::{
    task::{Task, TaskFn, TaskListeners},
    worker::Worker,
    ThreadPoolBuilder,
};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use std::{
    panic::UnwindSafe,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    thread,
    time::Duration,
};

/// A function that used to create a custom thread.
pub type ThreadFactory = dyn Fn() -> thread::Builder + Send + Sync + 'static;

type TPResult<T> = Result<T, TPError>;

/// An error returned from the [`ThreadPool::execute`].
///
/// [`ThreadPool::execute`]: crate::ThreadPool::execute
#[derive(Debug)]
pub enum TPError {
    /// The task could not be executed because the task is rejected
    /// by [`RejectedTaskHandler::Abort`] when the thread pool and the
    /// task channel are both full.
    Abort,

    /// The task could not be executed because the thread pool is closed.
    Closed,
}

impl std::error::Error for TPError {}

impl std::fmt::Display for TPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TPError::Abort => writeln!(f, "task abortion error."),
            TPError::Closed => writeln!(f, "the thread pool is closwd."),
        }
    }
}

/// If a task is rejected, the task will be handled by this.
#[derive(Clone)]
pub enum RejectedTaskHandler {
    /// Returns [`TPError::Abort`].
    Abort,

    /// Nothing to do, just return [`Ok`].
    Discard,

    /// Immediately run the rejected task in the caller thread.
    CallerRuns,
}

pub(crate) struct ThreadPoolSharedData {
    pub(crate) sender: Mutex<Option<Sender<Task>>>,
    pub(crate) core_workers: Mutex<Option<Vec<Worker>>>,
    pub(crate) workers: Mutex<Option<Vec<Worker>>>,
    pub(crate) next_task_id: AtomicUsize,
}

impl ThreadPoolSharedData {
    pub(crate) fn num_of_core_workers(&self) -> usize {
        self.core_workers
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, Vec::len)
    }

    pub(crate) fn num_of_active_workers(&self) -> usize {
        self.workers.lock().unwrap().as_ref().map_or(0, |x| {
            x.iter().filter(|worker| !worker.is_finished()).count()
        })
    }
}

/// A `ThreadPool` consists of a collection of reusable threads and
/// a bounded channel that is used to transfer and hold submitted
/// tasks.
///
/// # Bounded Channel(Queue)
///
/// A bounded channel is a concurrent structure that can help to
/// transfer messages across multiple threads. It holds a finite
/// number of elements, which is useful to prevent resource exhaustion.
///
/// A bounded channel consists of two sides: `Sender` and `Receiver`.
///
/// ## Sender Side
///
/// A sender is used to send a message into a channel. In thread pool,
/// we use the sender to transfer submitted tasks to avalible worker
/// threads.
///
/// ## Receiver Side
///
/// A receiver is used to fetch messages from the channel. There are
/// limited worker threads in a thread pool, for each of which it
/// contains a receiver that is used to fetch tasks from the channel.
///
/// # Worker Thread
///
/// We use a special structure, `Worker`, to represent a thread that
/// is always receiving tasks and executing them. In this library,
/// there are two kinds of the worker:
/// 1. Core worker: The worker thread never be terminated except the
/// associated thread pool is closed and the channel is empty.
/// 2. Non-core worker: A thread in this worker can be idle if no task
/// is received for a certain period of time(`keep_alive_time`).
///
/// This thread pool will store core workers and non-core workers in
/// two vectors. When you execute a task, it creates a core worker to
/// process the task if the core worker vector is not full, otherwise
/// the task will be sent to the task channel. If the channel buffer
/// is full, it attempts to find an idle worker thread or creates a
/// new non-core worker thread to process the task.
///
/// Worker threads will keep fetching tasks from the channel and
/// executing them until the queue is empty and the sender is dropped
/// or no tasks were received for a long time (only non-core thread).
///
/// # Rejected Task
///
/// New tasks will be rejected when the channel is full and the number
/// of worker threads in the thread pool reaches a certain number(`max_pool_size`). A rejected task will be handled by the [`RejectedTaskHandler`].
/// You can set the handler for rejected tasks when you build a thread
/// pool with [`ThreadPoolBuilder`].
#[derive(Clone)]
pub struct ThreadPool {
    pub(crate) reciver: Receiver<Task>,
    pub(crate) share: Arc<ThreadPoolSharedData>,

    pub(crate) core_pool_size: usize,
    pub(crate) max_pool_size: usize,
    pub(crate) keep_alive_time: Duration,
    pub(crate) rejected_task_handler: RejectedTaskHandler,
    pub(crate) task_lisenters: Arc<TaskListeners>,
    pub(crate) thread_factory: Arc<ThreadFactory>,
}

impl ThreadPool {
    /// Builds a thread pool from a configration(builder).
    ///
    /// This assumes arguments of the builder are valid.
    pub(crate) fn from_builder(builder: ThreadPoolBuilder) -> Self {
        let (sender, reciver) = bounded(builder.channel_capacity);
        Self {
            reciver,
            share: Arc::new(ThreadPoolSharedData {
                sender: Mutex::new(Some(sender)),
                core_workers: Mutex::new(Some(Vec::default())),
                workers: Mutex::new(Some(Vec::default())),
                next_task_id: AtomicUsize::new(0),
            }),
            core_pool_size: builder.core_pool_size,
            max_pool_size: builder.max_pool_size,
            keep_alive_time: builder.keep_alive_time,
            rejected_task_handler: builder.rejected_task_handler,
            task_lisenters: Arc::new(builder.task_lisenters),
            thread_factory: builder.thread_factory,
        }
    }

    /// Executes the given task in the future.
    ///
    /// If the task queue is full and no worker thread can be
    /// allocated to execute the task, the task will be handled by the
    /// setted [`RejectedTaskHandler`].
    ///
    /// # Errors
    ///
    /// 1. [`Abort`]: The task was handled by the [`RejectedTaskHandler::Abort`].
    ///
    /// 2. [`Closed`]: The channel was closed.
    ///
    /// [`Abort`]: crate::TPError::Abort
    /// [`Closed`]: crate::TPError::Closed
    pub fn execute<F>(&self, task_fn: F) -> Result<(), TPError>
    where
        F: FnOnce() + Send + UnwindSafe + 'static,
    {
        if self.is_closed() {
            return Err(TPError::Closed);
        }

        let task = self.create_task(Box::new(task_fn));
        let mut core_workers = self.share.core_workers.lock().unwrap();
        if let Some(core_workers) = core_workers.as_mut() {
            // Add a new worker to the core thread pool.
            if core_workers.len() < self.core_pool_size {
                let worker = self.create_worker(task, true);
                core_workers.push(worker);
                return Ok(());
            }
        }
        // Release lock.
        drop(core_workers);
        self.send_task(task)
    }

    /// Counts all active worker threads and returns it.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.share.num_of_active_workers() + self.share.num_of_core_workers()
    }

    /// Closes a thread pool.
    ///
    /// A closed thread pool will not accept any tasks, but will still
    /// process tasks in the channel(queue).
    ///
    /// # Examples
    ///
    /// ```
    /// use jtp::ThreadPoolBuilder;
    /// let thread_pool = ThreadPoolBuilder::default()
    ///     .build()
    ///     .unwrap();
    ///
    /// thread_pool.shutdown();
    ///
    /// assert!(thread_pool.execute(|| {
    ///     println!("Hello");
    /// }).is_err());
    /// ```
    pub fn shutdown(&self) {
        self.share.sender.lock().unwrap().take();
    }

    /// Returns `true` if the thread pool is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.share.sender.lock().unwrap().is_none()
    }

    /// Waits for all worker threads to finish. Note that is worker
    /// threads instead of tasks.
    ///
    /// If this is called in a worker thread, then the worker thread
    /// will not be joined.
    ///
    /// Note that this function will close the thread pool because
    /// if the thread pool is not closed, worker threads are never
    /// be terminated.
    ///
    /// # Errors
    /// An error is returned if a thread panics.
    ///
    /// # Examples
    ///
    /// ```
    /// use jtp::ThreadPoolBuilder;
    /// use std::sync::atomic::{AtomicUsize, Ordering};
    /// use std::sync::Arc;
    ///
    /// let mut thread_pool = ThreadPoolBuilder::default()
    ///     .build()
    ///     .unwrap();
    ///
    /// let sum = Arc::new(AtomicUsize::new(0));
    /// for _ in 0..10 {
    ///     let sum = sum.clone();
    ///     thread_pool.execute(move || {
    ///         // Increase `sum`.
    ///         sum.fetch_add(1, Ordering::SeqCst);
    ///     });
    /// }
    ///
    /// // Block current thread until all worker threads are finished.
    /// thread_pool.wait().unwrap();
    /// assert_eq!(10, sum.load(Ordering::Relaxed));
    /// ```
    pub fn wait(&self) -> std::thread::Result<()> {
        self.shutdown();
        Self::wait_workers(self.share.core_workers.lock().unwrap().take())?;
        Self::wait_workers(self.share.workers.lock().unwrap().take())
    }

    fn wait_workers(workers: Option<Vec<Worker>>) -> std::thread::Result<()> {
        if let Some(workers) = workers {
            for worker in workers {
                worker.join()?;
            }
        }
        Ok(())
    }

    fn create_task(&self, task_fn: TaskFn) -> Task {
        let id = self
            .share
            .next_task_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Task::create(id, task_fn, self.task_lisenters.clone())
    }

    fn create_worker(&self, task: Task, is_core: bool) -> Worker {
        Worker::new(
            is_core,
            self.keep_alive_time,
            self.thread_factory.clone(),
            self.reciver.clone(),
            task,
        )
    }

    fn send_task(&self, task: Task) -> TPResult<()> {
        let sender = self.share.sender.lock().unwrap();
        if sender.is_none() {
            return Err(TPError::Closed);
        }

        if let Err(err) = sender.as_ref().unwrap().try_send(task) {
            // Release lock.
            drop(sender);
            return match err {
                TrySendError::Full(task) => self.process_task_if_channel_full(task),
                TrySendError::Disconnected(_) => Err(TPError::Closed),
            };
        }
        Ok(())
    }

    fn process_task_if_channel_full(&self, task: Task) -> TPResult<()> {
        let mut workers = self.share.workers.lock().unwrap();
        if workers.is_none() {
            // the `wait` function will take workers and close thread
            // pool but the task is accepted.
            return self.reject(task);
        }

        let non_core_workers = workers.as_mut().unwrap();
        // Attempt to find an idle worker.
        let idle_worker = non_core_workers
            .iter_mut()
            .find(|worker| worker.is_finished());
        if let Some(idle_worker) = idle_worker {
            idle_worker.restart(task);
            return Ok(());
        }

        if non_core_workers.len() < self.max_pool_size - self.core_pool_size {
            let worker = self.create_worker(task, false);
            non_core_workers.push(worker);
            Ok(())
        } else {
            // Release lock.
            drop(workers);
            // Reject the task if there is no place in the channel and
            // the size of the thread pool reachs max.
            self.reject(task)
        }
    }

    fn reject(&self, task: Task) -> Result<(), TPError> {
        match &self.rejected_task_handler {
            RejectedTaskHandler::Abort => Err(TPError::Abort),
            RejectedTaskHandler::CallerRuns => {
                task.run();
                Ok(())
            }
            RejectedTaskHandler::Discard => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{RejectedTaskHandler, ThreadPoolBuilder};
    use std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    #[test]
    fn test_execute_in_multiple_threads() {
        let thread_pool = ThreadPoolBuilder::default()
            .core_pool_size(4)
            .max_pool_size(10)
            .channel_capacity(100)
            .keep_alive_time(Duration::from_secs(100))
            .build()
            .unwrap();

        let sum = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..10 {
            let sum = sum.clone();
            let thread_pool = thread_pool.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..10 {
                    let sum = sum.clone();
                    thread_pool
                        .execute(move || {
                            sum.fetch_add(1, Ordering::SeqCst);
                        })
                        .ok();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // check shared data.
        assert!(thread_pool.share.sender.lock().unwrap().is_some());
        assert_eq!(4, thread_pool.share.num_of_core_workers());
        assert!(thread_pool.share.num_of_active_workers() <= 6);

        thread_pool.wait().unwrap();
        assert_eq!(100, sum.load(Ordering::Relaxed));
    }

    #[test]
    fn test_shutdown_in_multiple_threads() {
        let thread_pool = ThreadPoolBuilder::default().build().unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..100 {
            let thread_pool = thread_pool.clone();
            let counter = counter.clone();
            handles.push(thread::spawn(move || {
                if thread_pool.is_closed() {
                    counter.fetch_add(1, Ordering::SeqCst);
                    assert!(thread_pool.execute(|| ()).is_err());
                }
                thread_pool.shutdown();
                assert!(thread_pool.execute(|| ()).is_err());
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(99, counter.load(Ordering::Relaxed));
    }

    #[test]
    fn test_lisenters() {
        let map0 = Arc::new(Mutex::new(HashSet::new()));
        let map1 = map0.clone();
        let map2 = map0.clone();

        let thread_pool = ThreadPoolBuilder::default()
            .lisenter_before_execute(move |id| {
                let mut map = map0.lock().unwrap();
                map.insert(id);
            })
            .lisenter_after_execute(move |id| {
                assert!(map1.lock().unwrap().contains(&id));
            })
            .channel_capacity(50)
            .build()
            .unwrap();

        for _ in 0..50 {
            thread_pool
                .execute(|| {
                    thread::sleep(Duration::from_millis(20));
                })
                .unwrap();
        }
        thread_pool.shutdown();
        thread_pool.wait().unwrap();
        assert_eq!(50, map2.lock().unwrap().len());
    }

    #[test]
    fn test_thread_factory() {
        let thread_pool = ThreadPoolBuilder::new()
            .thread_factory_fn(|| thread::Builder::new().name("test".into()))
            .core_pool_size(2)
            .max_pool_size(5)
            .channel_capacity(5)
            .rejected_handler(RejectedTaskHandler::Discard)
            .build()
            .unwrap();

        for _ in 0..20 {
            thread_pool
                .execute(|| thread::sleep(Duration::from_millis(20)))
                .unwrap();
        }

        let workers = thread_pool.share.core_workers.lock().unwrap();
        assert!(workers.as_ref().unwrap().len() == 2);
        for core_worker in workers.as_ref().unwrap() {
            assert_eq!(Some("test"), core_worker.handle.thread().name());
        }

        let workers = thread_pool.share.workers.lock().unwrap();
        assert!(workers.as_ref().unwrap().len() == 3);
        for worker in workers.as_ref().unwrap() {
            assert_eq!(Some("test"), worker.handle.thread().name());
        }
    }
}
