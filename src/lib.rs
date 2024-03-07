//! # Thread Pool
//!
//! A thread pool allows you to execute asyncronous tasks without
//! creating new threads for each one. It improves the performance and
//! the resource utilization by limiting the number of threads.
//!
//! # Build a thread pool
//!
//! You can use the [`ThreadPoolBuilder`] to build a thread pool with
//! the custom configuration.
//!
//! # Examples
//!
//! ```
//! use jtp::ThreadPoolBuilder;
//! let mut thread_pool = ThreadPoolBuilder::default()
//!     .core_pool_size(5)
//!     .max_pool_size(10)
//!     .channel_capacity(100)
//!     .build();
//!
//! thread_pool.execute(|| println!("Hello World")).unwrap();
//!
//! // Close the thread pool and wait for all worker threads to end.
//! thread_pool.wait();
//! ```

mod builder;
mod thread_pool;

pub(crate) mod task;
pub(crate) mod worker;

pub use builder::*;
pub use thread_pool::*;
