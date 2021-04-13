use asynchron::{
    Futurize,
    Futurized::{self, OnComplete},
};
use std::io::Result;

#[test]
fn count() -> Result<()> {
    let closure = Futurize::task(move || -> Futurized<i32, ()> {
        let mut counter = 0;
        counter += 1;
        return OnComplete(counter);
    });
    closure.try_wake();
    loop {
        if closure.awake() {
            match closure.try_get() {
                Futurized::OnComplete(counter) => {
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
