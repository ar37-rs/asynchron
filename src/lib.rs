#![deny(unsafe_code)]
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
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
            result
        } else {
            None
        }
    }
}

/// Result for Futurized task,
///
/// where C: type of ITaskHandle sync sender value, T: type of Completed value, E: type of Error value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Progress<C, T, E> {
    /// Current progress of the task
    Current(Option<C>),
    /// Indicates if the task is canceled
    Canceled,
    Completed(T),
    Error(E),
}

/// Resume/Suspend handle of the task.
#[derive(Clone)]
pub struct RespendHandle {
    _id: usize,
    pause: Arc<AtomicBool>,
}

impl RespendHandle {
    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Send signal to the inner task handle that the task should be suspended,
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn suspend(&self) {
        self.pause.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.pause.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }
}

/// Cancelation handle of the task.
#[derive(Clone)]
pub struct CancelationHandle {
    _id: usize,
    cancel: Arc<AtomicBool>,
}

impl CancelationHandle {
    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.cancel.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Inner handle of the task,
///
/// where C: type of sync sender value.
#[derive(Clone)]
pub struct ITaskHandle<C> {
    _id: usize,
    awaiting: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
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
        self.pause.store(TRU, Ordering::Relaxed)
    }

    /// Resume the task (it's quite rare that resuming task from inside itself, unless in a particular case).
    pub fn resume(&self) {
        self.pause.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task should be suspended,
    ///
    /// usually applied for a specific task with event loop in it.
    ///
    /// do other things (switch) while the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.pause.load(Ordering::Relaxed)
    }

