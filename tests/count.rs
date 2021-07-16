use asynchron::{
    Futurize,
    Progress,
};
use std::io::Result;

#[test]
fn count() -> Result<()> {
    let closure = Futurize::task(0, move |_| -> Progress <i32, ()> {
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
