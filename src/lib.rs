#![deny(unsafe_code)]
use core::{
    clone::Clone,
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    borrow::Cow,
    sync::{Arc, Condvar, Mutex},
    thread,
};

const ZER: usize = 0;
const TRU: bool = true;
const FAL: bool = false;

/// Result for Futurized task,
///
/// where C: type of ITaskHandle sync sender value, T: type of Completed value,
#[derive(Clone, Debug)]
pub enum Progress<'a, C, T> {
    /// Current progress of the task
    Current(Option<C>),
    /// Indicates if the task is canceled
    Canceled,
    Completed(T),
    Error(Cow<'a, str>),
}

/// Runtime handle of the task.
#[derive(Clone)]
pub struct RuntimeHandle {
    _id: usize,
    rt_states: Arc<(AtomicBool, AtomicBool, AtomicBool, AtomicBool)>,
}

impl RuntimeHandle {
    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Send signal to the inner task handle that the task should be suspended,
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn suspend(&self) {
        self.rt_states.3.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.rt_states.3.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.rt_states.2.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.rt_states.2.load(Ordering::Relaxed)
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.rt_states.0.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.rt_states.0.load(Ordering::Relaxed)
    }
}

/// Inner handle of the task,
///
/// where C: type of sync sender value.
#[derive(Clone)]
pub struct ITaskHandle<C> {
    _id: usize,
    rt_states: Arc<(AtomicBool, AtomicBool, AtomicBool, AtomicBool)>,
    sync_s: Arc<(AtomicBool, Mutex<Option<C>>, Condvar)>,
}

impl<C: Clone + Send + 'static> ITaskHandle<C> {
    /// Send current progress of the task.
    ///
    pub fn send(&self, t: C) {
        let (ready, mtx, cvar) = &*self.sync_s;
        if let Ok(mut mtx) = mtx.lock() {
            *mtx = Some(t);
            ready.store(TRU, Ordering::Relaxed);
            let _ = cvar.wait(mtx);
        }
    }

    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Suspend the task (it's quite rare that suspending task from inside itself, unless in a particular case).
    pub fn suspend(&self) {
        self.rt_states.3.store(TRU, Ordering::Relaxed)
    }

    /// Resume the task (it's quite rare that resuming task from inside itself, unless in a particular case).
    pub fn resume(&self) {
        self.rt_states.3.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task should be suspended,
    ///
    /// usually applied for a specific task with event loop in it.
    ///
    /// do other things (switch) while the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Cancel the task (it's quite rare that canceling task from inside itself, unless in a particular case).
    pub fn cancel(&self) {
        self.rt_states.2.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task should be canceled.
    pub fn is_canceled(&self) -> bool {
        self.rt_states.2.load(Ordering::Relaxed)
    }
}

impl<C> Drop for ITaskHandle<C> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.rt_states.1.store(TRU, Ordering::Relaxed)
        }
    }
}

/// Type name of ITaskHandle,
///
/// Use this if there's no needed for sending current value through channel and if type of ITaskHandle sync sender value not necessary to be known.
pub type InnerTaskHandle = ITaskHandle<()>;

/// Handle of the task.
#[derive(Clone)]
pub struct TaskHandle<C, T> {
    _id: usize,
    states: Arc<(
        Box<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<'static, C, T>>,
        Mutex<Progress<'static, C, T>>,
        AtomicBool,
        AtomicUsize,
    )>,
    rt_states: Arc<(AtomicBool, AtomicBool, AtomicBool, AtomicBool)>,
    sync_s: Arc<(AtomicBool, Mutex<Option<C>>, Condvar)>,
}

