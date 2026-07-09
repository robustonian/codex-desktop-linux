use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{manifest::AUDIO_DIR_NAME, timeline::TimelineEvent};

const AUDIO_METADATA_FILE_NAME: &str = "recording.json";
const AUDIO_PID_FILE_NAME: &str = "recording.pid";
const AUDIO_OUTPUT_FILE_NAME: &str = "recording.wav";
const AUDIO_STDERR_FILE_NAME: &str = "recording.stderr.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCaptureReport {
    pub ok: bool,
    pub provider: Option<String>,
    pub status: String,
    pub metadata_file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_start_time_ticks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct AudioRecorderSpec {
    provider: &'static str,
    binary: PathBuf,
    output_file_name: &'static str,
    args: Vec<String>,
}

pub fn available_audio_recorders() -> Vec<String> {
    if !audio_capture_enabled() {
        return Vec::new();
    }
    known_recorders()
        .into_iter()
        .filter_map(|recorder| find_in_path(recorder.binary).map(|_| recorder.provider.to_string()))
        .collect()
}

pub fn start_audio_capture(bundle_dir: &Path) -> Result<AudioCaptureReport> {
    crate::secure_fs::create_private_dir_all(&bundle_dir.join(AUDIO_DIR_NAME))
        .with_context(|| format!("failed to create {AUDIO_DIR_NAME} directory"))?;

    if !audio_capture_enabled() {
        let report = AudioCaptureReport::new(
            false,
            None,
            "disabled",
            None,
            None,
            Some("audio capture disabled by CODEX_RECORD_REPLAY_AUDIO".to_string()),
        );
        write_audio_metadata(bundle_dir, &report)?;
        return Ok(report);
    }

    let Some(spec) = detect_audio_recorder() else {
        let report = AudioCaptureReport::new(
            false,
            None,
            "missing",
            None,
            None,
            Some(
                "no Linux audio recorder found; install pw-record, parecord, ffmpeg, or arecord"
                    .to_string(),
            ),
        );
        write_audio_metadata(bundle_dir, &report)?;
        return Ok(report);
    };

    let relative_audio = format!("{AUDIO_DIR_NAME}/{}", spec.output_file_name);
    let audio_path = bundle_dir.join(&relative_audio);
    let stderr_path = bundle_dir.join(format!("{AUDIO_DIR_NAME}/{AUDIO_STDERR_FILE_NAME}"));
    let stderr = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stderr_path)
        .with_context(|| format!("failed to open {}", stderr_path.display()))?;
    set_private_file_mode_best_effort(&stderr_path);

    let mut command = Command::new(&spec.binary);
    command
        .args(&spec.args)
        .current_dir(bundle_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));

    match crate::process_reaper::spawn_reaped(&mut command, "failed to spawn audio recorder") {
        Ok(pid) => {
            let process_start_time_ticks = crate::process_identity::process_start_time_ticks(pid);
            crate::secure_fs::write_private_file(
                &bundle_dir.join(format!("{AUDIO_DIR_NAME}/{AUDIO_PID_FILE_NAME}")),
                format!("{pid}\n"),
            )?;
            let report = AudioCaptureReport {
                ok: true,
                provider: Some(spec.provider.to_string()),
                status: "recording".to_string(),
                metadata_file: audio_metadata_relative_path(),
                file: Some(relative_audio),
                pid: Some(pid),
                process_start_time_ticks,
                command: Some(spec.binary.to_string_lossy().to_string()),
                args: spec.args,
                message: None,
                started_at: crate::recorder::now_timestamp(),
                ended_at: None,
                end_reason: None,
            };
            write_audio_metadata(bundle_dir, &report)?;
            set_private_file_mode_best_effort(&audio_path);
            Ok(report)
        }
        Err(error) => {
            let report = AudioCaptureReport::new(
                false,
                Some(spec.provider.to_string()),
                "error",
                Some(relative_audio),
                Some(spec.binary.to_string_lossy().to_string()),
                Some(error.to_string()),
            );
            write_audio_metadata(bundle_dir, &report)?;
            Ok(report)
        }
    }
}

pub fn stop_audio_capture(
    bundle_dir: &Path,
    end_reason: &str,
) -> Result<Option<AudioCaptureReport>> {
    let metadata_path = bundle_dir.join(audio_metadata_relative_path());
    if !metadata_path.exists() {
        return Ok(None);
    }

    let mut report = read_audio_metadata(bundle_dir)?;
    if let Some(pid) = report.pid {
        terminate_process(pid, report.process_start_time_ticks);
    }
    if let Some(file) = &report.file {
        set_private_file_mode_best_effort(&bundle_dir.join(file));
    }
    report.ok = report
        .file
        .as_ref()
        .is_some_and(|file| bundle_dir.join(file).exists());
    report.status = if end_reason.starts_with("recording_controls_cancelled")
        || end_reason.starts_with("recording_controls_canceled")
    {
        "canceled".to_string()
    } else if end_reason == "max_duration" {
        "expired".to_string()
    } else {
        "stopped".to_string()
    };
    report.ended_at = Some(crate::recorder::now_timestamp());
    report.end_reason = Some(end_reason.to_string());
    write_audio_metadata(bundle_dir, &report)?;
    Ok(Some(report))
}

