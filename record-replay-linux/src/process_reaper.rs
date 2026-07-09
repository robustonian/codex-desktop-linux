use anyhow::{bail, Context, Result};
use std::{
    process::{Child, Command},
    sync::mpsc,
    thread,
};

pub(crate) fn spawn_reaped(command: &mut Command, context: &str) -> Result<u32> {
    let (sender, receiver) = mpsc::sync_channel::<Child>(1);
    thread::Builder::new()
        .name("codex-record-replay-reaper".to_string())
        .spawn(move || {
            if let Ok(mut child) = receiver.recv() {
                let _ = child.wait();
            }
        })
        .context("failed to spawn reaper thread before child process start")?;

    let mut child = command.spawn().with_context(|| context.to_string())?;
    let pid = child.id();
    if let Err(error) = sender.send(child) {
        child = error.0;
        let _ = child.kill();
        let _ = child.wait();
        bail!("failed to hand child process {pid} to reaper thread");
    }
    Ok(pid)
}
