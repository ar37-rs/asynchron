use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::io::Result;

#[test]
fn drive() -> Result<()> {
    let task: Futurized<usize, i32> =
        Futurize::task(0, move |_task: ITaskHandle<usize>| -> Progress<_, i32> {
            for i in 0..5 {
                _task.send(i)
            }
            let mut counter = 0;
            counter += 1;
            Progress::Completed(counter)
        });
    task.try_do();
    let mut sum = 0;
    let mut exit = false;
    loop {
        task.try_resolve(|progress, done| {
            match progress {
                Progress::Current(receiver) => {
                    if let Some(val) = receiver {
                        sum += val;
                    }
                }
                Progress::Completed(counter) => {
                    println!("\ncounter value: {}", counter);
                    assert_eq!(counter, 1);
                }
                _ => (),
            }

            if done {
                println!("sum value: {}\n", sum);
                assert_eq!(sum, 10);
                exit = true
            }
        });

        if exit {
            break;
        }
    }
    Ok(())
}
