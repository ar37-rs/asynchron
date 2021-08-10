#![deny(unsafe_code)]
use core::{
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

#[derive(Clone)]
struct Channel<C> {
    data: Arc<(AtomicBool, Mutex<Option<C>>, Condvar)>,
}

impl<C: Clone + Send + 'static> Channel<C> {
    fn init() -> Self {
        Self {
            data: Arc::new((AtomicBool::new(FAL), Mutex::new(None), Condvar::new())),
        }
    }

    fn send(&self, t: C) {
        let (ready, mtx, cvar) = &*self.data;
        if let Ok(mut mtx) = mtx.lock() {
            *mtx = Some(t);
            ready.store(TRU, Ordering::Relaxed);
            let _ = cvar.wait(mtx);
        }
    }

    fn try_recv(&self) -> Option<C> {
        let ready = &self.data.0;
        if ready.load(Ordering::Relaxed) {
            let (_, mtx, cvar) = &*self.data;
            let result = if let Ok(mut mtx) = mtx.lock() {
                let result = mtx.clone();
                *mtx = None;
                result
            } else {
                None
            };
            ready.store(FAL, Ordering::Relaxed);
            cvar.notify_all();
            return result;
        }
        None
    }
}

/// Result for Futurized task,
///
/// where C: type of ITaskHandle sync sender value, T: type of Completed value,
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    cancel_pause: Arc<(AtomicBool, AtomicBool)>,
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
        self.cancel_pause.1.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.cancel_pause.1.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.cancel_pause.0.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.cancel_pause.0.load(Ordering::Relaxed)
    }
}

/// Inner handle of the task,
///
/// where C: type of sync sender value.
#[derive(Clone)]
pub struct ITaskHandle<C> {
    _id: usize,
    awaiting: Arc<(AtomicBool, AtomicBool)>,
    cancel_pause: Arc<(AtomicBool, AtomicBool)>,
    sync_s: Channel<C>,
}