pub fn audio_timeline_event(report: &AudioCaptureReport) -> TimelineEvent {
    TimelineEvent::AudioRecording {
        file: report.file.clone(),
        metadata_file: report.metadata_file.clone(),
        provider: report.provider.clone(),
        status: report.status.clone(),
    }
}

fn detect_audio_recorder() -> Option<AudioRecorderSpec> {
    known_recorders().into_iter().find_map(|recorder| {
        let binary = find_in_path(recorder.binary)?;
        let output_file_name = recorder.output_file_name;
        let relative_output = format!("{AUDIO_DIR_NAME}/{output_file_name}");
        let args = (recorder.args)(&relative_output);
        Some(AudioRecorderSpec {
            provider: recorder.provider,
            binary,
            output_file_name,
            args,
        })
    })
}

struct KnownRecorder {
    provider: &'static str,
    binary: &'static str,
    output_file_name: &'static str,
    args: fn(&str) -> Vec<String>,
}

fn known_recorders() -> Vec<KnownRecorder> {
    vec![
        KnownRecorder {
            provider: "pipewire-pw-record",
            binary: "pw-record",
            output_file_name: AUDIO_OUTPUT_FILE_NAME,
            args: |output| vec![output.to_string()],
        },
        KnownRecorder {
            provider: "pulseaudio-parecord",
            binary: "parecord",
            output_file_name: AUDIO_OUTPUT_FILE_NAME,
            args: |output| vec!["--file-format=wav".to_string(), output.to_string()],
        },
        KnownRecorder {
            provider: "ffmpeg-pulse",
            binary: "ffmpeg",
            output_file_name: AUDIO_OUTPUT_FILE_NAME,
            args: |output| {
                vec![
                    "-hide_banner".to_string(),
                    "-loglevel".to_string(),
                    "warning".to_string(),
                    "-nostdin".to_string(),
                    "-f".to_string(),
                    "pulse".to_string(),
                    "-i".to_string(),
                    "default".to_string(),
                    "-acodec".to_string(),
                    "pcm_s16le".to_string(),
                    output.to_string(),
                ]
            },
        },
        KnownRecorder {
            provider: "alsa-arecord",
            binary: "arecord",
            output_file_name: AUDIO_OUTPUT_FILE_NAME,
            args: |output| {
                vec![
                    "-q".to_string(),
                    "-f".to_string(),
                    "cd".to_string(),
                    "-t".to_string(),
                    "wav".to_string(),
                    output.to_string(),
                ]
            },
        },
    ]
}

fn audio_capture_enabled() -> bool {
    match env::var("CODEX_RECORD_REPLAY_AUDIO") {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes" | "enabled"
        ),
        Err(_) => false,
    }
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path_value = env::var_os("PATH")?;
    for path in env::split_paths(&path_value) {
        let candidate = path.join(binary);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn read_audio_metadata(bundle_dir: &Path) -> Result<AudioCaptureReport> {
    let path = bundle_dir.join(audio_metadata_relative_path());
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read audio metadata at {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_audio_metadata(bundle_dir: &Path, report: &AudioCaptureReport) -> Result<()> {
    crate::secure_fs::write_private_file(
        &bundle_dir.join(audio_metadata_relative_path()),
        format!("{}\n", serde_json::to_string_pretty(report)?),
    )
}

fn audio_metadata_relative_path() -> String {
    format!("{AUDIO_DIR_NAME}/{AUDIO_METADATA_FILE_NAME}")
}

impl AudioCaptureReport {
    fn new(
        ok: bool,
        provider: Option<String>,
        status: &str,
        file: Option<String>,
        command: Option<String>,
        message: Option<String>,
    ) -> Self {
        Self {
            ok,
            provider,
            status: status.to_string(),
            metadata_file: audio_metadata_relative_path(),
            file,
            pid: None,
            process_start_time_ticks: None,
            command,
            args: Vec::new(),
            message,
            started_at: crate::recorder::now_timestamp(),
            ended_at: None,
            end_reason: None,
        }
    }
}

fn terminate_process(pid: u32, expected_start_time_ticks: Option<u64>) {
    if !process_is_alive(pid, expected_start_time_ticks) {
        return;
    }
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !process_is_alive(pid, expected_start_time_ticks) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let _ = Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn process_is_alive(pid: u32, expected_start_time_ticks: Option<u64>) -> bool {
    crate::process_identity::process_matches_start_time(pid, expected_start_time_ticks)
}

fn set_private_file_mode_best_effort(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn spawned_audio_children_are_reaped_after_exit() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("exit 0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let pid = crate::process_reaper::spawn_reaped(&mut command, "test audio recorder")
            .expect("test process should spawn");

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if crate::process_identity::process_start_time_ticks(pid).is_none() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }

        panic!("child process {pid} was not reaped");
    }
}
