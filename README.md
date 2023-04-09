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
jtp = "0.1.1"
```

And use this library:

```rust
// Creates a thread pool.
let thread_pool = ThreadPoolBuilder::default()
	.core_pool_size(6) // Sets the number of core threads.
	.max_pool_size(10) // Sets the maximum number of threads.
	.channel_capacity(100) // Sets the capacity of the task queue.
	.rejected_handler(RejectedTaskHandler::Abort)
	.build()
	.unwrap();

thread_pool.execute(|| println!("Hello World"));
thread_pool.wait();
```

## License

Apache License, Version 2.0, [LICENSE-APACHE](http://www.apache.org/licenses/LICENSE-2.0)
