## JTP

An implementation of a thread pool that is similar to `ThreadPoolExecutor` in java.

## Usage

Install this library using `cargo`,

```
cargo add jtp
```

Or add this to your `Cargo.toml`:

```
[dependencies]
jtp = "0.1.0"
```

And use this library:

```rust
// Creates a thread pool.
let mut thread_pool = ThreadPoolBuilder::default()
	.set_core_pool_size(6) // Sets the number of core threads.
	.set_max_pool_size(10) // Sets the maximum number of threads.
	.set_channel_capacity(100) // Sets the capacity of the task queue.
	.set_rejected_hask_handler(RejectedTaskHandler::Abort)
	.build()
	.unwrap();

for _ in 0..50 {
	// Execute a task in the future.
	thread_pool.execute(|| {
			println!("Hello World");
	});
}
```

## License

Apache License, Version 2.0, [LICENSE-APACHE](http://www.apache.org/licenses/LICENSE-2.0)
