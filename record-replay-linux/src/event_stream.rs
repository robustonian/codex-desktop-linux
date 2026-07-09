use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::timeline::{TimelineEvent, TimelineRecord};

pub fn append_event_stream_record(bundle_dir: &Path, record: &TimelineRecord) -> Result<()> {
    if let Some(event) = sky_event_for(bundle_dir, record)? {
        crate::secure_fs::append_private_line(
            &bundle_dir.join(crate::manifest::EVENT_STREAM_EVENTS_FILE_NAME),
            &serde_json::to_string(&event)?,
        )?;
    } else {
        crate::secure_fs::append_private_line(
            &bundle_dir.join(crate::manifest::EVENT_STREAM_SUPPRESSED_FILE_NAME),
            &record.to_json_line()?,
        )?;
    }
    if bundle_dir
        .join(crate::manifest::MANIFEST_FILE_NAME)
        .is_file()
    {
        crate::manifest::refresh_event_stream_session(bundle_dir)?;
    }
    Ok(())
}

fn sky_event_for(bundle_dir: &Path, record: &TimelineRecord) -> Result<Option<Value>> {
    let base = event_base(record);
    let event = match &record.event {
        TimelineEvent::SessionStarted { goal } => {
            let mut event = base_with_kind(base, "session.started");
            set_optional(&mut event, "goal", goal.as_deref());
            event
        }
        TimelineEvent::SpeechContext {
            transcript,
            file,
            source,
        } => json!({
            "kind": "keyboard.text_input",
            "index": record.index,
            "recordedAt": record.recorded_at,
            "timestamp": record.recorded_at,
            "text": transcript,
            "target": {
                "file": file,
                "source": source,
                "semanticKind": "speech_context"
            }
        }),
        TimelineEvent::BrowserTrace {
            file,
            url,
            title,
            source,
        } => json!({
            "kind": "window.changed",
            "index": record.index,
            "recordedAt": record.recorded_at,
            "timestamp": record.recorded_at,
            "title": title,
            "url": url,
            "target": {
                "file": file,
                "source": source,
                "semanticKind": "browser_trace"
            }
        }),
        TimelineEvent::DesktopSnapshot {
            file,
            browser_observation_count,
            focused_window_title,
            focused_window_app_id,
            focused_window_wm_class,
            focused_browser_name,
            focused_browser_title,
            focused_browser_url,
            focused_browser_domain,
            focused_browser_url_source,
            source,
            ..
        } => desktop_snapshot_event(
            bundle_dir,
            record,
            DesktopSnapshotEventContext {
                file,
                browser_observation_count: *browser_observation_count,
                focused_window_title: focused_window_title.as_deref(),
                focused_window_app_id: focused_window_app_id.as_deref(),
                focused_window_wm_class: focused_window_wm_class.as_deref(),
                focused_browser_name: focused_browser_name.as_deref(),
                focused_browser_title: focused_browser_title.as_deref(),
                focused_browser_url: focused_browser_url.as_deref(),
                focused_browser_domain: focused_browser_domain.as_deref(),
                focused_browser_url_source: focused_browser_url_source.as_deref(),
                source: source.as_deref(),
            },
        )?,
        TimelineEvent::SessionStopped => session_ended_event(bundle_dir, record, "stopped"),
        TimelineEvent::SessionCancelled { discarded } => {
            let mut event = session_ended_event(bundle_dir, record, "cancelled");
            event["discarded"] = json!(discarded);
            event
        }
        TimelineEvent::SessionExpired => session_ended_event(bundle_dir, record, "expired"),
        TimelineEvent::Diagnostic { level, message } => json!({
            "kind": "debug.error",
            "index": record.index,
            "recordedAt": record.recorded_at,
            "timestamp": record.recorded_at,
            "level": level,
            "message": message
        }),
        TimelineEvent::Navigation { url } => json!({
            "kind": "window.changed",
            "index": record.index,
            "recordedAt": record.recorded_at,
            "timestamp": record.recorded_at,
            "url": url,
            "target": {
                "semanticKind": "navigation"
            }
        }),
        TimelineEvent::UserMarker { note } => json!({
            "kind": "selection.changed",
            "index": record.index,
            "recordedAt": record.recorded_at,
            "timestamp": record.recorded_at,
            "selectedText": note,
            "target": {
                "semanticKind": "user_marker"
            }
        }),
        _ => return Ok(None),
    };
    Ok(Some(event))
}

fn event_base(record: &TimelineRecord) -> Value {
    json!({
        "index": record.index,
        "recordedAt": record.recorded_at,
        "timestamp": record.recorded_at,
    })
}

