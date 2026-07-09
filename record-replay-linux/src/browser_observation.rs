use codex_computer_use_linux::windowing;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BrowserObservation {
    pub browser: String,
    pub window_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wm_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_source: Option<String>,
}

pub(crate) fn observations_from_windows(
    windows: &[windowing::WindowInfo],
) -> Vec<BrowserObservation> {
    windows
        .iter()
        .filter(|window| !window.hidden)
        .filter_map(observation_from_window)
        .collect()
}

pub(crate) fn observation_from_window(
    window: &windowing::WindowInfo,
) -> Option<BrowserObservation> {
    let browser = browser_name_for_window(window)?;
    Some(BrowserObservation {
        browser,
        window_id: window.window_id,
        title: window.title.clone(),
        app_id: window.app_id.clone(),
        wm_class: window.wm_class.clone(),
        pid: window.pid,
        focused: window.focused,
        url: None,
        domain: None,
        url_source: None,
    })
}

fn browser_name_for_window(window: &windowing::WindowInfo) -> Option<String> {
    let haystack = [
        window.app_id.as_deref().unwrap_or_default(),
        window.wm_class.as_deref().unwrap_or_default(),
        window.title.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();

    if haystack.contains("google-chrome") || haystack.contains("chrome") {
        Some("Google Chrome".to_string())
    } else if haystack.contains("chromium") {
        Some("Chromium".to_string())
    } else if haystack.contains("brave") {
        Some("Brave".to_string())
    } else if haystack.contains("thorium") {
        Some("Thorium".to_string())
    } else if haystack.contains("firefox") {
        Some("Firefox".to_string())
    } else {
        None
    }
}
