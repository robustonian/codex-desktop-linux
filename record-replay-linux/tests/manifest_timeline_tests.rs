use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use codex_record_replay_linux::{
    append_timeline_record, bundle_draft_prompt, cancel_session, command_json, expire_session,
    mark_session, parse_timeline_line, read_runtime_status, read_timeline, record_browser_trace,
    record_speech_context, start_session, stop_session, update_active_status, validate_bundle_dir,
    validate_draft_prompt, write_active_status, write_stopped_status, Commands, RecordCommand,
    RecordStartOptions, RecordingBundleManifest, RecordingRuntimeState, SessionCancelArgs,
    TimelineEvent,
};

const MANIFEST_VALID_FIXTURE: &str = include_str!("fixtures/manifest_valid.json");
const MANIFEST_INVALID_FIXTURE: &str = include_str!("fixtures/manifest_invalid_paths.json");
const TIMELINE_FIXTURE: &str = include_str!("fixtures/timeline.jsonl");
const DRAFT_PROMPT_VALID: &str = include_str!("fixtures/draft_prompt_valid.md");
const DRAFT_PROMPT_EMPTY: &str = include_str!("fixtures/draft_prompt_empty.md");

#[test]
fn manifest_fixture_roundtrips_through_serde() {
    let manifest: RecordingBundleManifest =
        serde_json::from_str(MANIFEST_VALID_FIXTURE).expect("valid fixture");
    assert!(manifest.validate().is_valid());
    let rendered = serde_json::to_string(&manifest).unwrap();
    let manifest_roundtrip: RecordingBundleManifest = serde_json::from_str(&rendered).unwrap();
    assert_eq!(manifest_roundtrip, manifest);
}

#[test]
fn manifest_rejects_absolute_and_dotdot_paths() {
    let manifest: RecordingBundleManifest = serde_json::from_str(MANIFEST_INVALID_FIXTURE).unwrap();
    let report = manifest.validate();
    assert!(!report.is_valid());
    assert!(report
        .errors
        .iter()
        .any(|err| err.to_string().contains("must be relative")
            || err.to_string().contains("must not contain ..")));
}

#[test]
fn timeline_parses_lines_and_serializes_roundtrip() {
    for (expected_index, line) in TIMELINE_FIXTURE.lines().enumerate() {
        let parsed = parse_timeline_line(line).expect("valid timeline line");
        assert_eq!(parsed.index as usize, expected_index);
        assert!(parsed.validate().is_valid());
        let rendered = parsed.to_json_line().unwrap();
        let reparsed = parse_timeline_line(&rendered).unwrap();
        assert_eq!(parsed, reparsed);
    }
}

#[test]
fn draft_prompt_validation_uses_fixture_files() {
    assert!(validate_draft_prompt(DRAFT_PROMPT_VALID).is_valid());
    assert!(!validate_draft_prompt(DRAFT_PROMPT_EMPTY).is_valid());
}

#[test]
fn timeline_has_expected_event_shape() {
    let mut lines = TIMELINE_FIXTURE.lines();
    let navigation = parse_timeline_line(lines.next().expect("navigation line")).unwrap();
    assert!(matches!(navigation.event, TimelineEvent::Navigation { .. }));
    if let TimelineEvent::Navigation { url } = navigation.event {
        assert!(url.ends_with('/'));
    }
    let screenshot = parse_timeline_line(lines.next().expect("screenshot line")).unwrap();
    assert!(matches!(screenshot.event, TimelineEvent::Screenshot { .. }));
    let accessibility = parse_timeline_line(lines.next().expect("accessibility line")).unwrap();
    assert!(matches!(
        accessibility.event,
        TimelineEvent::AccessibilitySnapshot { count: 3, .. }
    ));
}

#[test]
fn speech_context_is_timeline_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let manifest = RecordingBundleManifest::new(
        "speech-context".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(root, &manifest).unwrap();
    fs::write(root.join("timeline.jsonl"), "").unwrap();

    let record = record_speech_context(
        root,
        "Use my spoken description as the expected workflow intent.",
        Some("microphone-transcript".to_string()),
    )
    .unwrap();

    assert!(record.validate().is_valid());
    assert!(matches!(record.event, TimelineEvent::SpeechContext { .. }));
    let raw = fs::read_to_string(root.join("timeline.jsonl")).unwrap();
    assert!(raw.contains("speech_context"));
    assert!(raw.contains("microphone-transcript"));
    assert!(raw.contains("transcripts/0000.txt"));
    assert_eq!(
        fs::read_to_string(root.join("transcripts/0000.txt")).unwrap(),
        "Use my spoken description as the expected workflow intent.\n",
    );
    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt
        .contains("speech context: Use my spoken description as the expected workflow intent."));
    assert!(prompt.contains("file=transcripts/0000.txt"));
}

