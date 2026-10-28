use super::{accessibility_permission_granted, capture_frontmost, CaptureIssue, InsertionTarget};
use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

enum Observation<T> {
    Editor(T),
    Browsing,
    Protected,
    Unknown,
}

struct TargetMemory<T> {
    generation: u64,
    capture_in_progress: bool,
    target: Option<T>,
}

impl<T: Clone> TargetMemory<T> {
    fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    fn complete(&mut self, generation: u64, observation: Observation<T>) -> Option<T> {
        if generation != self.generation {
            return None;
        }
        match observation {
            Observation::Editor(target) => self.target = Some(target),
            Observation::Browsing => {}
            Observation::Protected => self.target = None,
            Observation::Unknown => return None,
        }
        self.target.clone()
    }
}

static MEMORY: Mutex<TargetMemory<InsertionTarget>> = Mutex::new(TargetMemory {
    generation: 0,
    capture_in_progress: false,
    target: None,
});

fn observe(wait: bool) -> Result<InsertionTarget> {
    observe_with(&MEMORY, wait, || capture_frontmost(wait))
}

fn observe_with<T: Clone>(
    memory: &Mutex<TargetMemory<T>>,
    wait: bool,
    capture: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let generation = {
        let mut memory = memory
            .lock()
            .map_err(|_| anyhow!("Editor tracking is unavailable"))?;
        if memory.capture_in_progress {
            return Err(anyhow!("Editor capture is in progress"));
        }
        memory.capture_in_progress = wait;
        memory.begin()
    };
    // No AX IPC under the lock: an unresponsive editor must not stall another
    // observation or prevent a foreground capture superseding a worker sample.
    let captured = capture();
    let observation = match &captured {
        Ok(target) => Observation::Editor(target.clone()),
        Err(error) => match error.downcast_ref::<CaptureIssue>() {
            Some(CaptureIssue::NonEditable) => Observation::Browsing,
            Some(CaptureIssue::Protected) => Observation::Protected,
            None => Observation::Unknown,
        },
    };
    let selected = {
        let mut memory = memory
            .lock()
            .map_err(|_| anyhow!("Editor tracking is unavailable"))?;
        if wait {
            memory.capture_in_progress = false;
        }
        memory.complete(generation, observation)
    };
    selected.ok_or_else(|| {
        captured
            .err()
            .unwrap_or_else(|| anyhow!("The focused editor changed. Try again."))
    })
}

pub(super) fn capture() -> Result<InsertionTarget> {
    let target = observe(true)?;
    if !super::target_is_available(&target) {
        return Err(anyhow!("The previous text field is no longer available. Focus a text field to choose a new destination."));
    }
    Ok(target)
}

/// Tracks only retained AX identities and roles. It never reads field contents,
/// clipboard data, typed characters, or requests macOS permissions.
pub struct EditorMonitor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl EditorMonitor {
    pub fn start() -> Result<Self> {
        Self::with_sample(|| {
            objc2::rc::autoreleasepool(|_| {
                if accessibility_permission_granted() {
                    let _ = observe(false);
                } else if let Ok(mut memory) = MEMORY.lock() {
                    memory.begin();
                    memory.target = None;
                }
            });
        })
    }

    fn with_sample(mut sample: impl FnMut() + Send + 'static) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let worker = std::thread::Builder::new()
            .name("quill-editor".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    sample();
                    std::thread::park_timeout(Duration::from_millis(100));
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for EditorMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> TargetMemory<u32> {
        TargetMemory {
            generation: 0,
            capture_in_progress: false,
            target: None,
        }
    }

    fn sample(memory: &mut TargetMemory<u32>, observation: Observation<u32>) -> Option<u32> {
        let generation = memory.begin();
        memory.complete(generation, observation)
    }

    #[test]
    fn dropping_monitor_wakes_a_parked_worker_and_joins_it() {
        let (sampled_tx, sampled_rx) = std::sync::mpsc::channel();
        let monitor = EditorMonitor::with_sample(move || {
            let _ = sampled_tx.send(());
        })
        .unwrap();
        sampled_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = std::time::Instant::now();
        drop(monitor);
        assert!(start.elapsed() < Duration::from_secs(2));
        while sampled_rx.try_recv().is_ok() {}
        assert!(matches!(
            sampled_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn dropping_monitor_waits_for_inflight_sample_and_never_samples_again() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let monitor = EditorMonitor::with_sample(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        })
        .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let stop = Arc::clone(&monitor.stop);
        let dropping = std::thread::spawn(move || drop(monitor));
        // Wait for the actual stop request without depending on scheduler timing.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !stop.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        release_tx.send(()).unwrap();
        dropping.join().unwrap();
        assert!(entered_rx.try_recv().is_err());
    }

    #[test]
    fn foreground_capture_supersedes_an_inflight_worker_without_holding_ipc_lock() {
        let shared = Mutex::new(memory());
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let shared_ref = &shared;
            let worker = scope.spawn(move || {
                observe_with(shared_ref, false, || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    Ok(10)
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(observe_with(&shared, true, || Ok(20)).unwrap(), 20);
            release_tx.send(()).unwrap();
            assert!(worker.join().unwrap().is_err());
        });
        assert_eq!(shared.lock().unwrap().target, Some(20));
    }

    #[test]
    fn worker_does_not_interrupt_preparation_or_read_through_its_target() {
        let shared = Mutex::new(memory());
        assert_eq!(
            observe_with(&shared, true, || {
                assert!(
                    observe_with(&shared, false, || panic!("worker must not perform AX IPC"))
                        .is_err()
                );
                Ok(10)
            })
            .unwrap(),
            10
        );
        assert!(!shared.lock().unwrap().capture_in_progress);
        assert!(observe_with(&shared, true, || Err(CaptureIssue::Protected.into())).is_err());
        assert_eq!(shared.lock().unwrap().target, None);
        assert!(!shared.lock().unwrap().capture_in_progress);
    }

    #[test]
    fn browsing_keeps_the_last_editor_and_a_new_field_replaces_it() {
        let mut memory = memory();
        assert_eq!(sample(&mut memory, Observation::Browsing), None);
        assert_eq!(sample(&mut memory, Observation::Editor(10)), Some(10));
        assert_eq!(sample(&mut memory, Observation::Browsing), Some(10));
        let captured = sample(&mut memory, Observation::Editor(20));
        assert_eq!(captured, Some(20));
        assert_eq!(sample(&mut memory, Observation::Editor(30)), Some(30));
        assert_eq!(
            captured,
            Some(20),
            "an active session retains its destination"
        );
    }

    #[test]
    fn password_or_unreadable_focus_cannot_fall_back_to_an_old_field() {
        let mut memory = memory();
        sample(&mut memory, Observation::Editor(10));
        assert_eq!(sample(&mut memory, Observation::Unknown), None);
        assert_eq!(sample(&mut memory, Observation::Protected), None);
        assert_eq!(sample(&mut memory, Observation::Browsing), None);
    }

    #[test]
    fn slow_sample_cannot_overwrite_a_newer_capture_or_password_boundary() {
        let mut memory = memory();
        let stale = memory.begin();
        sample(&mut memory, Observation::Editor(20));
        assert_eq!(memory.complete(stale, Observation::Editor(10)), None);
        assert_eq!(memory.target, Some(20));
        let stale = memory.begin();
        sample(&mut memory, Observation::Protected);
        memory.complete(stale, Observation::Editor(30));
        assert_eq!(memory.target, None);
    }
}
