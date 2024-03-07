use std::{sync::Arc, thread, time::Duration};

use crate::{task::TaskListeners, RejectedTaskHandler, ThreadFactory, ThreadPool};

/// A builder of the [`ThreadPool`], which can be used to configure
/// the properties of a new thread pool.
///
/// # Examples
///
/// ```
/// use jtp::ThreadPoolBuilder;
/// use jtp::RejectedTaskHandler;
/// use std::time::Duration;
///
/// let thread_pool = ThreadPoolBuilder::default()
///     .core_pool_size(4)
///     .max_pool_size(7)
///     .keep_alive_time(Duration::from_secs(2))
///     .rejected_handler(RejectedTaskHandler::Discard)
///     .lisenter_before_execute(|id| println!("the task {} will be executed.", id))
///     .lisenter_before_execute(|id| println!("the task {} has been executed.", id))
///     .thread_factory_fn(|| {
///         std::thread::Builder::new().stack_size(1024 * 4)
///     })
///     .build();
/// ```
pub struct ThreadPoolBuilder {
    pub(crate) channel_capacity: usize,
    pub(crate) max_pool_size: usize,
    pub(crate) core_pool_size: usize,
    pub(crate) keep_alive_time: Duration,
    pub(crate) rejected_task_handler: RejectedTaskHandler,
    pub(crate) task_lisenters: TaskListeners,
    pub(crate) thread_factory: Arc<ThreadFactory>,
}

impl Default for ThreadPoolBuilder {
    /// Creates a new builder with the default configuration.
    ///
    /// # Default Configuration
    /// - `channel_capacity`: 1000
    /// - `max_pool_size`: the number of physical cores of the current
    /// system
    /// - `core_pool_size`: the half of the `max_pool_size` at least 1
    /// - `keep_alive_time`: 1 second
    /// - `rejected_task_handler`: [`RejectedTaskHandler::Abort`]
    /// - `before_execute`: an empty closure `|_| ()`
    /// - `after_execute`: an empty closure `|_| ()`
    /// - `thread_factory`: `|| thread::Builder::new()`
    fn default() -> Self {
        Self {
            channel_capacity: 1000,
            max_pool_size: num_cpus::get_physical(),
            core_pool_size: usize::max(1, num_cpus::get_physical() / 2),
            keep_alive_time: Duration::from_secs(1),
            rejected_task_handler: RejectedTaskHandler::Abort,
            task_lisenters: TaskListeners {
                before_execute: Box::new(|_| {}),
                after_execute: Box::new(|_| {}),
            },
            thread_factory: Arc::new(thread::Builder::new),
        }
    }
}

impl ThreadPoolBuilder {
    /// Creates the base configuration for the new thread pool.
    ///
    /// See: [`ThreadPoolBuilder::default`]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the capacity of the bounded task channel.
    #[must_use]
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    /// Sets the maximum allowed number of threads.
    #[must_use]
    pub fn max_pool_size(mut self, size: usize) -> Self {
        self.max_pool_size = size;
        self
    }

    /// Sets the number of core threads.
    #[must_use]
    pub fn core_pool_size(mut self, size: usize) -> Self {
        self.core_pool_size = size;
        self
    }

    /// Sets the time that non-core threads may remain idle.
    #[must_use]
    pub fn keep_alive_time(mut self, time: Duration) -> Self {
        self.keep_alive_time = time;
        self
    }

    /// Sets the policy to handle rejected tasks.
    ///
    /// When you execute a task, it will be handled by the given
    /// `handler` if the thread pool and the task channel are both
    /// full.
    #[must_use]
    pub fn rejected_handler(mut self, handler: RejectedTaskHandler) -> Self {
        self.rejected_task_handler = handler;
        self
    }

    /// Sets the listener function that will be invoked before a task
    /// is executed.
    #[must_use]
    pub fn lisenter_before_execute<F>(mut self, executor: F) -> Self
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.task_lisenters.before_execute = Box::new(executor);
        self
    }

    /// Sets the listener function that will be invoked after a task
    /// is executed.
    #[must_use]
    pub fn lisenter_after_execute<F>(mut self, executor: F) -> Self
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.task_lisenters.after_execute = Box::new(executor);
        self
    }

    /// Sets the factory function that is used to create a new custom
    /// thread.
    #[must_use]
    pub fn thread_factory_fn<F>(mut self, f: F) -> Self
    where
        F: Fn() -> thread::Builder + Send + Sync + 'static,
    {
        self.thread_factory = Arc::new(f);
        self
    }

    /// Creates a thread pool with the arguments.
    ///
    /// # Panics
    ///
    /// Panics if the builder with invalid arguments.
    pub fn build(self) -> ThreadPool {
        self.check_arguments();
        ThreadPool::from_builder(self)
    }

    fn check_arguments(&self) {
        if self.channel_capacity < self.max_pool_size {
            panic!("max_tasks must < max_pool_size.",);
        }

        if self.max_pool_size < self.core_pool_size {
            panic!("max_pool_size must < core_pool_size.",);
        }

        if self.core_pool_size == 0 {
            panic!("core_pool_size can not be 0.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ThreadPoolBuilder;

    #[test]
    #[should_panic]
    fn test_builder_args1() {
        ThreadPoolBuilder::default()
            .max_pool_size(7)
            .channel_capacity(3)
            .build();
    }

    #[test]
    #[should_panic]
    fn test_builder_args2() {
        ThreadPoolBuilder::default()
            .max_pool_size(6)
            .core_pool_size(7)
            .build();
    }

    #[test]
    #[should_panic]
    fn test_builder_args3() {
        ThreadPoolBuilder::default().core_pool_size(0).build();
    }
}
