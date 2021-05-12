# Asynchron

[![Crates.io](https://img.shields.io/crates/v/asynchron.svg)](https://crates.io/crates/asynchron)
[![Asynchron documentation](https://docs.rs/asynchron/badge.svg)](https://docs.rs/asynchron)
[![Rust](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml/badge.svg)](https://github.com/Ar37-rs/asynchron/actions/workflows/rust.yml)

Asynchronize blocking operation.

## Example

```rust
use asynchron::{Futurize, Futurized};
use std::time::{Duration, Instant};

fn main() {
    let instant1: Instant = Instant::now();
    let mut task1_approx_dur = Duration::from_millis(400);
    let task1 = Futurize::task(move || -> Futurized<Vec<u32>, String> {
        std::thread::sleep(task1_approx_dur);
        let elapsed_content = format!(
            "instant 1 enlapsed by now {}",
            instant1.elapsed().subsec_millis()
        );
        // for OnError demo, change the line above like so: (will return error)
        // let elapsed_content = format!("notsparatedbyspace{}", instant1.elapsed().subsec_millis());
        let mut vec_ui32 = Vec::new();
        elapsed_content
            .split(" ")
            .for_each(|ch| match ch.trim().parse::<u32>() {
                Ok(ui32) => vec_ui32.push(ui32),
                _ => (),
            });
        if vec_ui32.len() > 0 {
            return Futurized::OnComplete(vec_ui32);
        } else {
            return Futurized::OnError(
                "task 1 error:
                unable to parse u32
                at futurize/src/example.rs line 28\n"
                    .to_string(),
            );
        }
    });
    task1.try_wake();

    let mut task2_approx_dur = Duration::from_millis(600);
    let instant2: Instant = Instant::now();
    let task2 = Futurize::task(move || -> Futurized<u32, ()> {
        std::thread::sleep(task2_approx_dur);
        return Futurized::OnComplete(instant2.elapsed().subsec_millis());
    });
    task2.try_wake();

    loop {
        if task1.awake() {
            match task1.try_get() {
                Futurized::OnComplete(value) => {
                    task1_approx_dur = instant1.elapsed();
                    println!(
                        "task 1 completed at: {:?} has value: {:?}\n",
                        task1_approx_dur, value
                    );
                }
                Futurized::OnProgress => println!("waiting the task 1 to complete\n"),
                Futurized::OnError(e) => println!("{}", e),
            }
        }

        if task2.awake() {
            match task2.try_get() {
                Futurized::OnComplete(value) => {
                    task2_approx_dur = instant2.elapsed();
                    println!(
                        "task 2 completed at: {:?} has value: {:?}\n",
                        task2_approx_dur, value
                    );
                }
                Futurized::OnProgress => println!("waiting the task 2 to complete\n"),
                _ => (),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
        if !task1.awake() && !task2.awake() {
            break;
        }
    }
    let all_approxed_durs = task2_approx_dur + task1_approx_dur;
    println!(
        "all the tasks are completed {:?} earlier.\n\nOk",
        all_approxed_durs - task2_approx_dur
    )
}
```

## More Example

mixing sync and async can be found [here](https://github.com/Ar37-rs/asynchron/tree/main/example).
