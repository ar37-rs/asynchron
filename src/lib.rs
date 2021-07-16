#![deny(unsafe_code)]
use cfg_if::cfg_if;
use core::sync::atomic::{AtomicBool, Ordering};
use parking_lot::{Condvar, Mutex};
use std::{sync::Arc, thread};

#[derive(Clone, Debug)]
/// Simple helper thread safe state management (using Arc<(Mutex<T>, Condvar)> under the hood).
pub struct State<T> {
    item: Arc<(Mutex<T>, Condvar)>,
}

impl<T: Clone + Send + Sync + ?Sized> State<T> {
    /// Create new state.
    pub fn new(item: T) -> State<T> {
        State {
            item: Arc::new((Mutex::new(item), Condvar::new())),
        }
    }

    /// Async set state.
    pub async fn async_set(&self, item: T) {
        let (mtx, cvar) = &*self.item;
        let mut mtx = mtx.lock();
        *mtx = item;
        cvar.notify_one();
    }

    /// Async get state.
    pub async fn async_get(&self) -> T {
        let (mtx, cvar) = &*self.item;
        if mtx.is_locked() {
            let mut mtx = mtx.lock();
            cvar.wait(&mut mtx);
            mtx.clone()
        } else {
            let mtx = mtx.lock();
            mtx.clone()
        }
    }

    /// Set state.
    pub fn set(&self, item: T) {
        let (mtx, cvar) = &*self.item;
        let mut mtx = mtx.lock();
        *mtx = item;
        cvar.notify_one();
    }

