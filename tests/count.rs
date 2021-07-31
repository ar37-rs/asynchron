use asynchron::{
    Futurize,
    Futurized,
    Progress,
    InnerTaskHandle,
};
use std::io::Result;

#[test]
fn count() -> Result<()> {
    let closure: Futurized<(), i32, ()> = Futurize::task(0, move |_task: InnerTaskHandle| -> Progress <_,i32, ()> {
        let mut counter = 0;
        counter += 1;
        return Progress::Completed(counter);
    });
    closure.try_do();
    loop {
        if closure.is_in_progress() {
            match closure.try_get() {
                Progress::Completed(counter) => {
                    println!("counter value {}", counter);
                    assert_eq!(counter, 1);
                    break;
                }
                _ => (),
            }
        }
    }
    Ok(())
}