#[test]
fn browser_trace_is_bundle_artifact_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(root.join("timeline.jsonl"), "").unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let record = record_browser_trace(
        root,
        serde_json::json!({
            "events": [
                { "method": "Page.navigate", "params": { "url": "https://example.com/login" } }
            ]
        }),
        Some("https://example.com/login".to_string()),
        Some("Example Login".to_string()),
        Some("chrome-cdp".to_string()),
    )
    .unwrap();

    assert!(record.validate().is_valid());
    assert!(matches!(record.event, TimelineEvent::BrowserTrace { .. }));
    let timeline = read_timeline(root).unwrap();
    assert!(matches!(
        timeline.last(),
        Some(record) if matches!(&record.event, TimelineEvent::BrowserTrace { file, source, .. } if file == "browser/0000-trace.json" && source.as_deref() == Some("chrome-cdp"))
    ));
    assert!(root.join("browser/0000-trace.json").is_file());
    assert!(validate_bundle_dir(root).unwrap().is_valid());
    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt.contains("browser trace browser/0000-trace.json"));
    assert!(prompt.contains("Example Login"));
}

#[test]
fn desktop_snapshot_is_prompt_visible_bundle_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(root.join("timeline.jsonl"), "").unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();
    fs::write(
        root.join("x11/0000-desktop-snapshot.json"),
        "{\"ok\":true}\n",
    )
    .unwrap();

    let record = append_timeline_record(
        root,
        TimelineEvent::DesktopSnapshot {
            file: "x11/0000-desktop-snapshot.json".to_string(),
            window_count: 2,
            browser_observation_count: 1,
            focused_window_title: Some("Image Studio - Google Chrome".to_string()),
            focused_window_app_id: Some("google-chrome".to_string()),
            focused_window_wm_class: Some("Google-chrome".to_string()),
            focused_browser_name: Some("Google Chrome".to_string()),
            focused_browser_title: Some("Image Studio - Google Chrome".to_string()),
            focused_browser_url: Some("https://image-studio.example/".to_string()),
            focused_browser_domain: Some("image-studio.example".to_string()),
            focused_browser_url_source: Some("browser_trace".to_string()),
            source: Some("record-replay-hud".to_string()),
        },
    )
    .unwrap();

    assert!(record.validate().is_valid());
    assert!(validate_bundle_dir(root).unwrap().is_valid());
    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt.contains("desktop snapshot x11/0000-desktop-snapshot.json"));
    assert!(prompt.contains("Image Studio - Google Chrome"));
    assert!(prompt.contains("google-chrome"));
    assert!(prompt.contains("browser_url=https://image-studio.example/"));
    assert!(prompt.contains("browser_domain=image-studio.example"));
    assert!(prompt.contains("record-replay-hud"));
}

