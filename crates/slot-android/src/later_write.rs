//! Small files written off the GL thread (beta41 journal, 2026-11-19): the
//! last selection held the KONKR's shelf 5.1 s while its card was busy, a
//! changed setting 50-65 ms. One worker writes them in order; a newer
//! version of a file waiting its turn replaces the older one.
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send>;

struct Queue {
    jobs: Mutex<VecDeque<(PathBuf, Job)>>,
    ready: Condvar,
    /// The worker started: without it, files are written at once.
    alive: bool,
}

fn queue() -> &'static Queue {
    static QUEUE: OnceLock<Queue> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let started = std::thread::Builder::new().name("slot-later-write".into()).spawn(|| {
            let q = queue();
            loop {
                let job = {
                    let mut jobs = q.jobs.lock().unwrap_or_else(|e| e.into_inner());
                    while jobs.is_empty() {
                        jobs = q.ready.wait(jobs).unwrap_or_else(|e| e.into_inner());
                    }
                    jobs.pop_front()
                };
                if let Some((_, job)) = job { job(); }
            }
        });
        if let Err(error) = &started {
            crate::log_warn(&format!("background writer not started: {error}"));
        }
        Queue { jobs: Mutex::new(VecDeque::new()), ready: Condvar::new(), alive: started.is_ok() }
    })
}

/// Runs [job], which writes [file], on the writer thread. Tests write at once.
pub(crate) fn write(file: PathBuf, job: impl FnOnce() + Send + 'static) {
    if cfg!(test) {
        job();
        return;
    }
    let q = queue();
    if !q.alive {
        job();
        return;
    }
    let mut jobs = q.jobs.lock().unwrap_or_else(|e| e.into_inner());
    let job: Job = Box::new(job);
    match jobs.iter_mut().find(|(waiting, _)| *waiting == file) {
        Some(slot) => slot.1 = job,
        None => jobs.push_back((file, job)),
    }
    q.ready.notify_one();
}
