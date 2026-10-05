//! Dedicated single-thread tokio runtime owned by this crate.
//!
//! One worker thread drives all binding futures. Async napi entry points
//! run here via the runtime installed below; no libuv thread ever blocks.
//! Single worker (not `new_current_thread`, which nobody would drive once
//! moved into napi): sequential execution by construction.

#[cfg(test)]
use std::future::Future;

/// Install the dedicated runtime into napi (module init path).
///
/// Idempotent: `create_custom_tokio_runtime` keeps the first runtime and
/// ignores later calls, so repeated `install()` calls only waste one build.
pub fn install() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("rivulus-node-rt")
        .enable_all()
        .build()
        .expect("dedicated runtime builds");
    napi::bindgen_prelude::create_custom_tokio_runtime(rt);
}

/// Run a future to completion on a throwaway runtime, blocking the caller.
///
/// Sync test plumbing only (unit tests below). Production async paths go
/// through the installed runtime via napi, never here.
#[cfg(test)]
pub fn block_on_dedicated<F, T>(fut: F) -> T
where
    F: Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("ephemeral runtime builds")
        .block_on(fut)
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
    fn installed_runtime_drives_spawned_tasks() {
        super::install();
        let (tx, rx) = std::sync::mpsc::channel::<u32>();
        napi::bindgen_prelude::spawn(async move {
            let _ignored = tx.send(7u32);
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("spawned task ran");
        assert_eq!(got, 7u32);
    }
}