#[test]
fn event_stream_uses_sky_compatible_event_kinds() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: Some("record image generation workflow".to_string()),
            include_screenshot: false,
            include_accessibility: false,
            include_audio: false,
        }))
        .unwrap();

    record_speech_context(
        &root,
        "Open Chrome, open the image workspace, enter the image prompt, generate it, and download the image.",
        Some("codex-dictation-send".to_string()),
    )
    .unwrap();
    record_browser_trace(
        &root,
        serde_json::json!({
            "events": [
                { "method": "Page.navigate", "params": { "url": "https://image-studio.example/app" } }
            ]
        }),
        Some("https://image-studio.example/app".to_string()),
        Some("Image Studio - Google Chrome".to_string()),
        Some("chrome-cdp".to_string()),
    )
    .unwrap();
    fs::write(
        root.join("x11/0002-desktop-snapshot.json"),
        serde_json::json!({
            "schema_version": 1,
            "provider": "window-metadata",
            "captured_at": "2026-06-30T20:20:41Z",
            "source": "record-replay-hud",
            "windows": [
                {
                    "window_id": 42,
                    "title": "Image Studio - Google Chrome",
                    "app_id": "google-chrome",
                    "wm_class": "Google-chrome",
                    "focused": true,
                    "hidden": false,
                    "backend": "test"
                }
            ],
            "focused_window": {
                "window_id": 42,
                "title": "Image Studio - Google Chrome",
                "app_id": "google-chrome",
                "wm_class": "Google-chrome",
                "pid": 1234
            },
            "window_count": 1,
            "browser_observation_count": 1,
            "focused_browser_observation": {
                "browser": "Google Chrome",
                "window_id": 42,
                "title": "Image Studio - Google Chrome",
                "app_id": "google-chrome",
                "wm_class": "Google-chrome",
                "pid": 1234,
                "focused": true,
                "url": "https://image-studio.example/",
                "domain": "image-studio.example",
                "url_source": "browser_trace"
            }
        })
        .to_string(),
    )
    .unwrap();
    append_timeline_record(
        &root,
        TimelineEvent::DesktopSnapshot {
            file: "x11/0002-desktop-snapshot.json".to_string(),
            window_count: 1,
            browser_observation_count: 1,
            focused_window_title: Some("Image Studio - Google Chrome".to_string()),
            focused_window_app_id: Some("google-chrome".to_string()),
            focused_window_wm_class: Some("Google-chrome".to_string()),
            focused_browser_name: Some("Google Chrome".to_string()),
            focused_browser_title: Some("Image Studio - Google Chrome".to_string()),
            focused_browser_url: Some("https://image-studio.example/".to_string()),
            focused_browser_domain: Some("image-studio.example".to_string()),
            focused_browser_url_source: Some("browser_trace".to_string()),
            source: Some("record-replay-hud".to_string()),
        },
    )
    .unwrap();
    stop_session(&root).unwrap();

    let event_stream = read_jsonl_values(&root.join("events.jsonl"));
    let kinds = event_stream
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"session.started"));
    assert!(kinds.contains(&"keyboard.text_input"));
    assert!(kinds.contains(&"window.changed"));
    assert!(kinds.contains(&"session.ended"));
    assert!(!kinds.contains(&"session_started"));
    assert!(!kinds.contains(&"speech_context"));
    assert!(!kinds.contains(&"desktop_snapshot"));
    assert!(event_stream.iter().any(|event| {
        event["kind"] == "keyboard.text_input"
            && event["text"]
                .as_str()
                .is_some_and(|text| text.contains("Open Chrome, open the image workspace"))
            && event["target"]["file"] == "transcripts/0000.txt"
    }));
    assert!(event_stream.iter().any(|event| {
        event["kind"] == "window.changed"
            && event["title"] == "Image Studio - Google Chrome"
            && event["browser"] == "Google Chrome"
            && event["url"] == "https://image-studio.example/"
            && event["bundleIdentifier"] == "google-chrome"
            && event["target"]["browserDomain"] == "image-studio.example"
            && event["target"]["file"] == "x11/0002-desktop-snapshot.json"
    }));
    let session: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("session.json")).unwrap()).unwrap();
    assert_eq!(
        session["eventCount"].as_u64(),
        Some(event_stream.len() as u64)
    );
    assert!(session["suppressedEventCount"].as_u64().is_some());

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn image_generation_workflow_bundle_prompt_has_transcript_browser_and_window_context() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: Some("record image generation workflow".to_string()),
            include_screenshot: false,
            include_accessibility: false,
            include_audio: false,
        }))
        .unwrap();

    record_speech_context(
        &root,
        "Open Chrome, open the image workspace, enter a prompt to create an image of a neon cabin, generate it, then download the image.",
        Some("codex-dictation-send".to_string()),
    )
    .unwrap();
    let browser_record = record_browser_trace(
        &root,
        serde_json::json!({
            "events": [
                {
                    "method": "Page.navigate",
                    "params": { "url": "https://image-studio.example/app" }
                },
                {
                    "method": "Runtime.consoleAPICalled",
                    "params": { "text": "image prompt submitted" }
                }
            ]
        }),
        Some("https://image-studio.example/app".to_string()),
        Some("Image Studio - Google Chrome".to_string()),
        Some("chrome-cdp".to_string()),
    )
    .unwrap();
    let browser_trace_file = match &browser_record.event {
        TimelineEvent::BrowserTrace { file, .. } => file.as_str(),
        _ => panic!("expected browser trace record"),
    };
    fs::write(
        root.join("x11/0002-desktop-snapshot.json"),
        serde_json::json!({
            "schema_version": 1,
            "provider": "window-metadata",
            "captured_at": "2026-06-30T20:20:41Z",
            "source": "record-replay-hud",
            "windows": [
                {
                    "window_id": 42,
                    "title": "Image Studio - Google Chrome",
                    "app_id": "google-chrome",
                    "wm_class": "Google-chrome",
                    "focused": true,
                    "hidden": false,
                    "backend": "test"
                }
            ],
            "focused_window": {
                "title": "Image Studio - Google Chrome",
                "app_id": "google-chrome",
                "wm_class": "Google-chrome"
            },
            "window_count": 1,
            "browser_observation_count": 1,
            "focused_browser_observation": {
                "browser": "Google Chrome",
                "title": "Image Studio - Google Chrome",
                "focused": true,
                "url": "https://image-studio.example/",
                "domain": "image-studio.example",
                "url_source": "browser_trace"
            }
        })
        .to_string(),
    )
    .unwrap();
    append_timeline_record(
        &root,
        TimelineEvent::DesktopSnapshot {
            file: "x11/0002-desktop-snapshot.json".to_string(),
            window_count: 1,
            browser_observation_count: 1,
            focused_window_title: Some("Image Studio - Google Chrome".to_string()),
            focused_window_app_id: Some("google-chrome".to_string()),
            focused_window_wm_class: Some("Google-chrome".to_string()),
            focused_browser_name: Some("Google Chrome".to_string()),
            focused_browser_title: Some("Image Studio - Google Chrome".to_string()),
            focused_browser_url: Some("https://image-studio.example/".to_string()),
            focused_browser_domain: Some("image-studio.example".to_string()),
            focused_browser_url_source: Some("browser_trace".to_string()),
            source: Some("record-replay-hud".to_string()),
        },
    )
    .unwrap();

    let prompt = bundle_draft_prompt(&root).unwrap();
    assert!(root.join("session.json").is_file());
    assert!(root.join("events.jsonl").is_file());
    assert!(root.join("transcripts/0000.txt").is_file());
    assert!(root.join(browser_trace_file).is_file());
    assert!(root.join("x11/0002-desktop-snapshot.json").is_file());
    let event_stream = fs::read_to_string(root.join("events.jsonl")).unwrap();
    assert!(event_stream.contains("speech_context"));
    assert!(event_stream.contains("browser_trace"));
    assert!(event_stream.contains("desktop_snapshot"));
    assert!(prompt.contains("Open Chrome, open the image workspace"));
    assert!(prompt.contains("create an image of a neon cabin"));
    assert!(prompt.contains("https://image-studio.example/app"));
    assert!(prompt.contains("Image Studio - Google Chrome"));
    assert!(prompt.contains("google-chrome"));
    assert!(prompt.contains("download the image"));

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn start_session_writes_browser_input_capture_and_x11_evidence() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let report = runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: Some("record backend evidence".to_string()),
            include_screenshot: false,
            include_accessibility: false,
            include_audio: false,
        }))
        .unwrap();

    assert!(report.ok);
    assert!(root.join("session.json").is_file());
    assert!(root.join("events.jsonl").is_file());
    assert!(root.join("browser/0000-readiness.json").is_file());
    assert!(root.join("input-capture/0000-readiness.json").is_file());
    assert!(root.join("x11/0000-session.json").is_file());
    assert!(root.join("x11/0001-window-metadata.json").is_file());
    let timeline = read_timeline(&root).unwrap();
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::ProviderEvidence { provider, file, .. } if provider == "browser-trace" && file == "browser/0000-readiness.json")
    }));
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::ProviderEvidence { provider, file, .. } if provider == "input-capture-libei" && file == "input-capture/0000-readiness.json")
    }));
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::ProviderEvidence { provider, file, .. } if provider == "x11-recording" && file == "x11/0000-session.json")
    }));
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::ProviderEvidence { provider, file, .. } if provider == "window-metadata" && file == "x11/0001-window-metadata.json")
    }));
    assert!(validate_bundle_dir(&root).unwrap().is_valid());

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[cfg(unix)]
#[test]
fn start_session_creates_private_bundle_and_status_files() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let status_path = temp.path().join("status").join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: Some("private bundle".to_string()),
            include_screenshot: false,
            include_accessibility: false,
            include_audio: false,
        }))
        .unwrap();

    for dir in [
        root.as_path(),
        root.join("browser").as_path(),
        root.join("input-capture").as_path(),
        root.join("x11").as_path(),
        status_path.parent().unwrap(),
    ] {
        let mode = fs::metadata(dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{} should be private", dir.display());
    }
    for file in [
        root.join("manifest.json"),
        root.join("session.json"),
        root.join("timeline.jsonl"),
        root.join("events.jsonl"),
        root.join("diagnostics.json"),
        root.join("browser/0000-readiness.json"),
        root.join("input-capture/0000-readiness.json"),
        root.join("x11/0000-session.json"),
        status_path.clone(),
    ] {
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{} should be private", file.display());
    }

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn start_and_stop_session_records_audio_metadata_without_composer_dictation() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let status_path = temp.path().join("status.json");
    let previous_status = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    let previous_audio = std::env::var_os("CODEX_RECORD_REPLAY_AUDIO");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    std::env::set_var("CODEX_RECORD_REPLAY_AUDIO", "0");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: Some("audio metadata".to_string()),
            include_screenshot: false,
            include_accessibility: false,
            include_audio: true,
        }))
        .unwrap();

    assert!(root.join("audio/recording.json").is_file());
    let timeline = read_timeline(&root).unwrap();
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::AudioRecording { status, metadata_file, .. }
            if status == "disabled" && metadata_file == "audio/recording.json")
    }));

    stop_session(&root).unwrap();
    let timeline = read_timeline(&root).unwrap();
    assert!(timeline.iter().any(|record| {
        matches!(&record.event, TimelineEvent::AudioRecording { status, metadata_file, .. }
            if status == "stopped" && metadata_file == "audio/recording.json")
    }));
    assert!(validate_bundle_dir(&root).unwrap().is_valid());

    match previous_status {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
    match previous_audio {
        Some(value) => std::env::set_var("CODEX_RECORD_REPLAY_AUDIO", value),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_AUDIO"),
    }
}