    /// Get state.
    pub fn get(&self) -> T {
        let (mtx, cvar) = &*self.item;
        if mtx.is_locked() {
            let mut mtx = mtx.lock();
            cvar.wait(&mut mtx);
            mtx.clone()
        } else {
            let mtx = mtx.lock();
            mtx.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Result for returned Futurize values where T: Completed, E: Error.
pub enum Progress<T, E> {
    /// Current progress of the task
    Current,
    /// Indicates if the task is canceled
    Canceled,
    Completed(T),
    Error(E),
}

cfg_if! {
    if #[cfg(feature = "blocking")] {
        #[derive(Clone)]
        /// Futurize task asynchronously.
        pub struct Futurize<T, E> {
            id: u32,
            closure: Arc<dyn Send + Sync + Fn(Arc<AtomicBool>) -> Progress<T, E>>,
            states: Arc<(AtomicBool, AtomicBool, Mutex<Progress<T, E>>, Condvar)>,
            _cancel: Arc<AtomicBool>
        }
    } else {
        #[derive(Clone)]
        /// Futurize task asynchronously.
        /// # Example:
        ///
        ///```
        /// use asynchron::{Futurize, Progress};
        /// use std::time::{Duration, Instant};
        ///
        /// fn main() {
        ///     let instant: Instant = Instant::now();
        ///
        ///     let mut tasks = Vec::new();
        ///
        ///     for i in 0..5 {
        ///         let task = Futurize::task(i, move |cancel| -> Progress<u32, ()> {
        ///             let millis = i + 1;
        ///             let sleep_dur = Duration::from_millis((60 * millis).into());
        ///             std::thread::sleep(sleep_dur);
        ///             if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        ///                 return Progress::Canceled;
        ///             } else {
        ///                 return Progress::Completed(instant.elapsed().subsec_millis());
        ///             }
        ///         });
        ///         tasks.push(task);
        ///     }
        ///
        ///     for task in tasks.iter() {
        ///         task.try_do()
        ///     }
        ///
        ///     let mut task_count = tasks.len();
        ///     loop {
        ///         for task in tasks.iter() {
        ///             if task.is_in_progress() {
        ///                 match task.try_get() {
        ///                     Progress::Current => {
        ///                         if task.task_id() == 0 || task.task_id() == 3 {
        ///                             task.cancel();
        ///                         }
        ///                         println!("task with id: {} is trying to be done\n", task.task_id());
        ///                     }
        ///                     Progress::Canceled =>  println!("task with id: {} is canceled\n", task.task_id()),
        ///                     Progress::Completed(elapsed) => {
        ///                         println!(
        ///                             "task with id: {} elapsed at: {:?} milliseconds\n",
        ///                             task.task_id(), elapsed
        ///                         );
        ///                     }
        ///                     _ => (),
        ///                 }
        ///
        ///                 if task.is_done() {
        ///                     task_count -= 1;
        ///                 }
        ///             }
        ///         }
        ///
        ///         if task_count == 0 {
        ///             println!("all the tasks are done.");
        ///             break;
        ///         }
        ///         std::thread::sleep(Duration::from_millis(50));
        ///     }
        /// }
        ///```
        pub struct Futurize<T, E> {
            id: u32,
            closure: Arc<dyn Send + Sync + Fn(Arc<AtomicBool>) -> Progress<T, E>>,
            states: Arc<(AtomicBool, AtomicBool, Mutex<Progress<T, E>>)>,
            _cancel: Arc<AtomicBool>
        }
    }
}

impl<T, E> Futurize<T, E>
where
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// Create new task.
    /// # Other Example:
    ///
    /// ```
    /// use asynchron::{Futurize, Progress};
    /// use std::{
    ///     time::{Duration, Instant},
    /// };
    ///
    /// fn main() {
    ///     let instant1: Instant = Instant::now();
    ///     let mut task1_approx_dur = Duration::from_millis(400);
    ///
    ///     let task1 = Futurize::task(0, move |_canceled| {
    ///         std::thread::sleep(task1_approx_dur);
    ///         let elapsed_content = format!(
    ///            "instant 1 enlapsed by now {}",
    ///             instant1.elapsed().subsec_millis()
    ///         );
    ///         // for Error demo, change the line above like so: (will return error)
    ///         //
    ///         // let elapsed_content = format!("notsparatedbyspace{}", instant1.elapsed().subsec_millis());
    ///         let mut vec_ui32 = Vec::new();
    ///         elapsed_content
    ///             .split(" ")
    ///             .for_each(|ch| match ch.trim().parse::<u32>() {
    ///                 Ok(ui32) => {
    ///                     vec_ui32.push(ui32);
    ///                     // for Cancelation demo
    ///                     //
    ///                     // if canceled.load(sync::atomic::Ordering) {
    ///                     //     return Progress::Canceled;
    ///                     // }
    ///                 }
    ///                 _ => (),
    ///             });
    ///         // and this, for Cancelation at the end of the tunnel
    ///         //
    ///         // if canceled.load(sync::atomic::Ordering) {
    ///         //     return Progress::Canceled;
    ///         // }
    ///         if vec_ui32.len() > 0 {
    ///             return Progress::Completed(vec_ui32);
    ///         } else {
    ///             return Progress::Error(
    ///                 "task 1 error:
    ///                 unable to parse u32
    ///                 at asynchron/example.rs on the line 20\n"
    ///                     .to_string(),
    ///             );
    ///         }
    ///     });
    ///     task1.try_do();
    ///
    ///     let mut task2_approx_dur = Duration::from_millis(600);
    ///     let instant2: Instant = Instant::now();
    ///     let task2 = Futurize::task(1, move |_| -> Progress<u32, ()> {
    ///         std::thread::sleep(task2_approx_dur);
    ///         return Progress::Completed(instant2.elapsed().subsec_millis());
    ///     });
    ///     task2.try_do();
    ///
    ///     // let mut retry = 0;
    ///
    ///     loop {
    ///         if task1.is_in_progress() {
    ///             match task1.try_get() {
    ///                 Progress::Current => {
    ///                     println!("task 1 is trying to be done\n");
    ///                 }
    ///                 Progress::Canceled => println!("task 1 canceled"),
    ///                 Progress::Completed(value) => {
    ///                     task1_approx_dur = instant1.elapsed();
    ///                     println!(
    ///                         "task 1 completed at: {:?} has value: {:?}\n",
    ///                         task1_approx_dur, value
    ///                     );
    ///                     // task1.try_do();
    ///                     // retry += 1;
    ///                 }
    ///                 Progress::Error(e) => println!("{}", e),
    ///             }
    ///         }
    ///
    ///         // if retry == 0 {
    ///         //     task1.cancel()
    ///         // } else if retry > 1 {
    ///         //     break;
    ///         // }
    ///
    ///         if task2.is_in_progress() {
    ///             match task2.try_get() {
    ///                 Progress::Current => println!("task 2 is trying to be done\n"),
    ///                 Progress::Completed(value) => {
    ///                     task2_approx_dur = instant2.elapsed();
    ///                     println!(
    ///                         "task 2 completed at: {:?} has value: {:?}\n",
    ///                         task2_approx_dur, value
    ///                     );
    ///                     // task1.try_do();
    ///                 }
    ///                 _ => (),
    ///             }
    ///         }
    ///
    ///         if task1.is_done() && task2.is_done() {
    ///             break;
    ///         }
    ///         std::thread::sleep(Duration::from_millis(100));
    ///     }
    ///     let all_approxed_durs = task2_approx_dur + task1_approx_dur;
    ///     println!(
    ///         "all the tasks are done {:?} earlier.\n\nOk",
    ///         all_approxed_durs - task2_approx_dur
    ///     );
    /// }
    /// ```
    pub fn task<F>(id: u32, closure: F) -> Futurize<T, E>
    where
        F: Send + Sync + 'static + Fn(Arc<AtomicBool>) -> Progress<T, E>,
    {
        cfg_if! {
            if #[cfg(feature = "blocking")] {
                Futurize {
                    id,
                    closure: Arc::new(closure),
                    states: Arc::new((
                        AtomicBool::new(false),
                        AtomicBool::new(false),
                        Mutex::new(Progress::Current),
                        Condvar::new(),
                    )),
                    _cancel: Arc::new(AtomicBool::new(false))
                }
            } else {
                Futurize {
                    id,
                    closure: Arc::new(closure),
                    states: Arc::new((
                        AtomicBool::new(false),
                        AtomicBool::new(false),
                        Mutex::new(Progress::Current)
                    )),
                    _cancel: Arc::new(AtomicBool::new(false))
                }
            }
        }
    }

