use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
use std::io::Result;

#[test]
fn drive() -> Result<()> {
    let closure: Futurized<usize, i32, ()> = Futurize::task(
        0,
        move |_task: ITaskHandle<usize>| -> Progress<_, i32, ()> {
            for i in 0..5 {
                _task.send(i)
            }
            let mut counter = 0;
            counter += 1;
            return Progress::Completed(counter);
        },
    );
    closure.try_do();
    let mut sum = 0;
    loop {
        if closure.is_in_progress() {
            match closure.try_get() {
                Progress::Current(receiver) => {
                    if let Some(val) = receiver {
                        sum += val;
                    }
                }
                Progress::Completed(counter) => {
                    println!("\nsum value: {}\n", sum);
                    assert_eq!(sum, 10);
                    println!("counter value: {}\n", counter);
                    assert_eq!(counter, 1);
                    break;
                }
                _ => (),
            }
        }
    }
    Ok(())
}
