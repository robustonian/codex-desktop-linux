//! Shared test helpers.
//!
//! Several tests across modules mutate process-wide env vars
//! (`HOME`, `PATH`, `NVM_DIR`, `CODEX_CLI_PATH`, display sockets, ...) so
//! they can drive `command_path_env`, `npm_program`, and
//! `hydrate_session_bus_env` deterministically. Cargo runs unit tests in
//! parallel; without serialisation those mutations race across threads
//! — on a developer machine with `nvm` installed the tests would otherwise
//! pick up the real `~/.nvm/.../bin/npm` instead of the temp-dir fake. Each
//! test that touches env vars must hold this lock for its entire body.

use std::sync::{Mutex, MutexGuard, OnceLock};

pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}
