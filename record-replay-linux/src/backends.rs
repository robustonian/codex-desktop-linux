use codex_computer_use_linux::diagnostics::{Check, DoctorReport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::timeline::TimelineEvent;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RecordingBackendSignals {
    pub input_capture: Check,
    pub xdg_session_type: Option<String>,
    pub xdg_current_desktop: Option<String>,
    pub can_build_accessibility_tree: bool,
    pub can_query_windows: bool,
    pub screenshot_backends: Vec<String>,
}

impl RecordingBackendSignals {
    pub fn from_diagnostics(diagnostics: &DoctorReport) -> Self {
        Self {
            input_capture: diagnostics.portals.input_capture.clone(),
            xdg_session_type: diagnostics.platform.xdg_session_type.clone(),
            xdg_current_desktop: diagnostics.platform.xdg_current_desktop.clone(),
            can_build_accessibility_tree: diagnostics.readiness.can_build_accessibility_tree,
            can_query_windows: diagnostics.readiness.can_query_windows,
            screenshot_backends: diagnostics.capabilities.screenshot.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordingBackendStatus {
    Available,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordingBackend {
    pub id: String,
    pub label: String,
    pub layer: String,
    pub status: RecordingBackendStatus,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub fn recording_backend_catalog(diagnostics: &DoctorReport) -> Vec<RecordingBackend> {
    recording_backend_catalog_from_signals(&RecordingBackendSignals::from_diagnostics(diagnostics))
}

pub fn recording_backend_catalog_from_signals(
    signals: &RecordingBackendSignals,
) -> Vec<RecordingBackend> {
    let mut catalog = Vec::new();

    catalog.push(RecordingBackend {
        id: "browser-trace".to_string(),
        label: "Browser trace".to_string(),
        layer: "browser".to_string(),
        status: RecordingBackendStatus::Available,
        reason: "browser trace is the semantic baseline".to_string(),
        evidence: vec!["always-on semantic recorder".to_string()],
        notes: vec!["Replay remains skill-driven".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "at-spi".to_string(),
        label: "AT-SPI accessibility".to_string(),
        layer: "accessibility".to_string(),
        status: status_from_bool(signals.can_build_accessibility_tree),
        reason: if signals.can_build_accessibility_tree {
            "AT-SPI accessibility tree is available".to_string()
        } else {
            "AT-SPI accessibility tree is unavailable".to_string()
        },
        evidence: if signals.can_build_accessibility_tree {
            vec!["readiness.can_build_accessibility_tree=true".to_string()]
        } else {
            vec!["readiness.can_build_accessibility_tree=false".to_string()]
        },
        notes: vec!["Semantic UI path; not raw pointer replay".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "screenshot".to_string(),
        label: "Screenshot evidence".to_string(),
        layer: "visual".to_string(),
        status: if signals.screenshot_backends.is_empty() {
            RecordingBackendStatus::Missing
        } else {
            RecordingBackendStatus::Available
        },
        reason: if signals.screenshot_backends.is_empty() {
            "no screenshot backend detected".to_string()
        } else {
            format!(
                "screenshot backends available: {}",
                signals.screenshot_backends.join(", ")
            )
        },
        evidence: signals.screenshot_backends.clone(),
        notes: vec!["Visual evidence complements semantic backends".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "window-metadata".to_string(),
        label: "Window metadata".to_string(),
        layer: "window".to_string(),
        status: status_from_bool(signals.can_query_windows),
        reason: if signals.can_query_windows {
            "window query/focus metadata is available".to_string()
        } else {
            "window query/focus metadata is unavailable".to_string()
        },
        evidence: vec![format!(
            "readiness.can_query_windows={}",
            signals.can_query_windows
        )],
        notes: vec!["Window metadata helps explain target selection".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "input-capture-libei".to_string(),
        label: "InputCapture/libei".to_string(),
        layer: "input".to_string(),
        status: if signals.input_capture.ok {
            RecordingBackendStatus::Available
        } else {
            RecordingBackendStatus::Missing
        },
        reason: if signals.input_capture.ok {
            "portal input capture is available for libei-style capture readiness".to_string()
        } else {
            format!(
                "portal input capture is unavailable; libei readiness is missing ({})",
                signals.input_capture.detail
            )
        },
        evidence: vec![signals.input_capture.detail.clone()],
        notes: vec!["Readiness only; replay stays skill-driven".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "x11-recording".to_string(),
        label: "X11 recording metadata".to_string(),
        layer: "input".to_string(),
        status: if signals
            .xdg_session_type
            .as_deref()
            .is_some_and(|session| session.eq_ignore_ascii_case("x11"))
        {
            RecordingBackendStatus::Available
        } else {
            RecordingBackendStatus::Missing
        },
        reason: match signals.xdg_session_type.as_deref() {
            Some(session) if session.eq_ignore_ascii_case("x11") => {
                "X11 session detected; X11-specific recording metadata path is available"
                    .to_string()
            }
            Some(session) => {
                format!("X11-specific recording metadata path is unavailable on {session} sessions")
            }
            None => "X11-specific recording metadata path is unavailable without XDG_SESSION_TYPE"
                .to_string(),
        },
        evidence: signals
            .xdg_session_type
            .as_ref()
            .map(|session| vec![format!("XDG_SESSION_TYPE={session}")])
            .unwrap_or_else(|| vec!["XDG_SESSION_TYPE is unset".to_string()]),
        notes: vec!["X11 fallback is metadata-rich, not coordinate macro replay".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "user-markers".to_string(),
        label: "User markers".to_string(),
        layer: "semantic".to_string(),
        status: RecordingBackendStatus::Available,
        reason: "user markers are always accepted into the timeline".to_string(),
        evidence: vec!["timeline marker events".to_string()],
        notes: vec!["Used to bracket intent boundaries".to_string()],
    });

    catalog.push(RecordingBackend {
        id: "speech-transcript".to_string(),
        label: "Speech transcript".to_string(),
        layer: "semantic".to_string(),
        status: RecordingBackendStatus::Available,
        reason: "spoken context is always accepted into the bundle".to_string(),
        evidence: vec!["speech_context timeline events".to_string()],
        notes: vec!["Transcript data is semantic evidence, not audio replay".to_string()],
    });

    catalog
}

pub fn available_recorders(catalog: &[RecordingBackend]) -> Vec<String> {
    catalog
        .iter()
        .filter(|backend| backend.status == RecordingBackendStatus::Available)
        .map(|backend| backend.id.clone())
        .collect()
}

pub fn recording_backend_observation(diagnostics: &DoctorReport) -> TimelineEvent {
    recording_backend_observation_from_signals(&RecordingBackendSignals::from_diagnostics(
        diagnostics,
    ))
}

pub fn recording_backend_observation_from_signals(
    signals: &RecordingBackendSignals,
) -> TimelineEvent {
    TimelineEvent::Observation {
        label: "backend_catalog".to_string(),
        data: serde_json::to_value(recording_backend_catalog_from_signals(signals))
            .expect("backend catalog serializes"),
    }
}

fn status_from_bool(enabled: bool) -> RecordingBackendStatus {
    if enabled {
        RecordingBackendStatus::Available
    } else {
        RecordingBackendStatus::Missing
    }
}
