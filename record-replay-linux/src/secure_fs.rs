use anyhow::{bail, Context, Result};
use std::{
    fs,
    fs::{DirBuilder, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

pub const PRIVATE_DIR_MODE: u32 = 0o700;
pub const PRIVATE_FILE_MODE: u32 = 0o600;

pub struct DirectoryLock {
    path: PathBuf,
}

impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn ensure_no_symlink_components(path: &Path) -> Result<()> {
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        if cursor.as_os_str().is_empty() {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&cursor) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            bail!("path component must not be a symlink: {}", cursor.display());
        }
    }
    Ok(())
}

pub fn create_private_dir_all(path: &Path) -> Result<()> {
    ensure_no_symlink_components(path)?;
    let mut cursor = PathBuf::new();
    for component in path.components() {
        cursor.push(component.as_os_str());
        if cursor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    bail!("path component must not be a symlink: {}", cursor.display());
                }
                if !metadata.is_dir() {
                    bail!("path component is not a directory: {}", cursor.display());
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                create_private_dir(&cursor)
                    .with_context(|| format!("failed to create {}", cursor.display()))?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", cursor.display()));
            }
        }
    }
    Ok(())
}

pub fn create_new_private_dir(path: &Path) -> Result<()> {
    ensure_no_symlink_components(path)?;
    match fs::symlink_metadata(path) {
        Ok(_) => bail!("session directory already exists: {}", path.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            create_private_dir_all(parent)?;
        }
    }
    create_private_dir(path).with_context(|| format!("failed to create {}", path.display()))
}

pub fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_private_dir_all(parent)?;
        ensure_private_dir(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("record-replay");
    let tmp_path = path.with_file_name(format!(
        ".{file_name}.tmp.{}.{}",
        std::process::id(),
        unique_nanos()
    ));

    let write_result = (|| -> Result<()> {
        let mut file = private_open_options()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        file.write_all(contents.as_ref())
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", tmp_path.display()))?;
        drop(file);
        fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        set_private_file_mode(path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

pub fn append_private_line(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_private_dir_all(parent)?;
        ensure_private_dir(parent)?;
    }
    let mut file = private_open_options()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    set_private_file_mode(path)?;
    Ok(())
}

pub fn lock_directory(dir: &Path, name: &str) -> Result<DirectoryLock> {
    let lock_path = dir.join(name);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match private_open_options()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(DirectoryLock { path: lock_path });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists && Instant::now() < deadline => {
                if recover_stale_lock(&lock_path, false)? {
                    continue;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if recover_stale_lock(&lock_path, true)? {
                    continue;
                }
                bail!("timed out waiting for {}", lock_path.display());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to lock {}", lock_path.display()));
            }
        }
    }
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("path component must not be a symlink: {}", path.display());
    }
    if !metadata.is_dir() {
        bail!("path component is not a directory: {}", path.display());
    }
    #[cfg(unix)]
    {
        if metadata.uid() != effective_uid() {
            bail!(
                "private directory is not owned by current user: {}",
                path.display()
            );
        }
        let mode = metadata.permissions().mode() & 0o777;
        if mode != PRIVATE_DIR_MODE {
            set_private_dir_mode(path)?;
        }
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<()> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    builder.mode(PRIVATE_DIR_MODE);
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(error) => return Err(error.into()),
    }
    set_private_dir_mode(path)?;
    Ok(())
}

fn recover_stale_lock(path: &Path, recover_invalid: bool) -> Result<bool> {
    let observed = fs::read_to_string(path).unwrap_or_default();
    let pid = observed.trim().parse::<u32>().ok();
    let stale = match pid {
        Some(pid) => !pid_is_alive(pid),
        None => recover_invalid,
    };
    if !stale {
        return Ok(false);
    }

    if fs::read_to_string(path).unwrap_or_default() != observed {
        return Ok(false);
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn pid_is_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

fn private_open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    options.mode(PRIVATE_FILE_MODE);
    options
}

fn set_private_dir_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIR_MODE))
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }
    Ok(())
}

fn set_private_file_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_FILE_MODE))
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }
    Ok(())
}

fn unique_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