#[cfg(unix)]
#[test]
fn runtime_status_tightens_preexisting_private_directory() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let status_dir = temp.path().join("runtime");
    fs::create_dir(&status_dir).unwrap();
    fs::set_permissions(&status_dir, fs::Permissions::from_mode(0o777)).unwrap();
    let status_path = status_dir.join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    write_active_status(
        &temp.path().join("bundle"),
        Some("private runtime".to_string()),
    )
    .unwrap();

    let mode = fs::metadata(&status_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "preexisting status directory should be tightened"
    );

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[cfg(unix)]
#[test]
fn start_session_rejects_existing_or_symlink_session_dir() {
    use std::os::unix::fs as unix_fs;

    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("timeline.jsonl"), "do not truncate\n").unwrap();
    let result = runtime.block_on(start_session(RecordStartOptions {
        session_dir: existing.clone(),
        app_id: None,
        window_id: None,
        goal: None,
        include_screenshot: false,
        include_accessibility: false,
        include_audio: false,
    }));
    assert!(
        result.is_err(),
        "existing bundle directories must not be reused"
    );
    assert_eq!(
        fs::read_to_string(existing.join("timeline.jsonl")).unwrap(),
        "do not truncate\n"
    );

    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    let link = temp.path().join("bundle-link");
    unix_fs::symlink(&target, &link).unwrap();
    let result = runtime.block_on(start_session(RecordStartOptions {
        session_dir: link,
        app_id: None,
        window_id: None,
        goal: None,
        include_screenshot: false,
        include_accessibility: false,
        include_audio: false,
    }));
    assert!(result.is_err(), "session_dir symlinks must be rejected");

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn sealed_sessions_reject_mutations_and_terminal_rewrites() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(start_session(RecordStartOptions {
            session_dir: root.clone(),
            app_id: None,
            window_id: None,
            goal: None,
            include_screenshot: false,
            include_accessibility: false,
            include_audio: false,
        }))
        .unwrap();

    stop_session(&root).unwrap();
    let stopped_manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    let stopped_timeline = fs::read_to_string(root.join("timeline.jsonl")).unwrap();

    assert!(mark_session(&root, "after stop").is_err());
    assert!(record_speech_context(&root, "after stop", None).is_err());
    assert!(
        record_browser_trace(&root, serde_json::json!({"after":"stop"}), None, None, None).is_err()
    );
    assert!(stop_session(&root).is_err());

    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(manifest.end_reason, stopped_manifest.end_reason);
    assert_eq!(
        fs::read_to_string(root.join("timeline.jsonl")).unwrap(),
        stopped_timeline
    );

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn status_command_persists_expired_session() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    create_standard_bundle_dirs(&root);
    let manifest = RecordingBundleManifest::new(
        "expired-session".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{}}\n",
    )
    .unwrap();
    fs::write(root.join("diagnostics.json"), "{}\n").unwrap();

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    fs::write(
        &status_path,
        serde_json::json!({
            "ok": true,
            "schema_version": 1,
            "state": "active",
            "session_dir": root,
            "goal": null,
            "started_at": "2026-06-28T12:00:00Z",
            "updated_at": "2026-06-28T12:00:00Z",
            "expires_at": "2026-06-28T12:00:01Z",
            "max_duration_seconds": 1800,
            "last_event": "start",
            "status_path": status_path,
        })
        .to_string(),
    )
    .unwrap();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let value = runtime.block_on(command_json(Commands::Status)).unwrap();
    assert_eq!(
        value.get("state").and_then(serde_json::Value::as_str),
        Some("expired")
    );

    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(manifest.end_reason.as_deref(), Some("max_duration"));
    assert!(read_timeline(&root)
        .unwrap()
        .iter()
        .any(|record| matches!(record.event, TimelineEvent::SessionExpired)));
    assert_eq!(read_runtime_status().state, RecordingRuntimeState::Expired);

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn stale_recording_lock_is_recovered_for_dead_pid() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("timeline.jsonl"), "").unwrap();
    let manifest =
        RecordingBundleManifest::new("stale-lock".to_string(), "2026-06-28T12:00:00Z".to_string());
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();
    fs::write(root.join(".recording.lock"), "999999999\n").unwrap();

    let record = mark_session(&root, "after stale lock").unwrap();

    assert!(matches!(record.event, TimelineEvent::UserMarker { .. }));
    assert!(
        !root.join(".recording.lock").exists(),
        "lock file should be released after successful mark"
    );
}

