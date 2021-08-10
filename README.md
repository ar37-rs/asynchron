# Asynchron

[![Crates.io](https://img.shields.io/crates/v/asynchron.svg)](https://crates.io/crates/asynchron)
[![Asynchron documentation](https://docs.rs/asynchron/badge.svg)](https://docs.rs/asynchron)
[![CI](https://github.com/Ar37-rs/asynchron/actions/workflows/ci.yml/badge.svg)](https://github.com/Ar37-rs/asynchron/actions/workflows/ci.yml)

Asynchronize blocking operation.

## Example

```rust
use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::{
    io::Error,
    time::{Duration, Instant},
};

fn main() {
    let instant: Instant = Instant::now();
    let task: Futurized<String, u32> = Futurize::task(
        0,
        move |_task: ITaskHandle<String>| -> Progress<String, u32> {
            // // Panic if need to.
            // // will return Error with a message:
            // // "the task with id: (specific task id) panicked!"
            // std::panic::panic_any("loudness");
            let sleep_dur = Duration::from_millis(10);
            std::thread::sleep(sleep_dur);
            let result = Ok::<String, Error>(
                format!("The task with id: {} wake up from sleep", _task.id()).into(),
            );
            match result {
                Ok(value) => {
                    // Send current task progress.
                    _task.send(value)
                }
                Err(e) => {
                    // Return error immediately if something not right, for example:
                    return Progress::Error(e.to_string().into());
                }
            }

            if _task.is_canceled() {
                _task.send("Canceling the task".into());
                return Progress::Canceled;
            }
            Progress::Completed(instant.elapsed().subsec_millis())
        },
    );

    // Try do the task now.
    task.try_do();

    let mut exit = false;
    loop {
        task.try_resolve(|progress, done| {
            match progress {
                Progress::Current(task_receiver) => {
                    if let Some(value) = task_receiver {
                        println!("{}\n", value)
                    }
                    // // Cancel if need to.
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

            if done {
                // This scope act like "finally block", do final things here.
                exit = true
            }
        });

        if exit {
            break;
        }
    }
}
```

## More Example

Mixing sync and async with tokio, reqwest and fltk-rs can be found [here](https://github.com/Ar37-rs/asynchron/tree/main/example).
