use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineRecord {
    pub index: u64,
    pub recorded_at: String,
    #[serde(flatten)]
    pub event: TimelineEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum TimelineEvent {
    SessionStarted {
        #[serde(skip_serializing_if = "Option::is_none")]
        goal: Option<String>,
    },
    UserMarker {
        note: String,
    },
    SpeechContext {
        transcript: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        file: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    AudioRecording {
        #[serde(skip_serializing_if = "Option::is_none")]
        file: Option<String>,
        metadata_file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        status: String,
    },
    SessionStopped,
    SessionCancelled {
        discarded: bool,
    },
    SessionExpired,
    Navigation {
        url: String,
    },
    Screenshot {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    AccessibilitySnapshot {
        file: String,
        count: usize,
    },
    BrowserAction {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    BrowserTrace {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    DesktopSnapshot {
        file: String,
        window_count: usize,
        #[serde(default)]
        browser_observation_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_window_title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_window_app_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_window_wm_class: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_browser_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_browser_title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_browser_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_browser_domain: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        focused_browser_url_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    ProviderEvidence {
        provider: String,
        file: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    Diagnostic {
        level: String,
        message: String,
    },
    DraftPrompt {
        preview: String,
    },
    Observation {
        label: String,
        data: Value,
    },
}

impl TimelineRecord {
    pub fn new(index: u64, recorded_at: String, event: TimelineEvent) -> Self {
        Self {
            index,
            recorded_at,
            event,
        }
    }

    pub fn to_json_line(&self) -> Result<String> {
        serde_json::to_string(self).context("failed to serialize timeline record")
    }

    pub fn validate(&self) -> TimelineValidationReport {
        let mut report = TimelineValidationReport::default();
        if self.recorded_at.trim().is_empty() {
            report
                .errors
                .push(TimelineValidationError::MissingField("recorded_at"));
        }
        match &self.event {
            TimelineEvent::UserMarker { note } => {
                if note.trim().is_empty() {
                    report
                        .errors
                        .push(TimelineValidationError::MissingField("user_marker.note"));
                }
            }
            TimelineEvent::SpeechContext {
                transcript, file, ..
            } => {
                if transcript.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "speech_context.transcript",
                    ));
                }
                if let Some(file) = file {
                    validate_event_path(file, "speech_context.file", &mut report);
                }
            }
            TimelineEvent::AudioRecording {
                file,
                metadata_file,
                status,
                ..
            } => {
                if let Some(file) = file {
                    validate_event_path(file, "audio_recording.file", &mut report);
                }
                validate_event_path(metadata_file, "audio_recording.metadata_file", &mut report);
                if status.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "audio_recording.status",
                    ));
                }
            }
            TimelineEvent::Navigation { url } => {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    report
                        .warnings
                        .push("navigation url does not look like http/https".to_string());
                }
            }
            TimelineEvent::Screenshot { file, .. } => {
                validate_event_path(file, "screenshot.file", &mut report);
            }
            TimelineEvent::AccessibilitySnapshot { file, .. } => {
                validate_event_path(file, "accessibility_snapshot.file", &mut report);
            }
            TimelineEvent::BrowserAction { command, .. } => {
                if command.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "browser_action.command",
                    ));
                }
            }
            TimelineEvent::BrowserTrace { file, .. } => {
                validate_event_path(file, "browser_trace.file", &mut report);
            }
            TimelineEvent::DesktopSnapshot { file, .. } => {
                validate_event_path(file, "desktop_snapshot.file", &mut report);
            }
            TimelineEvent::ProviderEvidence {
                provider,
                file,
                status,
                ..
            } => {
                if provider.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "provider_evidence.provider",
                    ));
                }
                if status.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "provider_evidence.status",
                    ));
                }
                validate_event_path(file, "provider_evidence.file", &mut report);
            }
            TimelineEvent::Diagnostic { message, .. } => {
                if message.trim().is_empty() {
                    report
                        .errors
                        .push(TimelineValidationError::MissingField("diagnostic.message"));
                }
            }
            TimelineEvent::DraftPrompt { preview } => {
                if preview.trim().is_empty() {
                    report.errors.push(TimelineValidationError::MissingField(
                        "draft_prompt.preview",
                    ));
                }
            }
            TimelineEvent::Observation { label, .. } => {
                if label.trim().is_empty() {
                    report
                        .errors
                        .push(TimelineValidationError::MissingField("observation.label"));
                }
            }
            TimelineEvent::SessionStarted { .. }
            | TimelineEvent::SessionStopped
            | TimelineEvent::SessionCancelled { .. }
            | TimelineEvent::SessionExpired => {}
        }
        report
    }
}

pub fn parse_timeline_line(input: &str) -> Result<TimelineRecord, TimelineParseError> {
    serde_json::from_str(input).map_err(|error| TimelineParseError::Malformed(error.to_string()))
}

pub fn read_timeline(bundle_dir: &Path) -> Result<Vec<TimelineRecord>> {
    let path = bundle_dir.join(crate::manifest::TIMELINE_FILE_NAME);
    read_timeline_file(&path)
}

pub fn read_timeline_at(bundle_dir: &Path, rel_path: &str) -> Result<Vec<TimelineRecord>> {
    let path = crate::manifest::checked_bundle_path(bundle_dir, "timeline", rel_path)?;
    read_timeline_file(&path)
}

fn read_timeline_file(path: &Path) -> Result<Vec<TimelineRecord>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read timeline at {}", path.display()))?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| parse_timeline_line(line).map_err(anyhow::Error::from))
        .collect()
}

pub fn append_timeline_record(bundle_dir: &Path, event: TimelineEvent) -> Result<TimelineRecord> {
    let _lock = crate::secure_fs::lock_directory(bundle_dir, ".timeline.lock")?;
    let path = bundle_dir.join(crate::manifest::TIMELINE_FILE_NAME);
    let next_index = if path.exists() {
        fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u64
    } else {
        0
    };
    let record = TimelineRecord::new(next_index, crate::recorder::now_timestamp(), event);
    let line = record.to_json_line()?;
    crate::secure_fs::append_private_line(&path, &line)?;
    crate::event_stream::append_event_stream_record(bundle_dir, &record)?;
    Ok(record)
}

fn validate_event_path(value: &str, field: &'static str, report: &mut TimelineValidationReport) {
    if value.trim().is_empty() {
        report
            .errors
            .push(TimelineValidationError::MissingField(field));
        return;
    }
    if PathBuf::from(value).is_absolute() {
        report.errors.push(TimelineValidationError::InvalidPath {
            field,
            value: value.to_string(),
            reason: "must be relative".to_string(),
        });
    }
    if value.contains("..") || value.contains('\\') {
        report.errors.push(TimelineValidationError::InvalidPath {
            field,
            value: value.to_string(),
            reason: "must be normalized relative path".to_string(),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineParseError {
    MissingField(&'static str),
    Malformed(String),
}

impl fmt::Display for TimelineParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing timeline field: {field}"),
            Self::Malformed(message) => write!(f, "malformed timeline line: {message}"),
        }
    }
}

impl std::error::Error for TimelineParseError {}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TimelineValidationReport {
    pub errors: Vec<TimelineValidationError>,
    pub warnings: Vec<String>,
}

impl TimelineValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineValidationError {
    MissingField(&'static str),
    InvalidPath {
        field: &'static str,
        value: String,
        reason: String,
    },
}

impl fmt::Display for TimelineValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing timeline field: {field}"),
            Self::InvalidPath {
                field,
                value,
                reason,
            } => write!(f, "{field} path '{value}' is invalid: {reason}"),
        }
    }
}