#[test]
fn validate_bundle_rejects_duplicate_timeline_indexes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        concat!(
            "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{}}\n",
            "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:01Z\",\"kind\":\"user_marker\",\"payload\":{\"note\":\"duplicate index\"}}\n",
        ),
    )
    .unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{}\n").unwrap();

    let report = validate_bundle_dir(root).unwrap();
    assert!(
        !report.is_valid(),
        "duplicate timeline indexes must invalidate the bundle"
    );
}

#[test]
fn validate_bundle_rejects_unsafe_speech_context_file_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        concat!(
            "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{}}\n",
            "{\"index\":1,\"recorded_at\":\"2026-06-28T12:00:01Z\",\"kind\":\"speech_context\",\"payload\":{\"transcript\":\"unsafe path\",\"file\":\"/tmp/transcript.txt\"}}\n",
        ),
    )
    .unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{}\n").unwrap();

    let report = validate_bundle_dir(root).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|error| {
        error.to_string().contains("speech_context.file")
            && error.to_string().contains("must be relative")
    }));
}

#[test]
fn explicit_stop_after_expiry_persists_max_duration_end_reason() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{}}\n",
    )
    .unwrap();
    let manifest = RecordingBundleManifest::new(
        "expired-before-stop".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    fs::write(
        &status_path,
        serde_json::json!({
            "ok": true,
            "schema_version": 1,
            "state": "active",
            "session_dir": root,
            "goal": null,
            "started_at": "2026-06-28T12:00:00Z",
            "updated_at": "2026-06-28T12:00:00Z",
            "expires_at": "2026-06-28T12:00:01Z",
            "max_duration_seconds": 1800,
            "last_event": "start",
            "status_path": status_path,
        })
        .to_string(),
    )
    .unwrap();

    let record = stop_session(&root).unwrap();

    assert!(matches!(record.event, TimelineEvent::SessionExpired));
    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(manifest.end_reason.as_deref(), Some("max_duration"));
    assert_eq!(read_runtime_status().state, RecordingRuntimeState::Expired);

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn explicit_cancel_after_expiry_persists_max_duration_end_reason() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{}}\n",
    )
    .unwrap();
    let manifest = RecordingBundleManifest::new(
        "expired-before-cancel".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    fs::write(
        &status_path,
        serde_json::json!({
            "ok": true,
            "schema_version": 1,
            "state": "active",
            "session_dir": root,
            "goal": null,
            "started_at": "2026-06-28T12:00:00Z",
            "updated_at": "2026-06-28T12:00:00Z",
            "expires_at": "2026-06-28T12:00:01Z",
            "max_duration_seconds": 1800,
            "last_event": "start",
            "status_path": status_path,
        })
        .to_string(),
    )
    .unwrap();

    let record = cancel_session(&root, true).unwrap();

    assert!(matches!(record.event, TimelineEvent::SessionExpired));
    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(manifest.end_reason.as_deref(), Some("max_duration"));
    assert_eq!(read_runtime_status().state, RecordingRuntimeState::Expired);

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn draft_prompt_marks_captured_material_as_untrusted() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(root.join("timeline.jsonl"), TIMELINE_FIXTURE).unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let prompt = bundle_draft_prompt(root).unwrap();
    let lower = prompt.to_lowercase();
    assert!(lower.contains("untrusted"));
    assert!(lower.contains("do not follow"));
    let mode = fs::metadata(root.join("draft-prompt.md"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn runtime_status_tracks_active_and_stopped_recording() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);

    let session_dir = temp.path().join("bundle");
    let active =
        write_active_status(&session_dir, Some("record parity smoke".to_string())).unwrap();
    assert_eq!(active.state, RecordingRuntimeState::Active);
    assert_eq!(
        read_runtime_status().session_dir.as_deref(),
        Some(session_dir.as_path())
    );

    update_active_status("mark").unwrap();
    let marked = read_runtime_status();
    assert_eq!(marked.state, RecordingRuntimeState::Active);
    assert_eq!(marked.last_event.as_deref(), Some("mark"));

    let stopped = write_stopped_status(&session_dir).unwrap();
    assert_eq!(stopped.state, RecordingRuntimeState::Stopped);
    assert_eq!(read_runtime_status().last_event.as_deref(), Some("stop"));

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn validates_bundle_directory_and_generates_draft_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(root.join("timeline.jsonl"), TIMELINE_FIXTURE).unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();
    fs::write(root.join("draft-prompt.md"), DRAFT_PROMPT_VALID).unwrap();

    assert!(validate_bundle_dir(root).unwrap().is_valid());
    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt.contains("Draft a Codex skill"));
    assert!(prompt.contains("speech context"));
    assert!(prompt.contains("Timeline"));
    assert!(validate_draft_prompt(&prompt).is_valid());
}