fn base_with_kind(mut base: Value, kind: &str) -> Value {
    base["kind"] = json!(kind);
    base
}

fn set_optional(event: &mut Value, field: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        event[field] = json!(value);
    }
}

fn session_ended_event(bundle_dir: &Path, record: &TimelineRecord, status: &str) -> Value {
    let manifest = crate::manifest::read_manifest(bundle_dir).ok();
    json!({
        "kind": "session.ended",
        "index": record.index,
        "recordedAt": record.recorded_at,
        "timestamp": record.recorded_at,
        "status": status,
        "endReason": manifest
            .as_ref()
            .and_then(|manifest| manifest.end_reason.clone()),
        "startedAt": manifest.as_ref().map(|manifest| manifest.started_at.clone()),
        "endedAt": manifest.as_ref().and_then(|manifest| manifest.ended_at.clone())
    })
}

struct DesktopSnapshotEventContext<'a> {
    file: &'a str,
    browser_observation_count: usize,
    focused_window_title: Option<&'a str>,
    focused_window_app_id: Option<&'a str>,
    focused_window_wm_class: Option<&'a str>,
    focused_browser_name: Option<&'a str>,
    focused_browser_title: Option<&'a str>,
    focused_browser_url: Option<&'a str>,
    focused_browser_domain: Option<&'a str>,
    focused_browser_url_source: Option<&'a str>,
    source: Option<&'a str>,
}

fn desktop_snapshot_event(
    bundle_dir: &Path,
    record: &TimelineRecord,
    context: DesktopSnapshotEventContext<'_>,
) -> Result<Value> {
    let artifact = read_json_artifact(bundle_dir, context.file)?;
    let focused = artifact
        .as_ref()
        .and_then(|artifact| artifact.get("focused_window"));
    let title = focused
        .and_then(|window| window.get("title"))
        .and_then(Value::as_str)
        .or(context.focused_window_title);
    let app_id = focused
        .and_then(|window| window.get("app_id"))
        .and_then(Value::as_str)
        .or(context.focused_window_app_id);
    let wm_class = focused
        .and_then(|window| window.get("wm_class"))
        .and_then(Value::as_str)
        .or(context.focused_window_wm_class);
    let window_id = focused
        .and_then(|window| window.get("window_id"))
        .and_then(Value::as_u64);
    let pid = focused
        .and_then(|window| window.get("pid"))
        .and_then(Value::as_u64);
    let focused_browser = artifact
        .as_ref()
        .and_then(|artifact| artifact.get("focused_browser_observation"));
    let browser_name = focused_browser
        .and_then(|browser| browser.get("browser"))
        .and_then(Value::as_str)
        .or(context.focused_browser_name);
    let browser_title = focused_browser
        .and_then(|browser| browser.get("title"))
        .and_then(Value::as_str)
        .or(context.focused_browser_title);
    let browser_url = focused_browser
        .and_then(|browser| browser.get("url"))
        .and_then(Value::as_str)
        .or(context.focused_browser_url);
    let browser_domain = focused_browser
        .and_then(|browser| browser.get("domain"))
        .and_then(Value::as_str)
        .or(context.focused_browser_domain);
    let browser_url_source = focused_browser
        .and_then(|browser| browser.get("url_source"))
        .and_then(Value::as_str)
        .or(context.focused_browser_url_source);
    let browser_count = artifact
        .as_ref()
        .and_then(|artifact| artifact.get("browser_observation_count"))
        .and_then(Value::as_u64)
        .unwrap_or(context.browser_observation_count as u64);
    let mut event = json!({
        "kind": "window.changed",
        "index": record.index,
        "recordedAt": record.recorded_at,
        "timestamp": record.recorded_at,
        "processIdentifier": pid,
        "bundleIdentifier": app_id,
        "wmClass": wm_class,
        "title": title.or(browser_title),
            "url": browser_url,
            "windowID": window_id,
            "target": {
            "file": context.file,
            "source": context.source,
            "semanticKind": "desktop_snapshot",
            "browserObservationCount": browser_count,
            "browser": browser_name,
            "browserTitle": browser_title,
            "browserUrl": browser_url,
            "browserDomain": browser_domain,
            "browserUrlSource": browser_url_source
        }
    });
    set_optional(&mut event, "browser", browser_name);
    Ok(event)
}

fn read_json_artifact(bundle_dir: &Path, file: &str) -> Result<Option<Value>> {
    let path = crate::manifest::checked_bundle_path(bundle_dir, "event_stream_artifact", file)?;
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw)
        .map(Some)
        .with_context(|| format!("failed to parse {}", path.display()))
}