impl<C: Clone + Send + 'static> ITaskHandle<C> {
    /// Send current progress of the task.
    pub fn send(&self, t: C) {
        self.sync_s.send(t)
    }

    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Suspend the task (it's quite rare that suspending task from inside itself, unless in a particular case).
    pub fn suspend(&self) {
        self.cancel_pause.1.store(TRU, Ordering::Relaxed)
    }

    /// Resume the task (it's quite rare that resuming task from inside itself, unless in a particular case).
    pub fn resume(&self) {
        self.cancel_pause.1.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task should be suspended,
    ///
    /// usually applied for a specific task with event loop in it.
    ///
    /// do other things (switch) while the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Cancel the task (it's quite rare that canceling task from inside itself, unless in a particular case).
    pub fn cancel(&self) {
        self.cancel_pause.0.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task should be canceled.
    pub fn is_canceled(&self) -> bool {
        self.cancel_pause.0.load(Ordering::Relaxed)
    }
}

impl<C> Drop for ITaskHandle<C> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.awaiting.1.store(TRU, Ordering::Relaxed)
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
        AtomicBool,
        AtomicUsize,
        Mutex<Progress<'static, C, T>>,
        Box<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<'static, C, T>>,
    )>,
    task_handle: ITaskHandle<C>,
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
        self.states.1.store(size, Ordering::Relaxed)
    }

    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to resolve or try to get the progress from the original task (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &self.task_handle.awaiting.0;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle
                .cancel_pause
                .0
                .store(FAL, Ordering::Relaxed);
            let _stack_size = self.states.1.load(Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let inner_task_handle = self.task_handle.clone();
            let task = move || {
                let (ready, _, mtx, closure) = &*states;
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
        self.task_handle
            .cancel_pause
            .1
            .store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.task_handle
            .cancel_pause
            .1
            .store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.task_handle.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.task_handle.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.task_handle
            .cancel_pause
            .0
            .store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.task_handle.cancel_pause.0.load(Ordering::Relaxed)
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.task_handle.awaiting.0.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.task_handle.awaiting.0.load(Ordering::Relaxed)
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
        F: Send + Sync + 'static + Fn(ITaskHandle<C>) -> Progress<'static, C, T>,
    {
        let _channel = Channel::<C>::init();
        Futurized {
            _id: id,
            states: Arc::new((
                AtomicBool::new(FAL),
                AtomicUsize::new(ZER),
                Mutex::new(Progress::Current(None)),
                Box::new(f),
            )),
            receiver: _channel.clone(),
            task_handle: ITaskHandle {
                _id: id,
                awaiting: Arc::new((AtomicBool::new(FAL), AtomicBool::new(FAL))),
                cancel_pause: Arc::new((AtomicBool::new(FAL), AtomicBool::new(FAL))),
                sync_s: _channel,
            },
        }
    }
}

/// Futurized task.
#[derive(Clone)]
pub struct Futurized<C, T> {
    _id: usize,
    states: Arc<(
        AtomicBool,
        AtomicUsize,
        Mutex<Progress<'static, C, T>>,
        Box<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<'static, C, T>>,
    )>,
    receiver: Channel<C>,
    task_handle: ITaskHandle<C>,
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
        self.states.1.store(size, Ordering::Relaxed)
    }

    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to resolve or try to get the progress (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &self.task_handle.awaiting.0;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle
                .cancel_pause
                .0
                .store(FAL, Ordering::Relaxed);
            let _stack_size = self.states.1.load(Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let inner_task_handle = self.task_handle.clone();
            let task = move || {
                let (ready, _, mtx, closure) = &*states;
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
        self.task_handle
            .cancel_pause
            .1
            .store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.task_handle
            .cancel_pause
            .1
            .store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.task_handle.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Check if progress of the task is resumed.
    pub fn is_resumed(&self) -> bool {
        !self.task_handle.cancel_pause.1.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.task_handle
            .cancel_pause
            .0
            .store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.task_handle.cancel_pause.0.load(Ordering::Relaxed)
    }

    /// Get the task handle, if only intended to try to do,
    ///
    /// check progress, cancel, suspend or resume the task, from inside moving closures,
    ///
    /// or from other threads to avoid (channel) synchronous Sender data races.
    pub fn handle(&self) -> TaskHandle<C, T> {
        TaskHandle {
            _id: self._id,
            states: self.states.clone(),
            task_handle: self.task_handle.clone(),
        }
    }

    /// Get the runtime handle, if only intended to cancel,
    ///
    /// suspend or resume the task from inside moving closures or from other threads.
    ///
    /// use this to avoid unnecessary cloning unused task values.
    pub fn rt_handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            _id: self._id,
            cancel_pause: self.task_handle.cancel_pause.clone(),
        }
    }

    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Try to get the progress of the task, it won't block current thread (non-blocking).
    ///
    /// WARNING! to prevent from data races this fn should be called once at a time.
    pub fn try_get(&self) -> Progress<C, T> {
        let (awaiting, task_panicked) = &*self.task_handle.awaiting;
        if task_panicked.load(Ordering::Relaxed) {
            self.task_handle
                .cancel_pause
                .1
                .store(FAL, Ordering::Relaxed);
            task_panicked.store(FAL, Ordering::Relaxed);
            awaiting.store(FAL, Ordering::Relaxed);
            // fixes unsound problem, return error immediately if the task is panicked.
            return Progress::Error(format!("the task with id: {} panicked!", self._id).into());
        }

        if awaiting.load(Ordering::Relaxed) {
            let ready = &self.states.0;
            if ready.load(Ordering::Relaxed) {
                self.task_handle
                    .cancel_pause
                    .1
                    .store(FAL, Ordering::Relaxed);
                let mtx = &self.states.2;
                // there's almost zero chance to deadlock,
                // already guarded + with 2 atomicbools (awaiting and ready), so it's safe to unwrap here.
                let mut mtx = mtx.lock().unwrap();
                let result = mtx.clone();
                *mtx = Progress::Current(None);
                ready.store(FAL, Ordering::Relaxed);
                awaiting.store(FAL, Ordering::Relaxed);
                return result;
            }
            return Progress::Current(self.receiver.try_recv());
        }
        Progress::Current(None)
    }

    /// Try resolve the progress of the task,
    ///
    /// this fn is equivalent to:
    ///```
    /// //if task.is_in_progress() {
    /// //   match task.try_get() {
    /// //       ...
    /// //   }
    /// //}
    ///```
    /// WARNING! to prevent from data races this fn should be called once at a time.
    #[inline]
    pub fn try_resolve<F: FnOnce(Progress<C, T>, bool) -> ()>(&self, f: F) {
        if self.task_handle.awaiting.0.load(Ordering::Relaxed) {
            f(
                self.try_get(),
                !self.task_handle.awaiting.0.load(Ordering::Relaxed),
            )
        }
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.task_handle.awaiting.0.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.task_handle.awaiting.0.load(Ordering::Relaxed)
    }
}

use std::sync::{RwLock, RwLockReadGuard};
/// Simple (a bit costly) helper thread safe state management,
///
/// using std::sync::Arc and std::sync::RwLock under the hood.
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
///    }).join() {
///        let e = std::io::Error::new(std::io::ErrorKind::Other, "Unable to join thread.");
///        return Err(Box::new(e));
///    }
///
///    let values: (i32, i32) = (*state.get(), *state.get());
///    println!("{:?}", values);
///    Ok(())
///}
///```
#[derive(Clone)]
pub struct SyncState<T> {
    item: Arc<RwLock<T>>,
}

impl<T: Clone + Send + Sync + ?Sized> SyncState<T> {
    /// Create new state.
    pub fn new(t: T) -> SyncState<T> {
        SyncState {
            item: Arc::new(RwLock::new(t)),
        }
    }

    /// Get state.
    pub fn get(&self) -> RwLockReadGuard<'_, T> {
        let rwx = &*self.item;
        rwx.read().unwrap()
    }

    /// Set state.
    pub fn set(&self, t: T) {
        let rwx = &*self.item;
        if let Ok(mut rwx) = rwx.write() {
            *rwx = t
        }
    }
}