#[test]
fn validates_bundle_before_draft_prompt_is_generated() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("manifest.json"), MANIFEST_VALID_FIXTURE).unwrap();
    fs::write(root.join("timeline.jsonl"), TIMELINE_FIXTURE).unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let report = validate_bundle_dir(root).unwrap();
    assert!(report.is_valid(), "{report:?}");
}

#[test]
fn draft_prompt_rejects_manifest_paths_that_escape_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut manifest: RecordingBundleManifest =
        serde_json::from_str(MANIFEST_VALID_FIXTURE).expect("valid fixture");
    manifest.files.draft_prompt = "../escape.md".to_string();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(root.join("timeline.jsonl"), TIMELINE_FIXTURE).unwrap();
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let error = bundle_draft_prompt(root).unwrap_err().to_string();
    assert!(error.contains("bundle manifest is invalid"));
    assert!(!temp.path().join("escape.md").exists());
}

#[test]
fn draft_prompt_uses_manifest_timeline_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut manifest: RecordingBundleManifest =
        serde_json::from_str(MANIFEST_VALID_FIXTURE).expect("valid fixture");
    manifest.files.timeline = "events/custom.jsonl".to_string();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::create_dir(root.join("events")).unwrap();
    fs::write(root.join("events/custom.jsonl"), TIMELINE_FIXTURE).unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt.contains("navigation to https://example.com/"));
}

