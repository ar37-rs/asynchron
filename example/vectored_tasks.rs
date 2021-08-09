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
                let result = Ok::<String, ()>(format!(
                    "The task with id: {} wake up from sleep",
                    _task.id()
                ));
                // Send current task progress.
                if let Ok(value) = result {
                    _task.send(value);
                } else {
                    // return error immediately if something not right, for example:
                    return Progress::Error(
                        "Something ain't right..., programmer out of bounds.".into(),
                    );
                }

                if _task.is_canceled() {
                    let value = format!("Canceling the task with id: {}", _task.id());
                    _task.send(value);
                    return Progress::Canceled;
                }
                Progress::Completed(instant.elapsed().subsec_millis())
            },
        );
        // Try do the task now.
        task.try_do();
        vec_opt_tasks.push(Some(task))
    }

    let num_tasks = vec_opt_tasks.len();
    let mut count_down = num_tasks;

    loop {
        for i in 0..num_tasks {
            if let Some(task) = &vec_opt_tasks[i] {
                task.try_resolve(|progress, _| match progress {
                    Progress::Current(task_receiver) => {
                        if let Some(value) = task_receiver {
                            println!("{}\n", value)
                        }
                        // // Cancel if need to.
                        // if (task.id() % 2 != 0) || (task.id() == 0) {
                        //     task.cancel()
                        // }

                        // // or terminate if need to.
                        // // change the line above like so: "if let Some(task) = vec_opt_tasks[i].clone() {..."
                        // // and then simply set some items of vec_opt_tasks to None.
                        // if (task.id() % 2 != 0) || (task.id() == 0) {
                        //     vec_opt_tasks[i] = None;
                        //     count_down -= 1
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
                });

                if task.is_done() {
                    vec_opt_tasks[i] = None;
                    count_down -= 1;
                }
            }
        }

        if count_down == 0 {
            break;
        }
    }
}