impl<C, T> TaskHandle<C, T>
where
    C: Clone + Send + 'static,
    T: Clone + Send + 'static,
{
    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Set stack size (in bytes) of the thread for spawning Futurized task,
    ///
    /// Rust's official doc: https://doc.rust-lang.org/std/thread/struct.Builder.html#method.stack_size
    ///
    /// if the given value is zero or default, it will use Rust's standard stack size.
    pub fn stack_size(&self, size: usize) {
        self.states.3.store(size, Ordering::Relaxed)
    }

    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to resolve later.
    pub fn try_do(&self) {
        let waiting = &self.rt_states.0;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            let _self = self;
            _self.rt_states.2.store(FAL, Ordering::Relaxed);
            let _stack_size = _self.states.3.load(Ordering::Relaxed);
            let states = Arc::clone(&_self.states);
            let inner_task_handle = ITaskHandle {
                _id: _self._id,
                rt_states: Arc::clone(&_self.rt_states),
                sync_s: Arc::clone(&_self.sync_s),
            };
            let task = move || {
                let (closure, mtx, ready, _) = &*states;
                let result = closure(inner_task_handle);
                if let Ok(mut mtx) = mtx.lock() {
                    *mtx = result;
                }
                ready.store(TRU, Ordering::Relaxed)
            };
            if _stack_size == ZER {
                thread::spawn(task);
            } else {
                // Should panic if the given stack size value is incorrect, so it's better to unwrap() here.
                thread::Builder::new()
                    .stack_size(_stack_size)
                    .spawn(task)
                    .unwrap();
            }
        }
    }

    /// Send signal to the inner task handle that the task should be suspended.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn suspend(&self) {
        self.rt_states.3.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.rt_states.3.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.rt_states.2.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.rt_states.2.load(Ordering::Relaxed)
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.rt_states.0.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.rt_states.0.load(Ordering::Relaxed)
    }
}

/// Futurize task asynchronously.
/// # Example:
///
///```
///use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
///use std::{
///    io::Error,
///    time::{Duration, Instant},
///};
///
///fn main() {
///    let instant: Instant = Instant::now();
///    let task: Futurized<String, u32> = Futurize::task(
///        0,
///        move |_task: ITaskHandle<String>| -> Progress<String, u32> {
///            // // Panic if need to.
///            // // will return Error with a message:
///            // // "the task with id: (specific task id) panicked!"
///            // std::panic::panic_any("loudness");
///            let sleep_dur = Duration::from_millis(10);
///            std::thread::sleep(sleep_dur);
///            let result = Ok::<String, Error>(
///                format!("The task with id: {} wake up from sleep", _task.id()).into(),
///            );
///            match result {
///                Ok(value) => {
///                    // Send current task progress.
///                    _task.send(value)
///                }
///                Err(e) => {
///                    // Return error immediately if something not right, for example:
///                    return Progress::Error(e.to_string().into());
///                }
///            }
///
///            if _task.is_canceled() {
///                _task.send("Canceling the task".into());
///                return Progress::Canceled;
///            }
///            Progress::Completed(instant.elapsed().subsec_millis())
///        },
///    );
///
///    // Try do the task now.
///    task.try_do();
///
///    let mut exit = false;
///    loop {
///        task.try_resolve(|progress, done| {
///            match progress {
///                Progress::Current(task_receiver) => {
///                    if let Some(value) = task_receiver {
///                        println!("{}\n", value)
///                    }
///                    // // Cancel if need to.
///                    // task.cancel()
///                }
///                Progress::Canceled => {
///                    println!("The task was canceled\n")
///                }
///                Progress::Completed(elapsed) => {
///                    println!("The task finished in: {:?} milliseconds\n", elapsed)
///                }
///                Progress::Error(e) => {
///                    println!("{}\n", e)
///                }
///            }
///
///            if done {
///                // This scope act like "finally block", do final things here.
///                exit = true
///            }
///        });
///
///        if exit {
///            break;
///        }
///    }
///}
///```
pub struct Futurize<C, T> {
    _data: PhantomData<(C, T)>,
}

