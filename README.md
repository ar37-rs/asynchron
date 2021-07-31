# Asynchron

[![Crates.io](https://img.shields.io/crates/v/asynchron.svg)](https://crates.io/crates/asynchron)
[![Asynchron documentation](https://docs.rs/asynchron/badge.svg)](https://docs.rs/asynchron)
[![Rust](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml/badge.svg)](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml)

Asynchronize blocking operation.

## Example

```rust
use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::time::{Duration, Instant};

fn main() {
    let instant: Instant = Instant::now();
    let task: Futurized<String, u32, String> = Futurize::task(
        0,
        move |_task: ITaskHandle<String>| -> Progress<String, u32, String> {
            let sleep_dur = Duration::from_millis(10);
            std::thread::sleep(sleep_dur);
            let value = format!("The task wake up from sleep");
            // Send current task progress.
            // let _ = _task.send(value); (to ignore sender error in some specific cases if needed).
            if let Err(e) = _task.send(value) {
                // Return error immedietly
                // !WARNING!
                // if always ignoring error,
                // Undefined Behavior there's always a chance to occur and hard to debug,
                // always return error for safety in many cases (recommended), rather than unwrapping.
                return Progress::Error(format!(
                    "Progress error while sending state: {}",
                    e.to_string(),
                ));
            }

            if _task.is_canceled() {
                let _ = _task.send("Canceling the task".into());
                Progress::Canceled
            } else {
                Progress::Completed(instant.elapsed().subsec_millis())
            }
        },
    );

    // Try do the task now.
    task.try_do();

    loop {
        if task.is_in_progress() {
            match task.try_get() {
                Progress::Current(task_receiver) => {
                    if let Some(value) = task_receiver {
                        println!("{}\n", value)
                    }
                    // Cancel if need to.
                    // task.cancel();
                }
                Progress::Canceled => {
                    println!("The task was canceled\n")
                }
                Progress::Completed(elapsed) => {
                    println!("The task finished in: {:?} milliseconds\n", elapsed)
                }
                Progress::Error(e) => {
                    println!("{}\n", e)
                }
            }

            if task.is_done() {
                break;
            }
        }
    }
}
```

## More Example

Mixing sync and async with tokio, reqwest and fltk-rs can be found [here](https://github.com/Ar37-rs/asynchron/tree/main/example).
