use codex_computer_use_linux::diagnostics::Check;
use codex_record_replay_linux::{
    recording_backend_catalog_from_signals, recording_backend_observation_from_signals,
    RecordingBackendSignals, RecordingBackendStatus, RecordingBundleManifest,
};

fn signals(input_capture_ok: bool, xdg_session_type: Option<&str>) -> RecordingBackendSignals {
    RecordingBackendSignals {
        input_capture: if input_capture_ok {
            Check {
                ok: true,
                detail: "portal input capture is available".to_string(),
            }
        } else {
            Check {
                ok: false,
                detail: "org.freedesktop.portal.InputCapture is unavailable".to_string(),
            }
        },
        xdg_session_type: xdg_session_type.map(str::to_string),
        xdg_current_desktop: None,
        can_build_accessibility_tree: false,
        can_query_windows: false,
        screenshot_backends: vec![],
    }
}

#[test]
fn catalog_reports_input_capture_and_x11_reasons() {
    let catalog = recording_backend_catalog_from_signals(&signals(false, Some("wayland")));

    let input_capture = catalog
        .iter()
        .find(|backend| backend.id == "input-capture-libei")
        .expect("input capture backend");
    assert_eq!(input_capture.status, RecordingBackendStatus::Missing);
    assert!(input_capture.reason.contains("libei"));
    assert!(input_capture.reason.contains("unavailable"));

    let x11_backend = catalog
        .iter()
        .find(|backend| backend.id == "x11-recording")
        .expect("x11 backend");
    assert_eq!(x11_backend.status, RecordingBackendStatus::Missing);
    assert!(x11_backend.reason.contains("X11"));
    assert!(x11_backend.reason.contains("wayland"));

    let browser_trace = catalog
        .iter()
        .find(|backend| backend.id == "browser-trace")
        .expect("browser backend");
    assert_eq!(browser_trace.status, RecordingBackendStatus::Available);
}

#[test]
fn x11_session_marks_the_x11_backend_available() {
    let catalog = recording_backend_catalog_from_signals(&signals(true, Some("x11")));

    let x11_backend = catalog
        .iter()
        .find(|backend| backend.id == "x11-recording")
        .expect("x11 backend");
    assert_eq!(x11_backend.status, RecordingBackendStatus::Available);
    assert!(x11_backend.reason.contains("X11 session"));
}

#[test]
fn backend_catalog_is_emitted_as_timeline_observation() {
    let observation = recording_backend_observation_from_signals(&signals(true, Some("x11")));

    match observation {
        codex_record_replay_linux::TimelineEvent::Observation { label, data } => {
            assert_eq!(label, "backend_catalog");
            let backends = data.as_array().expect("backend array");
            assert!(backends
                .iter()
                .any(|backend| backend["id"] == "input-capture-libei"));
            assert!(backends
                .iter()
                .any(|backend| backend["id"] == "x11-recording"));
        }
        other => panic!("unexpected timeline event: {other:?}"),
    }
}

#[test]
fn manifest_roundtrips_the_backend_catalog_field() {
    let catalog = recording_backend_catalog_from_signals(&signals(true, Some("x11")));
    let mut manifest = RecordingBundleManifest::new("session".to_string(), "now".to_string());
    manifest.backend_catalog = catalog.clone();

    let rendered = serde_json::to_string(&manifest).expect("manifest serializes");
    let reparsed: RecordingBundleManifest =
        serde_json::from_str(&rendered).expect("manifest parses");

    assert_eq!(reparsed.backend_catalog, catalog);
}