impl<C: Clone + Send + 'static, T> Futurize<C, T> {
    /// Create new task.
    pub fn task<F>(id: usize, f: F) -> Futurized<C, T>
    where
        F: Send + Sync + Fn(ITaskHandle<C>) -> Progress<'static, C, T> + 'static,
    {
        Futurized {
            _id: id,
            states: Arc::new((
                Box::new(f),
                Mutex::new(Progress::Current(None)),
                AtomicBool::new(FAL),
                AtomicUsize::new(ZER),
            )),
            receiver: Arc::new((AtomicBool::new(FAL), Mutex::new(None), Condvar::new())),
            rt_states: Arc::new((
                AtomicBool::new(FAL),
                AtomicBool::new(FAL),
                AtomicBool::new(FAL),
                AtomicBool::new(FAL),
            )),
        }
    }
}

/// Futurized task.
#[derive(Clone)]
pub struct Futurized<C, T> {
    _id: usize,
    states: Arc<(
        Box<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<'static, C, T>>,
        Mutex<Progress<'static, C, T>>,
        AtomicBool,
        AtomicUsize,
    )>,
    receiver: Arc<(AtomicBool, Mutex<Option<C>>, Condvar)>,
    rt_states: Arc<(AtomicBool, AtomicBool, AtomicBool, AtomicBool)>,
}

impl<C, T> Futurized<C, T>
where
    C: Clone + Send + 'static,
    T: Clone + Send + 'static,
{
    /// Set stack size (in bytes) of the thread for spawning Futurized task,
    ///
    /// Rust's official doc: https://doc.rust-lang.org/std/thread/struct.Builder.html#method.stack_size
    ///
    /// if the given value is zero or default, it will use Rust's standard stack size.
    pub fn stack_size(&self, size: usize) {
        self.states.3.store(size, Ordering::Relaxed)
    }

    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to resolve later.
    pub fn try_do(&self) {
        let waiting = &self.rt_states.0;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            let _self = self;
            _self.rt_states.3.store(FAL, Ordering::Relaxed);
            let _stack_size = _self.states.3.load(Ordering::Relaxed);
            let states = Arc::clone(&_self.states);
            let inner_task_handle = ITaskHandle {
                _id: _self._id,
                rt_states: Arc::clone(&_self.rt_states),
                sync_s: Arc::clone(&_self.receiver),
            };
            let task = move || {
                let (closure, mtx, ready, _) = &*states;
                let result = closure(inner_task_handle);
                if let Ok(mut mtx) = mtx.lock() {
                    *mtx = result;
                }
                ready.store(TRU, Ordering::Relaxed)
            };
            if _stack_size == ZER {
                thread::spawn(task);
            } else {
                // Should panic if the given stack size value is incorrect, so it's better to unwrap() here.
                thread::Builder::new()
                    .stack_size(_stack_size)
                    .spawn(task)
                    .unwrap();
            }
        }
    }

    /// Send signal to the inner task handle that the task should be suspended.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn suspend(&self) {
        self.rt_states.3.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.rt_states.3.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.rt_states.3.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.rt_states.2.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.rt_states.2.load(Ordering::Relaxed)
    }

    /// Get the task handle, if only intended to try to do,
    ///
    /// check progress, cancel, suspend or resume the task, from inside moving closures,
    ///
    /// or from other threads to avoid (channel) synchronous Sender data races.
    pub fn handle(&self) -> TaskHandle<C, T> {
        let _self = self;
        TaskHandle {
            _id: _self._id,
            states: Arc::clone(&_self.states),
            sync_s: Arc::clone(&_self.receiver),
            rt_states: Arc::clone(&_self.rt_states),
        }
    }

    /// Get the runtime handle, if only intended to check progress, cancel,
    ///
    /// suspend or resume the task from inside moving closures or from other threads.
    ///
    /// use this to avoid unnecessary cloning unused task values.
    pub fn rt_handle(&self) -> RuntimeHandle {
        let _self = self;
        RuntimeHandle {
            _id: _self._id,
            rt_states: Arc::clone(&_self.rt_states),
        }
    }

    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Try resolve the progress of the task,
    ///
    /// WARNING! to prevent from data races this fn should be called once at a time.
    pub fn try_resolve<F: FnOnce(Progress<C, T>, bool) -> ()>(&self, f: F) {
        if self.rt_states.0.load(Ordering::Relaxed) {
            let _awaiting = &self.rt_states.0;
            let result = || {
                let _self = self;
                let (awaiting, task_panicked, _, suspended) = &*_self.rt_states;
                {
                    if task_panicked.load(Ordering::Relaxed) {
                        suspended.store(FAL, Ordering::Relaxed);
                        task_panicked.store(FAL, Ordering::Relaxed);
                        awaiting.store(FAL, Ordering::Relaxed);
                        // fixes unsound problem, return error immediately if the task is panicked.
                        return Progress::Error(Cow::Owned(format!(
                            "the task with id: {} panicked!",
                            _self._id
                        )));
                    }
                }

                if awaiting.load(Ordering::Relaxed) {
                    let _self = _self;
                    {
                        if _self.receiver.0.load(Ordering::Relaxed) {
                            let (ready, mtx, cvar) = &*_self.receiver;
                            let result = if let Ok(mut mtx) = mtx.lock() {
                                let result = Clone::clone(&*mtx);
                                *mtx = None;
                                result
                            } else {
                                None
                            };
                            ready.store(FAL, Ordering::Relaxed);
                            cvar.notify_all();
                            return Progress::Current(result);
                        }
                    }

                    let ready = &_self.states.2;
                    if ready.load(Ordering::Relaxed) {
                        suspended.store(FAL, Ordering::Relaxed);
                        let mtx = &_self.states.1;
                        // there's almost zero chance to deadlock,
                        // already guarded + with 2 atomicbools (awaiting and ready), so it's safe to unwrap here.
                        let mut mtx = mtx.lock().unwrap();
                        let result = Clone::clone(&*mtx);
                        *mtx = Progress::Current(None);
                        ready.store(FAL, Ordering::Relaxed);
                        awaiting.store(FAL, Ordering::Relaxed);
                        return result;
                    }
                    return Progress::Current(None);
                }
                Progress::Current(None)
            };
            f(result(), !_awaiting.load(Ordering::Relaxed))
        }
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.rt_states.0.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.rt_states.0.load(Ordering::Relaxed)
    }
}

