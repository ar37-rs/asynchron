#![deny(unsafe_code)]
use core::sync::atomic::{AtomicBool, Ordering};
use parking_lot::{Condvar, Mutex};
use std::{sync::Arc, thread};
#[derive(Clone)]
/// Result for returned Futurize values where T: OnComplete, E: OnError.
pub enum Futurized<T, E> {
    /// Return complete value immediately if available.
    OnComplete(T),
    /// Return error value immediately if available.
    OnError(E),
    /// Indicates the task is on progress
    OnProgress,
}

#[derive(Clone)]
/// Futurize task asyncronously.
pub struct Futurize<T, E> {
    closure: Arc<dyn Send + Sync + Fn() -> Futurized<T, E>>,
    cvar: Arc<(Mutex<Futurized<T, E>>, Condvar)>,
    awaiting: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl<T, E> Futurize<T, E>
where
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// Create new Futurized task.
    /// # Example:
    ///
    /// ```
    /// use asynchron::{Futurize, Futurized};
    /// use std::time::{Duration, Instant};
    ///
    /// fn main() {
    ///     let instant1: Instant = Instant::now();
    ///     let mut task1_approx_dur = Duration::from_millis(400);
    ///     let task1 = Futurize::task(move || -> Futurized<Vec<u32>, String> {
    ///         std::thread::sleep(task1_approx_dur);
    ///         let elapsed_content = format!(
    ///             "instant 1 enlapsed by now {}",
    ///             instant1.elapsed().subsec_millis()
    ///         );
    ///         // for OnError demo, change the line above like so: (will return error)
    ///         // let elapsed_content = format!("notsparatedbyspace{}", instant1.elapsed().subsec_millis();
    ///         let mut vec_ui32 = Vec::new();
    ///         elapsed_content
    ///             .split(" ")
    ///             .for_each(|ch| match ch.trim().parse::<u32>() {
    ///                 Ok(ui32) => vec_ui32.push(ui32),
    ///                 _ => (),
    ///             });
    ///         if vec_ui32.len() > 0 {
    ///             return Futurized::OnComplete(vec_ui32);
    ///         } else {
    ///             return Futurized::OnError(
    ///                 "task 1 error:
    ///                 unable to parse u32
    ///                 at futurize/src/example.rs line 28\n".to_string(),
    ///             );
    ///         }
    ///     });
    ///     task1.try_wake();
    ///
    ///     let mut task2_approx_dur = Duration::from_millis(600);
    ///     let instant2: Instant = Instant::now();
    ///     let task2 = Futurize::task(move || -> Futurized<u32, ()> {
    ///         std::thread::sleep(task2_approx_dur);
    ///         return Futurized::OnComplete(instant2.elapsed().subsec_millis());
    ///     });
    ///     task2.try_wake();
    ///
    ///     loop {
    ///         if task1.awake() {
    ///             match task1.try_get() {
    ///                 Futurized::OnComplete(value) => {
    ///                     task1_approx_dur = instant1.elapsed();
    ///                     println!(
    ///                         "task 1 completed at: {:?} has value: {:?}\n",
    ///                         task1_approx_dur, value
    ///                     );
    ///                 }
    ///                 Futurized::OnProgress => println!("waiting the task 1 to complete\n"),
    ///                 Futurized::OnError(e) => println!("{}", e),
    ///             }
    ///         }
    ///
    ///         if task2.awake() {
    ///             match task2.try_get() {
    ///                 Futurized::OnComplete(value) => {
    ///                     task2_approx_dur = instant2.elapsed();
    ///                     println!(
    ///                         "task 2 completed at: {:?} has value: {:?}\n",
    ///                         task2_approx_dur, value
    ///                     );
    ///                 }
    ///                 Futurized::OnProgress => println!("waiting the task 2 to complete\n"),
    ///                 _ => (),
    ///             }
    ///         }
    ///         std::thread::sleep(Duration::from_millis(100));
    ///         if !task1.awake() && !task2.awake() {
    ///            break;
    ///         }
    ///     }
    ///     let all_approxed_durs = task2_approx_dur + task1_approx_dur;
    ///     println!(
    ///         "all the tasks are completed {:?} earlier.\n\nOk",
    ///         all_approxed_durs - task2_approx_dur
    ///     )
    /// }
    /// ```
    pub fn task<F>(closure: F) -> Futurize<T, E>
    where
        F: Send + Sync + 'static + Fn() -> Futurized<T, E>,
    {
        Futurize {
            closure: Arc::new(closure),
            cvar: Arc::new((Mutex::new(Futurized::OnProgress), Condvar::new())),
            awaiting: Arc::new(AtomicBool::new(false)),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Try (it won't block current thread) wake the task and then try get later somewhere.
    #[inline]
    pub fn try_wake(&self) {
        let awaiting = &*self.awaiting;
        if !awaiting.load(Ordering::SeqCst) {
            awaiting.store(true, Ordering::SeqCst);
            let ready = Arc::clone(&self.ready);
            let closure = Arc::clone(&self.closure);
            let cvar = Arc::clone(&self.cvar);
            thread::spawn(move || {
                let result = closure();
                let (mtx, cvar) = &*cvar;
                let mut mtx = mtx.lock();
                *mtx = result;
                ready.store(true, Ordering::SeqCst);
                cvar.notify_one();
            });
        }
    }

    /// Wait until the task is completed and then get (careful, this is blocking operation).
    #[inline]
    pub fn get(&self) -> Futurized<T, E> {
        self.try_wake();
        let (mtx, cvar) = &*self.cvar;
        let mut mtx = mtx.lock();
        cvar.wait(&mut mtx);
        let awaiting = &*self.awaiting;
        let ready = &*self.ready;
        let result = mtx.clone();
        *mtx = Futurized::OnProgress;
        ready.store(false, Ordering::SeqCst);
        awaiting.store(false, Ordering::SeqCst);
        result
    }

    /// Try get if the task is awake, it won't block current thread (non-blocking).
    #[inline]
    pub fn try_get(&self) -> Futurized<T, E> {
        let awaiting = &*self.awaiting;
        if awaiting.load(Ordering::SeqCst) {
            let ready = &*self.ready;
            if ready.load(Ordering::SeqCst) {
                ready.store(false, Ordering::SeqCst);
                let mtx = &self.cvar.0;
                let mut mtx = mtx.lock();
                let result = mtx.clone();
                *mtx = Futurized::OnProgress;
                awaiting.store(false, Ordering::SeqCst);
                result
            } else {
                Futurized::OnProgress
            }
        } else {
            self.try_wake();
            Futurized::OnProgress
        }
    }

    /// Check if the task is awake.
    #[inline(always)]
    pub fn awake(&self) -> bool {
        let awaiting = &*self.awaiting;
        awaiting.load(Ordering::Relaxed)
    }
}