    /// Cancel the task (it's quite rare that canceling task from inside itself, unless in a particular case).
    pub fn cancel(&self) {
        self.cancel.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task should be canceled.
    pub fn is_canceled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Type name of ITaskHandle,
///
/// Use this if there's no needed for sending current value through channel and if type of ITaskHandle sync sender value not necessary to be known.
pub type InnerTaskHandle = ITaskHandle<()>;

/// Handle of the task.
#[derive(Clone)]
pub struct TaskHandle<C, T, E> {
    _id: usize,
    closure: Arc<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<C, T, E>>,
    states: Arc<(AtomicBool, AtomicUsize, Mutex<Progress<C, T, E>>)>,
    task_handle: ITaskHandle<C>,
}

impl<C, T, E> TaskHandle<C, T, E>
where
    C: Clone + Send + 'static,
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
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
    /// and then try to get the progress from the original task (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &*self.task_handle.awaiting;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle.cancel.store(FAL, Ordering::Relaxed);
            let _stack_size = self.states.1.load(Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let closure = Arc::clone(&self.closure);
            let inner_task_handle = self.task_handle.clone();
            let task = move || {
                let (ready, _, mtx) = &*states;
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
        self.task_handle.pause.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.task_handle.pause.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.task_handle.pause.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.task_handle.cancel.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.task_handle.cancel.load(Ordering::Relaxed)
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.task_handle.awaiting.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.task_handle.awaiting.load(Ordering::Relaxed)
    }
}

/// Futurize task asynchronously.
/// # Example:
///
///```
///use asynchron::{Futurize, Futurized, ITaskHandle, Progress};
///use std::time::{Duration, Instant};
///
///fn main() {
///    let instant: Instant = Instant::now();
///    let task: Futurized<String, u32, String> = Futurize::task(
///        0,
///        move |_task: ITaskHandle<String>| -> Progress<String, u32, String> {
///            let sleep_dur = Duration::from_millis(10);
///            std::thread::sleep(sleep_dur);
///            // Send current task progress.
///            let result = Ok::<String, ()>("The task  wake up from sleep.".into());
///            if let Ok(value) = result {
///                _task.send(value);
///            } else {
///                // return error immediately if something not right, for example:
///                return Progress::Error(
///                    "Something ain't right..., programmer out of bounds.".into(),
///                );
///            }
///            if _task.is_canceled() {
///                let _ = _task.send("Canceling the task".into());
///                Progress::Canceled
///            } else {
///                Progress::Completed(instant.elapsed().subsec_millis())
///            }
///        },
///    );
///
///    // Try do the task now.
///    task.try_do();
///
///    let mut exit = false;
///    loop {
///        task.try_resolve(|progress, resolved| {
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
///            if resolved {
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
pub struct Futurize<C, T, E> {
    _data: PhantomData<(C, T, E)>,
}

impl<C: Clone + Send + 'static, T, E> Futurize<C, T, E> {
    /// Create new task.
    pub fn task<F>(id: usize, f: F) -> Futurized<C, T, E>
    where
        F: Send + Sync + 'static + Fn(ITaskHandle<C>) -> Progress<C, T, E>,
    {
        let _channel = Channel::<C>::init();
        Futurized {
            _id: id,
            closure: Arc::new(f),
            states: Arc::new((
                AtomicBool::new(FAL),
                AtomicUsize::new(ZER),
                Mutex::new(Progress::Current(None)),
            )),
            receiver: _channel.clone(),
            task_handle: ITaskHandle {
                _id: id,
                awaiting: Arc::new(AtomicBool::new(FAL)),
                cancel: Arc::new(AtomicBool::new(FAL)),
                pause: Arc::new(AtomicBool::new(FAL)),
                sync_s: _channel,
            },
        }
    }
}

/// Futurized task.
#[derive(Clone)]
pub struct Futurized<C, T, E> {
    _id: usize,
    closure: Arc<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<C, T, E>>,
    states: Arc<(AtomicBool, AtomicUsize, Mutex<Progress<C, T, E>>)>,
    receiver: Channel<C>,
    task_handle: ITaskHandle<C>,
}

impl<C, T, E> Futurized<C, T, E>
where
    C: Clone + Send + 'static,
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
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
    /// and then try to get the progress (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &*self.task_handle.awaiting;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle.cancel.store(FAL, Ordering::Relaxed);
            let _stack_size = self.states.1.load(Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let closure = Arc::clone(&self.closure);
            let task_handle = self.task_handle.clone();
            let task = move || {
                let (ready, _, mtx) = &*states;
                let result = closure(task_handle);
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
        self.task_handle.pause.store(TRU, Ordering::Relaxed)
    }

    /// Resume the suspended task.
    pub fn resume(&self) {
        self.task_handle.pause.store(FAL, Ordering::Relaxed)
    }

    /// Check if progress of the task is suspended.
    pub fn is_suspended(&self) -> bool {
        self.task_handle.pause.load(Ordering::Relaxed)
    }

    /// Send signal to the inner task handle that the task should be canceled.
    ///
    /// this won't do anything if not explicitly configured inside the task.
    pub fn cancel(&self) {
        self.task_handle.cancel.store(TRU, Ordering::Relaxed)
    }

    /// Check if progress of the task is canceled.
    pub fn is_canceled(&self) -> bool {
        self.task_handle.is_canceled()
    }

    /// Get task handle,
    ///
    /// 'recommended' if only intended to try to do the task, check progress of the task or task cancelation,
    ///
    /// from inside moving closures or from other threads to avoid (channel) synchronous Sender data races.
    pub fn handle(&self) -> TaskHandle<C, T, E> {
        TaskHandle {
            _id: self._id,
            closure: self.closure.clone(),
            states: self.states.clone(),
            task_handle: self.task_handle.clone(),
        }
    }

    /// Get cancelation handle, use this to avoid unnecessary cloning unused task values,
    ///
    /// 'recommended' if only intended to cancel the task from inside moving closures or from other threads.
    pub fn cancelation_handle(&self) -> CancelationHandle {
        CancelationHandle {
            _id: self._id,
            cancel: self.task_handle.cancel.clone(),
        }
    }

    /// Get resume/suspend handle, use this to avoid unnecessary cloning unused task values,
    ///
    /// 'recommended' if only intended to resume or suspend the task from inside moving closures or from other threads.
    pub fn respend_handle(&self) -> RespendHandle {
        RespendHandle {
            _id: self._id,
            pause: self.task_handle.pause.clone(),
        }
    }

    /// Get the id of the task
    pub fn id(&self) -> usize {
        self._id
    }

    /// Try to get the progress of the task, it won't block current thread (non-blocking).
    ///
    /// WARNING! to prevent from data races this fn should be called once at a time.
    pub fn try_get(&self) -> Progress<C, T, E> {
        let awaiting = &*self.task_handle.awaiting;
        if awaiting.load(Ordering::Relaxed) {
            let ready = &self.states.0;
            if ready.load(Ordering::Relaxed) {
                self.task_handle.pause.store(FAL, Ordering::Relaxed);
                let mtx = &self.states.2;
                // there's almost zero chance to deadlock,
                // already guarded + with 2 atomicbools (awaiting and ready), so it's safe to unwrap here.
                let mut mtx = mtx.lock().unwrap();
                let result = mtx.clone();
                *mtx = Progress::Current(None);
                ready.store(FAL, Ordering::Relaxed);
                awaiting.store(FAL, Ordering::Relaxed);
                result
            } else {
                Progress::Current(self.receiver.try_recv())
            }
        } else {
            Progress::Current(None)
        }
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
    pub fn try_resolve<F: FnOnce(Progress<C, T, E>, bool) -> ()>(&self, f: F) {
        if self.task_handle.awaiting.load(Ordering::Relaxed) {
            f(
                self.try_get(),
                !self.task_handle.awaiting.load(Ordering::Relaxed),
            )
        }
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.task_handle.awaiting.load(Ordering::Relaxed)
    }

    /// Check if the task isn't in progress anymore (done).
    pub fn is_done(&self) -> bool {
        !self.task_handle.awaiting.load(Ordering::Relaxed)
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