use std::sync::{LockResult, MutexGuard};
/// Simple helper thread safe state management,
///
/// using std::sync::Arc and std::sync::Mutex under the hood.
///
/// # Example:
///
///```
///use asynchron::SyncState;
///
///fn main() -> core::result::Result<(), Box<dyn std::error::Error>> {
///    let state: SyncState<i32> = SyncState::new(0);
///    let _state = state.clone();
///
///    if let Err(_) = std::thread::spawn(move||{
///        _state.set(20);
///    })
///     .join()
///    {
///        let e = std::io::Error::new(std::io::ErrorKind::Other, "Unable to join thread.");
///        return Err(Box::new(e));
///    }
///
///    if let Ok(value) = state.get() {
///       println!("{:?}", *value);
///    } else {
///       let e = std::io::Error::new(std::io::ErrorKind::Other, "Mutex poisonous!");
///       return Err(Box::new(e));
///    }
///    Ok(())
///}
///```
#[derive(Clone)]
pub struct SyncState<T> {
    item: Arc<Mutex<T>>,
}

impl<T: Clone + Send + Sync + ?Sized> SyncState<T> {
    /// Create new state.
    pub fn new(t: T) -> SyncState<T> {
        SyncState {
            item: Arc::new(Mutex::new(t)),
        }
    }

    /// Get state.
    pub fn get(&self) -> LockResult<MutexGuard<'_, T>> {
        let rwx = &*self.item;
        rwx.lock()
    }

    /// Set state.
    pub fn set(&self, t: T) {
        let rwx = &*self.item;
        if let Ok(mut rwx) = rwx.lock() {
            *rwx = t
        }
    }
}
