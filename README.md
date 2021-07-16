# Asynchron

[![Crates.io](https://img.shields.io/crates/v/asynchron.svg)](https://crates.io/crates/asynchron)
[![Asynchron documentation](https://docs.rs/asynchron/badge.svg)](https://docs.rs/asynchron)
[![Rust](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml/badge.svg)](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml)

Asynchronize blocking operation.

## Example

```rust
use asynchron::{Futurize, Progress};
use std::time::{Duration, Instant};

fn main() {
    let instant: Instant = Instant::now();

    let mut tasks = Vec::new();

    for i in 0..5 {
        let task = Futurize::task(i, move |cancel| -> Progress<u32, ()> {
            let millis = i + 1;
            let sleep_dur = Duration::from_millis((60 * millis).into());
            std::thread::sleep(sleep_dur);
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Progress::Canceled;
            } else {
                return Progress::Completed(instant.elapsed().subsec_millis());
            }
        });
        tasks.push(task)
    }

    for task in tasks.iter() {
        task.try_do()
    }

    let mut task_count = tasks.len();
    loop {
        for task in tasks.iter() {
            if task.is_in_progress() {
                match task.try_get() {
                    Progress::Current => {
                        if task.task_id() == 0 || task.task_id() == 3 {
                            task.cancel()
                        }
                        println!("task with id: {} is trying to be done\n", task.task_id())
                    }
                    Progress::Canceled =>  println!("task with id: {} is canceled\n", task.task_id()),
                    Progress::Completed(elapsed) => println!(
                        "task with id: {} elapsed at: {:?} milliseconds\n",task.task_id(), elapsed
                    ),
                    _ => (),
                }

                if task.is_done() {
                    task_count -= 1
                }
            }
        }

        if task_count == 0 {
            println!("all the tasks are done.");
            break;
        }
        std::thread::sleep(Duration::from_millis(50))
    }
}
```

## More Example

Compact example with egui [here](https://github.com/Ar37-rs/egui-extras-lib/tree/main/example),

Mixing sync and async with tokio, reqwest and fltk-rs can be found [here](https://github.com/Ar37-rs/asynchron/tree/main/example).
