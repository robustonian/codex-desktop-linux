use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const DEFAULT_MAX_DURATION_SECONDS: i64 = 30 * 60;
const STATUS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRuntimeState {
    Active,
    Stopped,
    Canceled,
    Expired,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingRuntimeStatus {
    pub ok: bool,
    pub schema_version: u32,
    pub state: RecordingRuntimeState,
    pub session_dir: Option<PathBuf>,
    pub goal: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub end_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_duration_seconds: u64,
    pub last_event: Option<String>,
    pub status_path: PathBuf,
}

impl RecordingRuntimeStatus {
    pub fn idle(path: PathBuf) -> Self {
        Self {
            ok: true,
            schema_version: STATUS_SCHEMA_VERSION,
            state: RecordingRuntimeState::Idle,
            session_dir: None,
            goal: None,
            started_at: None,
            end_reason: None,
            updated_at: Utc::now(),
            expires_at: None,
            max_duration_seconds: DEFAULT_MAX_DURATION_SECONDS as u64,
            last_event: None,
            status_path: path,
        }
    }

    fn refresh_expiry(mut self) -> Self {
        if matches!(self.state, RecordingRuntimeState::Active) {
            if let Some(expires_at) = self.expires_at {
                if Utc::now() > expires_at {
                    self.state = RecordingRuntimeState::Expired;
                    self.last_event = Some("expired".to_string());
                    self.end_reason = Some("max_duration".to_string());
                }
            }
        }
        self
    }
}

pub fn status_path() -> PathBuf {
    if let Some(path) = env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH").map(PathBuf::from) {
        return path;
    }
    runtime_root()
        .join("codex-record-replay")
        .join("status.json")
}

pub fn read_runtime_status() -> RecordingRuntimeStatus {
    let path = status_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return RecordingRuntimeStatus::idle(path);
    };
    let Ok(mut status) = serde_json::from_str::<RecordingRuntimeStatus>(&raw) else {
        return RecordingRuntimeStatus::idle(path);
    };
    status.status_path = path;
    status.refresh_expiry()
}

pub fn refresh_runtime_status() -> RecordingRuntimeStatus {
    let status = read_runtime_status();
    if matches!(status.state, RecordingRuntimeState::Expired) {
        if let Some(session_dir) = status.session_dir.as_ref() {
            if !manifest_has_end_reason(session_dir) {
                let _ = crate::recorder::expire_session(session_dir);
            }
        }
        return read_runtime_status();
    }
    status
}

pub(crate) fn expired_status_for(session_dir: &Path) -> bool {
    let status = read_runtime_status();
    status.session_dir.as_deref() == Some(session_dir)
        && matches!(status.state, RecordingRuntimeState::Expired)
}

pub fn write_active_status(
    session_dir: &Path,
    goal: Option<String>,
) -> Result<RecordingRuntimeStatus> {
    let path = status_path();
    let now = Utc::now();
    let status = RecordingRuntimeStatus {
        ok: true,
        schema_version: STATUS_SCHEMA_VERSION,
        state: RecordingRuntimeState::Active,
        session_dir: Some(session_dir.to_path_buf()),
        goal,
        started_at: Some(now),
        end_reason: None,
        updated_at: now,
        expires_at: Some(now + Duration::seconds(DEFAULT_MAX_DURATION_SECONDS)),
        max_duration_seconds: DEFAULT_MAX_DURATION_SECONDS as u64,
        last_event: Some("start".to_string()),
        status_path: path.clone(),
    };
    write_status(&path, &status)?;
    Ok(status)
}

pub fn update_active_status(event: &str) -> Result<Option<RecordingRuntimeStatus>> {
    update_active_status_for(None, event)
}

pub fn update_active_status_for(
    session_dir: Option<&Path>,
    event: &str,
) -> Result<Option<RecordingRuntimeStatus>> {
    let mut status = read_runtime_status();
    if !matches!(status.state, RecordingRuntimeState::Active) {
        return Ok(None);
    }
    if let Some(expected_dir) = session_dir {
        if status.session_dir.as_deref() != Some(expected_dir) {
            return Ok(None);
        }
    }
    status.updated_at = Utc::now();
    status.last_event = Some(event.to_string());
    write_status(&status.status_path, &status)?;
    Ok(Some(status))
}

pub fn write_stopped_status(session_dir: &Path) -> Result<RecordingRuntimeStatus> {
    let mut status = read_runtime_status();
    status.ok = true;
    status.schema_version = STATUS_SCHEMA_VERSION;
    status.state = RecordingRuntimeState::Stopped;
    status.session_dir = Some(session_dir.to_path_buf());
    status.updated_at = Utc::now();
    status.end_reason = Some("recording_controls_stopped".to_string());
    status.last_event = Some("stop".to_string());
    write_status(&status.status_path, &status)?;
    Ok(status)
}

pub fn write_canceled_status(
    session_dir: &Path,
    discarded: bool,
) -> Result<RecordingRuntimeStatus> {
    let mut status = read_runtime_status();
    status.ok = true;
    status.schema_version = STATUS_SCHEMA_VERSION;
    status.state = RecordingRuntimeState::Canceled;
    status.session_dir = Some(session_dir.to_path_buf());
    status.updated_at = Utc::now();
    status.end_reason = Some(if discarded {
        "recording_controls_cancelled_discarded".to_string()
    } else {
        "recording_controls_cancelled".to_string()
    });
    status.last_event = Some(if discarded {
        "cancel_discard".to_string()
    } else {
        "cancel".to_string()
    });
    write_status(&status.status_path, &status)?;
    Ok(status)
}

pub fn write_expired_status(session_dir: &Path) -> Result<RecordingRuntimeStatus> {
    let mut status = read_runtime_status();
    status.ok = true;
    status.schema_version = STATUS_SCHEMA_VERSION;
    status.state = RecordingRuntimeState::Expired;
    status.session_dir = Some(session_dir.to_path_buf());
    status.updated_at = Utc::now();
    status.end_reason = Some("max_duration".to_string());
    status.last_event = Some("expired".to_string());
    write_status(&status.status_path, &status)?;
    Ok(status)
}

fn write_status(path: &Path, status: &RecordingRuntimeStatus) -> Result<()> {
    if let Some(parent) = path.parent() {
        crate::secure_fs::create_private_dir_all(parent)
            .with_context(|| format!("failed to create status directory {}", parent.display()))?;
    }
    crate::secure_fs::write_private_file(
        path,
        format!("{}\n", serde_json::to_string_pretty(status)?),
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn runtime_root() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .map(|state| state.join("run"))
        })
        .unwrap_or_else(|| env::temp_dir().join(format!("codex-record-replay-{}", effective_uid())))
}

fn manifest_has_end_reason(session_dir: &Path) -> bool {
    crate::manifest::read_manifest(session_dir)
        .ok()
        .and_then(|manifest| manifest.end_reason)
        .is_some()
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

#[cfg(not(unix))]
fn effective_uid() -> u32 {
    0
}
