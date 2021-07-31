use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::time::{Duration, Instant};

fn main() {
    let instant: Instant = Instant::now();
    let mut vec_opt_tasks = Vec::new();
    for i in 0..5 {
        let task: Futurized<String, u32, String> = Futurize::task(
            i,
            move |_task: ITaskHandle<String>| -> Progress<String, u32, String> {
                let millis = _task.id() + 1;
                let sleep_dur = Duration::from_millis((10 * millis) as u64);
                std::thread::sleep(sleep_dur);
                let value = format!("The task with id: {} wake up from sleep", _task.id());
                // Send current task progress.
                // let _ = _task.send(value); (to ignore sender error in some specific cases if needed).
                if let Err(err) = _task.send(value) {
                    // Return error immediately
                    // !WARNING!
                    // if always ignoring error,
                    // Undefined Behavior there's always a chance to occur and hard to debug,
                    // always return error for safety in many cases (recommended), rather than unwrapping.
                    return Progress::Error(format!(
                        "The task with id: {} progress error while sending state: {}",
                        _task.id(),
                        err.to_string(),
                    ));
                }

                if _task.is_canceled() {
                    let _ = _task.send(format!("Canceling the task with id: {}", _task.id()));
                    Progress::Canceled
                } else {
                    Progress::Completed(instant.elapsed().subsec_millis())
                }
            },
        );
        // Try do the task now.
        task.try_do();
        vec_opt_tasks.push(Some(task))
    }

    let mut count_down = vec_opt_tasks.len();

    loop {
        for i in 0..vec_opt_tasks.len() {
            if let Some(task) = &vec_opt_tasks[i] {
                if task.is_in_progress() {
                    match task.try_get() {
                        Progress::Current(task_receiver) => {
                            if let Some(value) = task_receiver {
                                println!("{}\n", value)
                            }
                            // Cancel if need to.
                            // if (task.id() % 2 != 0) || (task.id() == 0) {
                            //     task.cancel()
                            // }
                        }
                        Progress::Canceled => {
                            println!("The task with id: {} was canceled\n", task.id())
                        }
                        Progress::Completed(elapsed) => {
                            println!(
                                "The task with id: {} finished in: {:?} milliseconds\n",
                                task.id(),
                                elapsed
                            )
                        }
                        Progress::Error(err) => {
                            println!("{}", err)
                        }
                    }

                    if task.is_done() {
                        vec_opt_tasks[i] = None;
                        count_down -= 1;
                    }
                }
            }
        }

        if count_down == 0 {
            break;
        }
    }
}
