//! Dedicated single-thread tokio runtime owned by this crate.
//!
//! One OS thread parks a `new_current_thread` runtime for the life of the
//! process and runs binding futures on it sequentially. Async napi entry
//! points (M3) submit here; no libuv thread ever blocks.

use std::future::Future;
use std::pin::Pin;
use std::sync::{mpsc, OnceLock};
use std::thread::{self, JoinHandle};

/// A boxed job the runtime thread polls to completion.
type BoxJob = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

struct Dedicated {
    tx: mpsc::Sender<BoxJob>,
    _thread: JoinHandle<()>,
}

static DEDICATED: OnceLock<Dedicated> = OnceLock::new();

fn global() -> &'static Dedicated {
    DEDICATED.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<BoxJob>();
        let thread = thread::Builder::new()
            .name(String::from("rivulus-node-rt"))
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("dedicated runtime builds");
                rt.block_on(async move {
                    // Blocking receive is fine: this thread exists only to run jobs.
                    while let Ok(job) = rx.recv() {
                        job.await;
                    }
                });
            })
            .expect("runtime thread spawns");
        Dedicated {
            tx,
            _thread: thread,
        }
    })
}

/// Run a future to completion on the dedicated runtime, blocking the caller.
///
/// Test and sync-path plumbing only. Async napi contexts in M3 await the
/// oneshot instead of calling this.
pub fn block_on_dedicated<F, T>(fut: F) -> T
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<T>();
    global()
        .tx
        .send(Box::pin(async move {
            let _ignored = tx.send(fut.await);
        }))
        .expect("runtime thread alive");
    rx.blocking_recv().expect("job ran to completion")
}

#[cfg(test)]
mod tests {
    use super::block_on_dedicated;

    #[test]
    fn dedicated_runtime_runs_a_future() {
        let out = block_on_dedicated(async { 40u32 + 2u32 });
        assert_eq!(out, 42u32);
    }

    #[test]
    fn jobs_run_sequentially_on_one_thread() {
        let tids: Vec<std::thread::ThreadId> = (0u32..4u32)
            .map(|i| {
                block_on_dedicated(async move {
                    tokio::task::yield_now().await;
                    (i, std::thread::current().id())
                })
                .1
            })
            .collect();
        assert!(tids.windows(2).all(|w| w[0] == w[1]));
    }
}