#[test]
fn bundle_draft_prompt_refuses_canceled_bundles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let mut manifest = RecordingBundleManifest::new(
        "fixture-session".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    manifest.end_reason = Some("recording_controls_canceled_discarded".to_string());
    manifest.ended_at = Some("2026-06-28T12:05:00Z".to_string());
    codex_record_replay_linux::manifest::write_manifest(root, &manifest).unwrap();
    fs::write(
        root.join("timeline.jsonl"),
        concat!(
            "{\"index\":0,\"recorded_at\":\"2026-06-28T12:00:00Z\",\"kind\":\"session_started\",\"payload\":{\"goal\":\"Record a browser login workflow and turn it into a skill.\"}}\n",
            "{\"index\":1,\"recorded_at\":\"2026-06-28T12:05:00Z\",\"kind\":\"session_cancelled\",\"payload\":{\"discarded\":true}}\n"
        ),
    )
    .unwrap();
    create_standard_bundle_dirs(root);
    fs::write(root.join("diagnostics.json"), "{\"ok\":true}\n").unwrap();

    let prompt = bundle_draft_prompt(root).unwrap();
    assert!(prompt.contains("canceled") || prompt.contains("discarded"));
    assert!(prompt.contains("Do not draft a reusable skill from this bundle."));
}

