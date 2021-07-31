#![deny(unsafe_code)]
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};
use crossbeam_channel::{bounded, Receiver, SendError, Sender};
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use std::{sync::Arc, thread};

#[derive(Clone)]
/// Simple helper thread safe state management,
///
/// uses std::sync::Arc and fast RwLock<T> [read/write lock] from parking_lot crate under the hood.
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
        rwx.read()
    }

    /// Set state.
    pub fn set(&self, t: T) {
        let rwx = &*self.item;
        let mut rwx = rwx.write();
        *rwx = t
    }
}

const ZER: usize = 0;
const TRU: bool = true;
const FAL: bool = false;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// Result for Futurized task,
///
/// where C: type of ITaskHandle sync sender value, T: type of Completed value, E: type of Error value.
pub enum Progress<C, T, E> {
    /// Current progress of the task
    Current(Option<C>),
    /// Indicates if the task is canceled
    Canceled,
    Completed(T),
    Error(E),
}

#[derive(Clone)]
/// Resume/Suspend handle of the task.
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

#[derive(Clone)]
/// Cancelation handle of the task.
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

#[derive(Clone)]
/// Inner handle of the task,
///
/// where C: type of sync sender value.
pub struct ITaskHandle<C> {
    _id: usize,
    awaiting: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    sync_s: Sender<C>,
}

impl<C> ITaskHandle<C> {
    /// Send current progress of the task,
    ///
    /// fast synchronous sender from crossbeam_channel crate under the hood.
    pub fn send(&self, t: C) -> Result<(), SendError<C>> {
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

    /// Check if progress of the task sould be canceled.
    pub fn is_canceled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Type name of ITaskHandle,
///
/// Use this if there's no needed for sending current value through channel and if type of ITaskHandle sync sender value not necessary to be known.
pub type InnerTaskHandle = ITaskHandle<()>;

#[derive(Clone)]
/// Handle of the task,
///
pub struct TaskHandle<C, T, E> {
    _id: usize,
    closure: Arc<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<C, T, E>>,
    states: Arc<(AtomicBool, Mutex<Progress<C, T, E>>)>,
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

    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to get the progress from the original task (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &*self.task_handle.awaiting;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle.cancel.store(FAL, Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let closure = Arc::clone(&self.closure);
            let inner_task_handle = self.task_handle.clone();
            thread::spawn(move || {
                let (ready, mtx) = &*states;
                let result = closure(inner_task_handle);
                *mtx.lock() = result;
                ready.store(TRU, Ordering::Relaxed)
            });
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
///            let value = format!("The task wake up from sleep");
///            // Send current task progress.
///            // let _ = _task.send(value); (to ignore sender error in some specific cases if needed).
///            if let Err(e) = _task.send(value) {
///                // Return error immediately
///                // !WARNING!
///                // if always ignoring error,
///                // Undefined Behavior there's always a chance to occur and hard to debug,
///                // always return error for safety in many cases (recommended), rather than unwrapping.
///                return Progress::Error(format!(
///                    "Progress error while sending state: {}",
///                    e.to_string(),
///                ));
///            }
///
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
///    loop {
///        if task.is_in_progress() {
///            match task.try_get() {
///                Progress::Current(task_receiver) => {
///                    if let Some(value) = task_receiver {
///                        println!("{}\n", value)
///                    }
///                    // Cancel if need to.
///                    // task.cancel();
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
///            if task.is_done() {
///                break;
///            }
///        }
///    }
///}
///```
pub struct Futurize<C, T, E> {
    _data: PhantomData<(C, T, E)>,
}

impl<C, T, E> Futurize<C, T, E> {
    /// Create new task.
    pub fn task<F>(id: usize, f: F) -> Futurized<C, T, E>
    where
        F: Send + Sync + 'static + Fn(ITaskHandle<C>) -> Progress<C, T, E>,
    {
        let (sender, receiver) = bounded::<C>(ZER);
        Futurized {
            _id: id,
            closure: Arc::new(f),
            states: Arc::new((AtomicBool::new(FAL), Mutex::new(Progress::Current(None)))),
            receiver,
            task_handle: ITaskHandle {
                _id: id,
                awaiting: Arc::new(AtomicBool::new(FAL)),
                cancel: Arc::new(AtomicBool::new(FAL)),
                pause: Arc::new(AtomicBool::new(FAL)),
                sync_s: sender,
            },
        }
    }
}

#[derive(Clone)]
/// Futurized task
pub struct Futurized<C, T, E> {
    _id: usize,
    closure: Arc<dyn Send + Sync + Fn(ITaskHandle<C>) -> Progress<C, T, E>>,
    states: Arc<(AtomicBool, Mutex<Progress<C, T, E>>)>,
    receiver: Receiver<C>,
    task_handle: ITaskHandle<C>,
}

impl<C, T, E> Futurized<C, T, E>
where
    C: Clone + Send + 'static,
    T: Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// Try (it won't block the current thread) to do the task now,
    ///
    /// and then try to get the progress (if the task is in progress).
    pub fn try_do(&self) {
        let waiting = &*self.task_handle.awaiting;
        if !waiting.load(Ordering::SeqCst) {
            waiting.store(TRU, Ordering::SeqCst);
            self.task_handle.cancel.store(FAL, Ordering::Relaxed);
            let states = Arc::clone(&self.states);
            let closure = Arc::clone(&self.closure);
            let task_handle = self.task_handle.clone();
            thread::spawn(move || {
                let (ready, mtx) = &*states;
                let result = closure(task_handle);
                *mtx.lock() = result;
                ready.store(TRU, Ordering::Relaxed)
            });
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
    /// WARNING! to prevent from data races this fn should be called once at time.
    pub fn try_get(&self) -> Progress<C, T, E> {
        let awaiting = &*self.task_handle.awaiting;
        if awaiting.load(Ordering::Relaxed) {
            let ready = &self.states.0;
            if ready.load(Ordering::SeqCst) {
                ready.store(FAL, Ordering::SeqCst);
                self.task_handle.pause.store(FAL, Ordering::Relaxed);
                let mtx = &self.states.1;
                let mut mtx = mtx.lock();
                let result = mtx.clone();
                *mtx = Progress::Current(None);
                awaiting.store(FAL, Ordering::Relaxed);
                result
            } else {
                if let Ok(val) = self.receiver.try_recv() {
                    Progress::Current(Some(val))
                } else {
                    Progress::Current(None)
                }
            }
        } else {
            Progress::Current(None)
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
