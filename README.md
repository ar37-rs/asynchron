# Asynchron

[![Crates.io](https://img.shields.io/crates/v/asynchron.svg)](https://crates.io/crates/asynchron)
[![Asynchron documentation](https://docs.rs/asynchron/badge.svg)](https://docs.rs/asynchron)
[![Rust](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml/badge.svg)](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml)

Asynchronize blocking operation.

## Example

```rust
use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::{
    convert::Infallible,
    time::{Duration, Instant},
};

fn main() {
    let instant: Instant = Instant::now();
    let task: Futurized<String, u32, String> = Futurize::task(
        0,
        move |_task: ITaskHandle<String>| -> Progress<String, u32, String> {
            let sleep_dur = Duration::from_millis(10);
            std::thread::sleep(sleep_dur);
            // Send current task progress.
            let result = Ok::<String, Infallible>("The task  wake up from sleep.".into());
            if let Ok(value) = result {
                _task.send(value);
            } else {
                // return error immediately if something not right, for example:
                return Progress::Error(
                    "Something ain't right..., programmer out of bounds.".into(),
                );
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
                    // task.cancel()
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