    /// Try (it won't block the current thread) to do the task now and then try to get the progress somewhere later on.
    #[inline]
    pub fn try_do(&self) {
        let awaiting = &self.states.0;
        if !awaiting.load(Ordering::SeqCst) {
            awaiting.store(true, Ordering::SeqCst);
            if self._cancel.load(Ordering::SeqCst) {
                self._cancel.store(false, Ordering::SeqCst)
            }
            let states = Arc::clone(&self.states);
            let cancel = Arc::clone(&self._cancel);
            let closure = Arc::clone(&self.closure);
            cfg_if! {
                if #[cfg(feature = "blocking")] {
                    thread::spawn(move || {
                        let (_, ready,mtx,cvar) = &*states;
                        let result = closure(cancel);
                        let mut mtx = mtx.lock();
                        *mtx = result;
                        ready.store(true, Ordering::Relaxed);
                        cvar.notify_one();
                    });
                } else {
                    thread::spawn(move || {
                        let (_,ready, mtx) = &*states;
                        let result = closure(cancel);
                        let mut mtx = mtx.lock();
                        *mtx = result;
                        ready.store(true, Ordering::Relaxed);
                    });
                }
            }
        }
    }

    /// Cancel the task.
    #[inline(always)]
    pub fn cancel(&self) {
        if !self._cancel.load(Ordering::SeqCst) {
            self._cancel.store(true, Ordering::SeqCst)
        }
    }

    /// Check if the task is canceled.
    #[inline(always)]
    pub fn is_canceled(&self) -> bool {
        self._cancel.load(Ordering::Relaxed)
    }

    /// Get the id of the task
    #[inline(always)]
    pub fn task_id(&self) -> u32 {
        self.id
    }

    #[cfg(feature = "blocking")]
    /// Wait until the task is completed and then get (careful, this is blocking operation).
    #[inline]
    pub fn get(&self) -> Progress<T, E> {
        self.try_do();
        let (awaiting, ready, mtx, cvar) = &*self.states;
        let mut mtx = mtx.lock();
        cvar.wait(&mut mtx);
        let result = mtx.clone();
        *mtx = Progress::Current;
        ready.store(false, Ordering::Relaxed);
        awaiting.store(false, Ordering::Relaxed);
        result
    }

    /// Try to get the progress of the task, it won't block current thread (non-blocking).
    #[inline]
    pub fn try_get(&self) -> Progress<T, E> {
        if self.states.0.load(Ordering::Relaxed) {
            cfg_if! {
                if #[cfg(feature = "blocking")] {
                    let (awaiting, ready, mtx, _) = &*self.states;
                } else {
                    let (awaiting, ready, mtx) = &*self.states;
                }
            }
            if ready.load(Ordering::SeqCst) {
                ready.store(false, Ordering::SeqCst);
                let mut mtx = mtx.lock();
                let result = mtx.clone();
                *mtx = Progress::Current;
                awaiting.store(false, Ordering::Relaxed);
                result
            } else {
                Progress::Current
            }
        } else {
            self.try_do();
            Progress::Current
        }
    }

    /// Check if the task is in progress.
    #[inline(always)]
    pub fn is_in_progress(&self) -> bool {
        self.states.0.load(Ordering::Relaxed)
    }

    /// Check if the task is completed.
    #[inline(always)]
    pub fn is_done(&self) -> bool {
        !self.states.0.load(Ordering::Relaxed)
    }
}