#[test]
fn record_cancel_marks_bundle_as_canceled_and_discarded() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();

    let manifest = RecordingBundleManifest::new(
        "fixture-session".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    write_active_status(&root, Some("cancel parity smoke".to_string())).unwrap();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let response = runtime
        .block_on(command_json(codex_record_replay_linux::Commands::Record {
            command: RecordCommand::Cancel(SessionCancelArgs {
                session_dir: root.clone(),
                discarded: true,
            }),
        }))
        .unwrap();

    assert_eq!(response["ok"], true);
    assert_eq!(response["command"], "record.cancel");
    assert_eq!(response["discarded"], true);
    assert_eq!(response["isRecording"], false);
    assert_eq!(
        response["endReason"],
        "recording_controls_cancelled_discarded"
    );
    assert_eq!(
        response["sessionDirectoryPath"].as_str(),
        Some(root.to_string_lossy().as_ref())
    );
    assert_eq!(
        response["eventsPath"].as_str(),
        Some(root.join("events.jsonl").to_string_lossy().as_ref())
    );
    assert_eq!(
        response["metadataPath"].as_str(),
        Some(root.join("session.json").to_string_lossy().as_ref())
    );
    let status = read_runtime_status();
    assert_eq!(status.state, RecordingRuntimeState::Canceled);
    assert_eq!(
        status.end_reason.as_deref(),
        Some("recording_controls_cancelled_discarded")
    );

    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(
        manifest.end_reason.as_deref(),
        Some("recording_controls_cancelled_discarded")
    );
    assert!(manifest.ended_at.is_some());

    let timeline = read_timeline(&root).unwrap();
    assert!(
        matches!(timeline.last(), Some(record) if matches!(&record.event, TimelineEvent::SessionCancelled { discarded } if *discarded))
    );

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

#[test]
fn record_expire_marks_bundle_at_max_duration() {
    let _guard = status_env_guard();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("bundle");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("timeline.jsonl"), "").unwrap();

    let manifest = RecordingBundleManifest::new(
        "fixture-session".to_string(),
        "2026-06-28T12:00:00Z".to_string(),
    );
    codex_record_replay_linux::manifest::write_manifest(&root, &manifest).unwrap();

    let status_path = temp.path().join("status.json");
    let previous = std::env::var_os("CODEX_RECORD_REPLAY_STATUS_PATH");
    std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", &status_path);
    write_active_status(&root, Some("max duration parity smoke".to_string())).unwrap();

    let record = expire_session(&root).unwrap();
    assert!(matches!(record.event, TimelineEvent::SessionExpired));

    let status = read_runtime_status();
    assert_eq!(status.state, RecordingRuntimeState::Expired);
    assert_eq!(status.end_reason.as_deref(), Some("max_duration"));

    let manifest = codex_record_replay_linux::manifest::read_manifest(&root).unwrap();
    assert_eq!(manifest.end_reason.as_deref(), Some("max_duration"));
    assert!(manifest.ended_at.is_some());

    let timeline = read_timeline(&root).unwrap();
    assert!(matches!(
        timeline.last(),
        Some(record) if matches!(&record.event, TimelineEvent::SessionExpired)
    ));

    match previous {
        Some(path) => std::env::set_var("CODEX_RECORD_REPLAY_STATUS_PATH", path),
        None => std::env::remove_var("CODEX_RECORD_REPLAY_STATUS_PATH"),
    }
}

fn status_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn status_env_guard() -> MutexGuard<'static, ()> {
    status_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn create_standard_bundle_dirs(root: &Path) {
    for dir in [
        "screenshots",
        "accessibility",
        "browser",
        "transcripts",
        "audio",
        "input-capture",
        "x11",
    ] {
        fs::create_dir(root.join(dir)).unwrap();
    }
}

fn read_jsonl_values(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
