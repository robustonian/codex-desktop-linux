use crate::diagnostics::hydrate_session_bus_env;
use crate::terminal::{enrich_terminal_windows, TerminalWindowContext};
use anyhow::{bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, process::Command, time::Duration};
use tokio::time::sleep;
use zbus::{zvariant::OwnedValue, Proxy};

pub const GNOME_SHELL_INTROSPECT_BACKEND: &str = "gnome-shell-introspect";
pub const GNOME_SHELL_EXTENSION_BACKEND: &str = "gnome-shell-extension";
pub const HYPRLAND_BACKEND: &str = "hyprland";
pub const GNOME_SHELL_EXTENSION_SERVICE: &str = "com.openai.Codex.WindowControl";
pub const GNOME_SHELL_EXTENSION_OBJECT_PATH: &str = "/com/openai/Codex/WindowControl";
pub const WINDOW_PERMISSION_HINT: &str = "Computer Use could not access a GNOME window list backend. Targeted window input requires session-bus access plus either GNOME Shell Introspect permission or the Codex GNOME Shell extension backend. Run setup_window_targeting to install the extension backend.";
const FOCUS_VERIFY_ATTEMPTS: usize = 6;
const FOCUS_VERIFY_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct WindowInfo {
    pub window_id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub wm_class: Option<String>,
    pub pid: Option<u32>,
    pub bounds: Option<WindowBounds>,
    pub workspace: Option<i32>,
    pub focused: bool,
    pub hidden: bool,
    pub client_type: Option<String>,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalWindowContext>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct WindowBounds {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct WindowTarget {
    #[serde(default)]
    pub window_id: Option<u64>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub tty: Option<String>,
    #[serde(default)]
    pub terminal_pid: Option<u32>,
    #[serde(default)]
    pub terminal_command: Option<String>,
    #[serde(default)]
    pub terminal_cwd: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub wm_class: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WindowFocusResult {
    pub requested_window: WindowInfo,
    pub focused_window: Option<WindowInfo>,
    pub exact_window_focused: bool,
    pub app_focused: bool,
    pub backend: String,
    pub note: String,
}

impl WindowTarget {
    pub fn has_target(&self) -> bool {
        self.window_id.is_some()
            || self.pid.is_some()
            || self.has_terminal_target()
            || self
                .app_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            || self
                .wm_class
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            || self
                .title
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }

    pub fn requires_exact_focus(&self) -> bool {
        self.window_id.is_some()
            || self.pid.is_some()
            || self.has_terminal_target()
            || self
                .title
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }

    fn has_terminal_target(&self) -> bool {
        self.terminal_pid.is_some()
            || self
                .tty
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            || self
                .terminal_command
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            || self
                .terminal_cwd
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

pub async fn list_windows() -> Result<Vec<WindowInfo>> {
    match list_extension_windows().await {
        Ok(windows) => Ok(windows),
        Err(extension_error) => match list_gnome_shell_introspect_windows().await {
            Ok(windows) => Ok(windows),
            Err(introspect_error) => match list_hyprland_windows() {
                Ok(windows) => Ok(windows),
                Err(hyprland_error) => Err(anyhow::anyhow!(
                    "Codex GNOME Shell extension failed: {extension_error:#}; GNOME Shell Introspect failed: {introspect_error:#}; Hyprland failed: {hyprland_error:#}"
                )),
            },
        },
    }
}

async fn list_gnome_shell_introspect_windows() -> Result<Vec<WindowInfo>> {
    hydrate_session_bus_env();

    let connection = zbus::Connection::session()
        .await
        .context("failed to connect to session bus")?;
    let proxy = Proxy::new(
        &connection,
        "org.gnome.Shell",
        "/org/gnome/Shell/Introspect",
        "org.gnome.Shell.Introspect",
    )
    .await
    .context("failed to create GNOME Shell introspection proxy")?;
    let windows: HashMap<u64, HashMap<String, OwnedValue>> = proxy
        .call("GetWindows", &())
        .await
        .context("GNOME Shell GetWindows call failed")?;

    let mut windows = windows
        .into_iter()
        .map(|(window_id, properties)| window_from_properties(window_id, &properties))
        .collect::<Vec<_>>();
    windows.sort_by_key(|window| window.window_id);
    enrich_terminal_windows(&mut windows);
    Ok(windows)
}

pub async fn list_extension_windows() -> Result<Vec<WindowInfo>> {
    let json = call_extension_json("ListWindows").await?;
    let mut windows: Vec<WindowInfo> =
        serde_json::from_str(&json).context("Codex GNOME Shell extension returned invalid JSON")?;
    for window in &mut windows {
        window.backend = GNOME_SHELL_EXTENSION_BACKEND.to_string();
    }
    windows.sort_by_key(|window| window.window_id);
    enrich_terminal_windows(&mut windows);
    Ok(windows)
}

fn list_hyprland_windows() -> Result<Vec<WindowInfo>> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .context("failed to run hyprctl clients -j")?;
    if !output.status.success() {
        bail!(
            "hyprctl clients -j failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    parse_hyprland_clients(&String::from_utf8_lossy(&output.stdout))
}

fn parse_hyprland_clients(json: &str) -> Result<Vec<WindowInfo>> {
    let clients: Vec<HyprlandClient> =
        serde_json::from_str(json).context("failed to parse hyprctl clients -j output")?;

    let mut windows = clients
        .into_iter()
        .filter(|client| client.mapped.unwrap_or(true))
        .map(WindowInfo::try_from)
        .collect::<Result<Vec<_>>>()?;
    windows.sort_by_key(|window| window.window_id);
    enrich_terminal_windows(&mut windows);
    Ok(windows)
}

pub async fn focused_window() -> Result<Option<WindowInfo>> {
    current_focused_window().await
}

pub async fn focus_window_target(target: &WindowTarget) -> Result<WindowFocusResult> {
    if !target.has_target() {
        bail!("Pass window_id, pid, app_id, wm_class, title, tty, terminal_pid, terminal_command, or terminal_cwd to target a window.");
    }

    let windows = list_windows().await?;
    let requested_window = resolve_window_target(&windows, target)?.clone();
    ensure_backend_can_focus_target(target, &requested_window)?;

    if requested_window.backend == HYPRLAND_BACKEND {
        activate_hyprland_window(requested_window.window_id)?;
    } else if requested_window.backend == GNOME_SHELL_EXTENSION_BACKEND {
        activate_extension_window(requested_window.window_id).await?;
    } else {
        let app_id = requested_window
            .app_id
            .as_deref()
            .or(target.app_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .context("GNOME Shell can only focus by app_id; the matched window has no app_id")?
            .to_string();
        focus_app(&app_id).await?;
    }

    let focused_window = wait_for_focused_window(&requested_window).await;
    let exact_window_focused = focused_window
        .as_ref()
        .is_some_and(|window| window.window_id == requested_window.window_id);
    let app_focused = focused_window
        .as_ref()
        .is_some_and(|window| same_optional_string(&window.app_id, &requested_window.app_id));

    Ok(WindowFocusResult {
        backend: requested_window.backend.clone(),
        requested_window,
        focused_window,
        exact_window_focused,
        app_focused,
        note: "Computer Use activated the requested window through the available window backend, then verified focus through a fresh window query."
            .to_string(),
    })
}

fn ensure_backend_can_focus_target(target: &WindowTarget, window: &WindowInfo) -> Result<()> {
    if target.requires_exact_focus()
        && window.backend != GNOME_SHELL_EXTENSION_BACKEND
        && window.backend != HYPRLAND_BACKEND
    {
        bail!(
            "Exact window targeting requires the Codex GNOME Shell extension or Hyprland backend; {} can list the matched window but cannot activate a specific window safely.",
            window.backend
        );
    }
    Ok(())
}

async fn current_focused_window() -> Result<Option<WindowInfo>> {
    Ok(list_windows()
        .await?
        .into_iter()
        .find(|window| window.focused))
}

async fn wait_for_focused_window(requested_window: &WindowInfo) -> Option<WindowInfo> {
    let mut last_focused_window = None;
    for attempt in 0..FOCUS_VERIFY_ATTEMPTS {
        if let Ok(focused_window) = current_focused_window().await {
            if focused_window
                .as_ref()
                .is_some_and(|window| window.window_id == requested_window.window_id)
            {
                return focused_window;
            }
            if focused_window.is_some() {
                last_focused_window = focused_window;
            }
        }

        if attempt + 1 < FOCUS_VERIFY_ATTEMPTS {
            sleep(FOCUS_VERIFY_DELAY).await;
        }
    }
    last_focused_window
}

pub fn resolve_window_target<'a>(
    windows: &'a [WindowInfo],
    target: &WindowTarget,
) -> Result<&'a WindowInfo> {
    if let Some(window_id) = target.window_id {
        return windows
            .iter()
            .find(|window| window.window_id == window_id)
            .with_context(|| format!("No window matched window_id {window_id}."));
    }

    if target.has_terminal_target() {
        let matches = windows
            .iter()
            .filter(|window| window_matches_terminal_target(window, target))
            .filter(|window| target.pid.is_none_or(|pid| window.pid == Some(pid)))
            .filter(|window| optional_exact_match(&window.app_id, target.app_id.as_deref()))
            .filter(|window| optional_exact_match(&window.wm_class, target.wm_class.as_deref()))
            .filter(|window| optional_title_match(&window.title, target.title.as_deref()))
            .collect::<Vec<_>>();
        return unique_window_match(matches, "terminal target");
    }

    if let Some(pid) = target.pid {
        let matches = windows
            .iter()
            .filter(|window| window.pid == Some(pid))
            .collect::<Vec<_>>();
        return unique_window_match(matches, &format!("pid {pid}"));
    }

    if let Some(app_id) = normalized_target(target.app_id.as_deref()) {
        if let Some(window) = windows.iter().find(|window| {
            window
                .app_id
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&app_id))
        }) {
            return Ok(window);
        }
        bail!("No window matched app_id {app_id}.");
    }

    if let Some(wm_class) = normalized_target(target.wm_class.as_deref()) {
        if let Some(window) = windows.iter().find(|window| {
            window
                .wm_class
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&wm_class))
        }) {
            return Ok(window);
        }
        bail!("No window matched wm_class {wm_class}.");
    }

    if let Some(title) = normalized_target(target.title.as_deref()) {
        let title_lower = title.to_ascii_lowercase();
        if let Some(window) = windows.iter().find(|window| {
            window
                .title
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().contains(&title_lower))
        }) {
            return Ok(window);
        }
        bail!("No window title contained {title}.");
    }

    bail!("Pass window_id, pid, app_id, wm_class, title, tty, terminal_pid, terminal_command, or terminal_cwd to target a window.");
}

fn unique_window_match<'a>(
    matches: Vec<&'a WindowInfo>,
    description: &str,
) -> Result<&'a WindowInfo> {
    match matches.as_slice() {
        [window] => Ok(*window),
        [] => bail!("No window matched {description}."),
        windows => {
            let ids = windows
                .iter()
                .map(|window| window.window_id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "{description} matched multiple windows ({ids}); add window_id, tty, title, or terminal_command to disambiguate."
            );
        }
    }
}

fn window_matches_terminal_target(window: &WindowInfo, target: &WindowTarget) -> bool {
    let Some(terminal) = &window.terminal else {
        return false;
    };

    if let Some(tty) = normalized_target(target.tty.as_deref()) {
        if !tty_matches(&terminal.tty, &tty) {
            return false;
        }
    }

    if let Some(pid) = target.terminal_pid {
        let active_pid = terminal.active_process.as_ref().map(|process| process.pid);
        if active_pid != Some(pid) && terminal.root_process.pid != pid {
            return false;
        }
    }

    if let Some(command) = normalized_target(target.terminal_command.as_deref()) {
        let command = command.to_ascii_lowercase();
        let active_matches = terminal
            .active_process
            .as_ref()
            .is_some_and(|process| terminal_process_matches_command(process, &command));
        if !active_matches && !terminal_process_matches_command(&terminal.root_process, &command) {
            return false;
        }
    }

    if let Some(cwd) = normalized_target(target.terminal_cwd.as_deref()) {
        let active_matches = terminal
            .active_process
            .as_ref()
            .is_some_and(|process| terminal_process_matches_cwd(process, &cwd));
        if !active_matches && !terminal_process_matches_cwd(&terminal.root_process, &cwd) {
            return false;
        }
    }

    true
}

fn terminal_process_matches_command(
    process: &crate::terminal::TerminalProcess,
    command_lower: &str,
) -> bool {
    process
        .command_name
        .to_ascii_lowercase()
        .contains(command_lower)
        || process
            .command_line
            .to_ascii_lowercase()
            .contains(command_lower)
}

fn terminal_process_matches_cwd(process: &crate::terminal::TerminalProcess, cwd: &str) -> bool {
    let requested = cwd.trim_end_matches('/');
    process.cwd.as_deref().is_some_and(|value| {
        let actual = value.trim_end_matches('/');
        actual == requested
            || (!requested.starts_with('/')
                && actual
                    .strip_suffix(requested)
                    .is_some_and(|prefix| prefix.ends_with('/')))
    })
}

fn tty_matches(actual: &str, requested: &str) -> bool {
    actual == requested
        || actual
            .strip_prefix("/dev/")
            .is_some_and(|value| value == requested)
        || actual
            .strip_prefix("/dev/pts/")
            .is_some_and(|value| value == requested)
}

fn optional_exact_match(actual: &Option<String>, requested: Option<&str>) -> bool {
    normalized_target(requested).is_none_or(|requested| {
        actual
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case(&requested))
    })
}

fn optional_title_match(actual: &Option<String>, requested: Option<&str>) -> bool {
    normalized_target(requested).is_none_or(|requested| {
        let requested = requested.to_ascii_lowercase();
        actual
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().contains(&requested))
    })
}

pub fn window_permission_hint(error: &str) -> Option<String> {
    let lower = error.to_ascii_lowercase();
    if lower.contains("accessdenied")
        || lower.contains("access denied")
        || lower.contains("not allowed")
        || lower.contains("operation not permitted")
        || lower.contains("failed to connect to session bus")
    {
        Some(WINDOW_PERMISSION_HINT.to_string())
    } else {
        None
    }
}

async fn focus_app(app_id: &str) -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("failed to connect to session bus")?;
    let proxy = Proxy::new(
        &connection,
        "org.gnome.Shell",
        "/org/gnome/Shell",
        "org.gnome.Shell",
    )
    .await
    .context("failed to create GNOME Shell proxy")?;
    let _: () = proxy
        .call("FocusApp", &(app_id))
        .await
        .with_context(|| format!("GNOME Shell FocusApp failed for app_id {app_id}"))?;
    Ok(())
}

async fn call_extension_json(method: &str) -> Result<String> {
    hydrate_session_bus_env();

    let connection = zbus::Connection::session()
        .await
        .context("failed to connect to session bus")?;
    let proxy = Proxy::new(
        &connection,
        GNOME_SHELL_EXTENSION_SERVICE,
        GNOME_SHELL_EXTENSION_OBJECT_PATH,
        GNOME_SHELL_EXTENSION_SERVICE,
    )
    .await
    .context("failed to create Codex GNOME Shell extension proxy")?;
    let json: String = proxy
        .call(method, &())
        .await
        .with_context(|| format!("Codex GNOME Shell extension {method} call failed"))?;
    Ok(json)
}

async fn activate_extension_window(window_id: u64) -> Result<()> {
    hydrate_session_bus_env();

    let connection = zbus::Connection::session()
        .await
        .context("failed to connect to session bus")?;
    let proxy = Proxy::new(
        &connection,
        GNOME_SHELL_EXTENSION_SERVICE,
        GNOME_SHELL_EXTENSION_OBJECT_PATH,
        GNOME_SHELL_EXTENSION_SERVICE,
    )
    .await
    .context("failed to create Codex GNOME Shell extension proxy")?;
    let (ok, message): (bool, String) = proxy
        .call("ActivateWindow", &(window_id))
        .await
        .with_context(|| {
            format!("Codex GNOME Shell extension ActivateWindow failed for {window_id}")
        })?;
    if ok {
        Ok(())
    } else {
        bail!("Codex GNOME Shell extension refused activation: {message}");
    }
}

fn activate_hyprland_window(window_id: u64) -> Result<()> {
    let address = format!("address:0x{window_id:x}");
    let output = Command::new("hyprctl")
        .args(["dispatch", "focuswindow", &address])
        .output()
        .with_context(|| format!("failed to run hyprctl dispatch focuswindow {address}"))?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "hyprctl dispatch focuswindow {address} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

#[derive(Debug, Deserialize)]
struct HyprlandClient {
    address: String,
    mapped: Option<bool>,
    hidden: Option<bool>,
    at: Option<[i32; 2]>,
    size: Option<[u32; 2]>,
    workspace: Option<HyprlandWorkspace>,
    #[serde(rename = "class")]
    class_name: Option<String>,
    title: Option<String>,
    pid: Option<i64>,
    xwayland: Option<bool>,
    #[serde(rename = "focusHistoryID")]
    focus_history_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct HyprlandWorkspace {
    id: Option<i32>,
}

impl TryFrom<HyprlandClient> for WindowInfo {
    type Error = anyhow::Error;

    fn try_from(client: HyprlandClient) -> Result<Self> {
        let window_id = parse_hyprland_address(&client.address)?;
        let bounds = client.size.map(|[width, height]| WindowBounds {
            x: client.at.map(|[x, _]| x),
            y: client.at.map(|[_, y]| y),
            width,
            height,
        });
        let client_type = client.xwayland.map(|xwayland| {
            if xwayland {
                "x11".to_string()
            } else {
                "wayland".to_string()
            }
        });

        Ok(WindowInfo {
            window_id,
            title: client.title,
            app_id: client.class_name.clone(),
            wm_class: client.class_name,
            pid: client.pid.and_then(|pid| u32::try_from(pid).ok()),
            bounds,
            workspace: client.workspace.and_then(|workspace| workspace.id),
            focused: client.focus_history_id == Some(0),
            hidden: client.hidden.unwrap_or(false),
            client_type,
            backend: HYPRLAND_BACKEND.to_string(),
            terminal: None,
        })
    }
}

fn parse_hyprland_address(address: &str) -> Result<u64> {
    let hex = address
        .trim()
        .strip_prefix("0x")
        .context("Hyprland window address did not start with 0x")?;
    u64::from_str_radix(hex, 16)
        .with_context(|| format!("failed to parse Hyprland window address {address}"))
}

fn window_from_properties(window_id: u64, properties: &HashMap<String, OwnedValue>) -> WindowInfo {
    let width = get_u32(properties, "width");
    let height = get_u32(properties, "height");
    let bounds = width.zip(height).map(|(width, height)| WindowBounds {
        x: get_i32(properties, "x"),
        y: get_i32(properties, "y"),
        width,
        height,
    });

    WindowInfo {
        window_id,
        title: get_string(properties, "title"),
        app_id: get_string(properties, "app-id"),
        wm_class: get_string(properties, "wm-class"),
        pid: get_u32(properties, "pid"),
        bounds,
        workspace: get_i32(properties, "workspace"),
        focused: get_bool(properties, "has-focus").unwrap_or(false),
        hidden: get_bool(properties, "is-hidden").unwrap_or(false),
        client_type: get_u32(properties, "client-type").map(client_type_name),
        backend: GNOME_SHELL_INTROSPECT_BACKEND.to_string(),
        terminal: None,
    }
}

fn get_string(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(ToOwned::to_owned)
}

fn get_bool(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    properties
        .get(key)
        .and_then(|value| bool::try_from(value).ok())
}

fn get_u32(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
    properties
        .get(key)
        .and_then(|value| u32::try_from(value).ok())
}

fn get_i32(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<i32> {
    properties.get(key).and_then(|value| {
        i32::try_from(value).ok().or_else(|| {
            u32::try_from(value)
                .ok()
                .and_then(|value| value.try_into().ok())
        })
    })
}

fn client_type_name(value: u32) -> String {
    match value {
        0 => "wayland",
        1 => "x11",
        _ => "unknown",
    }
    .to_string()
}

fn normalized_target(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn same_optional_string(left: &Option<String>, right: &Option<String>) -> bool {
    match (left.as_deref(), right.as_deref()) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{TerminalProcess, TerminalWindowContext};
    use zbus::zvariant::Value;

    fn owned_value(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn window(window_id: u64, title: &str, app_id: &str, wm_class: &str) -> WindowInfo {
        WindowInfo {
            window_id,
            title: Some(title.to_string()),
            app_id: Some(app_id.to_string()),
            wm_class: Some(wm_class.to_string()),
            pid: Some(window_id as u32 + 1000),
            bounds: Some(WindowBounds {
                x: None,
                y: None,
                width: 800,
                height: 600,
            }),
            workspace: None,
            focused: false,
            hidden: false,
            client_type: Some("wayland".to_string()),
            backend: GNOME_SHELL_INTROSPECT_BACKEND.to_string(),
            terminal: None,
        }
    }

    fn terminal_window(
        window_id: u64,
        title: &str,
        tty: &str,
        active_pid: u32,
        active_command: &str,
        active_cwd: &str,
    ) -> WindowInfo {
        let mut window = window(
            window_id,
            title,
            "com.mitchellh.ghostty.desktop",
            "com.mitchellh.ghostty",
        );
        window.terminal = Some(TerminalWindowContext {
            tty: tty.to_string(),
            root_process: TerminalProcess {
                pid: active_pid - 1,
                command_name: "zsh".to_string(),
                command_line: "zsh --login".to_string(),
                cwd: Some("/home/avifenesh".to_string()),
            },
            active_process: Some(TerminalProcess {
                pid: active_pid,
                command_name: active_command.to_string(),
                command_line: format!("{active_command} resume 123"),
                cwd: Some(active_cwd.to_string()),
            }),
            process_count: 2,
            confidence: "heuristic".to_string(),
            match_reason: "test".to_string(),
        });
        window
    }

    #[test]
    fn target_reports_when_any_selector_is_present() {
        assert!(!WindowTarget::default().has_target());
        assert!(WindowTarget {
            title: Some("Ghostty".to_string()),
            ..Default::default()
        }
        .has_target());
        assert!(WindowTarget {
            tty: Some("/dev/pts/1".to_string()),
            ..Default::default()
        }
        .has_target());
    }

    #[test]
    fn title_pid_and_window_id_targets_require_exact_focus() {
        assert!(WindowTarget {
            title: Some("Ghostty".to_string()),
            ..Default::default()
        }
        .requires_exact_focus());
        assert!(WindowTarget {
            pid: Some(123),
            ..Default::default()
        }
        .requires_exact_focus());
        assert!(WindowTarget {
            window_id: Some(123),
            ..Default::default()
        }
        .requires_exact_focus());
        assert!(WindowTarget {
            terminal_command: Some("codex".to_string()),
            ..Default::default()
        }
        .requires_exact_focus());
        assert!(!WindowTarget {
            app_id: Some("com.mitchellh.ghostty.desktop".to_string()),
            ..Default::default()
        }
        .requires_exact_focus());
    }

    #[test]
    fn exact_targets_require_extension_activation_backend() {
        let window = window(
            2,
            "Ghostty",
            "com.mitchellh.ghostty.desktop",
            "com.mitchellh.ghostty",
        );

        let error = ensure_backend_can_focus_target(
            &WindowTarget {
                terminal_command: Some("codex".to_string()),
                ..Default::default()
            },
            &window,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Exact window targeting requires"));
    }

    #[test]
    fn app_targets_can_use_app_level_focus_backend() {
        let window = window(
            2,
            "Ghostty",
            "com.mitchellh.ghostty.desktop",
            "com.mitchellh.ghostty",
        );

        ensure_backend_can_focus_target(
            &WindowTarget {
                app_id: Some("com.mitchellh.ghostty.desktop".to_string()),
                ..Default::default()
            },
            &window,
        )
        .unwrap();
    }

    #[test]
    fn resolves_target_by_window_id_first() {
        let windows = vec![
            window(1, "Codex", "codex.desktop", "Codex"),
            window(2, "Ghostty", "com.mitchellh.ghostty.desktop", "Ghostty"),
        ];

        let matched = resolve_window_target(
            &windows,
            &WindowTarget {
                window_id: Some(2),
                title: Some("Codex".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(matched.window_id, 2);
    }

    #[test]
    fn pid_target_reports_ambiguous_matches() {
        let mut first = window(1, "Ghostty One", "com.mitchellh.ghostty.desktop", "Ghostty");
        let mut second = window(2, "Ghostty Two", "com.mitchellh.ghostty.desktop", "Ghostty");
        first.pid = Some(300);
        second.pid = Some(300);

        let error = resolve_window_target(
            &[first, second],
            &WindowTarget {
                pid: Some(300),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("pid 300 matched multiple windows"));
    }

    #[test]
    fn resolves_target_by_title_substring_case_insensitive() {
        let windows = vec![window(
            2,
            "avifenesh@host: ~/projects/codex",
            "com.mitchellh.ghostty.desktop",
            "Ghostty",
        )];

        let matched = resolve_window_target(
            &windows,
            &WindowTarget {
                title: Some("PROJECTS/CODEX".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(matched.window_id, 2);
    }

    #[test]
    fn resolves_terminal_target_by_tty() {
        let windows = vec![
            terminal_window(1, "Claude", "/dev/pts/0", 101, "claude", "/tmp"),
            terminal_window(2, "Codex", "/dev/pts/1", 201, "codex", "/home/avifenesh"),
        ];

        let matched = resolve_window_target(
            &windows,
            &WindowTarget {
                tty: Some("pts/1".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(matched.window_id, 2);
    }

    #[test]
    fn resolves_terminal_target_by_active_command() {
        let windows = vec![
            terminal_window(1, "Claude", "/dev/pts/0", 101, "claude", "/tmp"),
            terminal_window(2, "Codex", "/dev/pts/1", 201, "codex", "/home/avifenesh"),
        ];

        let matched = resolve_window_target(
            &windows,
            &WindowTarget {
                terminal_command: Some("codex resume".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(matched.window_id, 2);
    }

    #[test]
    fn resolves_terminal_target_by_cwd_suffix() {
        let windows = vec![
            terminal_window(1, "Home", "/dev/pts/0", 101, "zsh", "/home/avifenesh"),
            terminal_window(
                2,
                "Project",
                "/dev/pts/1",
                201,
                "codex",
                "/home/avifenesh/projects/codex-desktop-linux",
            ),
        ];

        let matched = resolve_window_target(
            &windows,
            &WindowTarget {
                terminal_cwd: Some("projects/codex-desktop-linux".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(matched.window_id, 2);
    }

    #[test]
    fn terminal_cwd_does_not_match_arbitrary_substrings() {
        let windows = vec![terminal_window(
            1,
            "Project",
            "/dev/pts/1",
            201,
            "codex",
            "/home/avifenesh/projects/codex-desktop-linux",
        )];

        let error = resolve_window_target(
            &windows,
            &WindowTarget {
                terminal_cwd: Some("fenesh/proj".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("No window matched terminal target"));
    }

    #[test]
    fn terminal_target_reports_ambiguous_matches() {
        let windows = vec![
            terminal_window(1, "One", "/dev/pts/0", 101, "zsh", "/home/avifenesh"),
            terminal_window(2, "Two", "/dev/pts/1", 201, "zsh", "/home/avifenesh"),
        ];

        let error = resolve_window_target(
            &windows,
            &WindowTarget {
                terminal_command: Some("zsh".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("matched multiple windows"));
    }

    #[test]
    fn maps_access_denied_errors_to_permission_hint() {
        let hint = window_permission_hint(
            "GDBus.Error:org.freedesktop.DBus.Error.AccessDenied: GetWindows is not allowed",
        );

        assert_eq!(hint.as_deref(), Some(WINDOW_PERMISSION_HINT));
    }

    #[test]
    fn parses_hyprland_clients_as_window_info() {
        let clients_json = r#"[
          {
            "address": "0x559952b6db60",
            "mapped": true,
            "hidden": false,
            "at": [10, 48],
            "size": [1900, 1022],
            "workspace": {"id": 2, "name": "2"},
            "class": "brave-browser",
            "title": "Repo - Brave",
            "pid": 24134,
            "xwayland": false,
            "focusHistoryID": 1
          },
          {
            "address": "0x559952be43d0",
            "mapped": true,
            "hidden": false,
            "at": [10, 48],
            "size": [1900, 1022],
            "workspace": {"id": 1, "name": "1"},
            "class": "codex-desktop",
            "title": "Codex",
            "pid": 68986,
            "xwayland": false,
            "focusHistoryID": 0
          },
          {
            "address": "0x559952c99aa0",
            "mapped": true,
            "hidden": false,
            "at": [0, 0],
            "size": [400, 300],
            "workspace": {"id": 3, "name": "3"},
            "class": "transient",
            "title": "Transient",
            "pid": -1,
            "xwayland": false,
            "focusHistoryID": 2
          }
        ]"#;

        let windows = parse_hyprland_clients(clients_json).unwrap();

        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].window_id, 0x559952b6db60);
        assert_eq!(windows[0].app_id.as_deref(), Some("brave-browser"));
        assert_eq!(windows[0].wm_class.as_deref(), Some("brave-browser"));
        assert_eq!(windows[0].title.as_deref(), Some("Repo - Brave"));
        assert_eq!(windows[0].pid, Some(24134));
        assert_eq!(windows[0].bounds.as_ref().unwrap().x, Some(10));
        assert_eq!(windows[0].bounds.as_ref().unwrap().height, 1022);
        assert_eq!(windows[0].workspace, Some(2));
        assert!(!windows[0].focused);
        assert_eq!(windows[0].client_type.as_deref(), Some("wayland"));
        assert_eq!(windows[0].backend, HYPRLAND_BACKEND);
        assert!(windows[1].focused);
        assert_eq!(windows[2].pid, None);
    }

    #[test]
    fn hyprland_backend_can_exact_focus_targets() {
        let mut window = window(2, "Codex", "codex-desktop", "codex-desktop");
        window.backend = HYPRLAND_BACKEND.to_string();

        ensure_backend_can_focus_target(
            &WindowTarget {
                title: Some("Codex".to_string()),
                ..Default::default()
            },
            &window,
        )
        .unwrap();
    }

    #[test]
    fn extracts_known_window_properties() {
        let properties = HashMap::from([
            ("title".to_string(), owned_value(Value::from("Ghostty"))),
            (
                "app-id".to_string(),
                owned_value(Value::from("com.mitchellh.ghostty.desktop")),
            ),
            ("wm-class".to_string(), owned_value(Value::from("Ghostty"))),
            ("client-type".to_string(), owned_value(Value::from(0_u32))),
            ("is-hidden".to_string(), owned_value(Value::from(false))),
            ("has-focus".to_string(), owned_value(Value::from(true))),
            ("width".to_string(), owned_value(Value::from(1200_u32))),
            ("height".to_string(), owned_value(Value::from(800_u32))),
        ]);

        let info = window_from_properties(42, &properties);

        assert_eq!(info.window_id, 42);
        assert_eq!(info.title.as_deref(), Some("Ghostty"));
        assert!(info.focused);
        assert_eq!(info.client_type.as_deref(), Some("wayland"));
        assert_eq!(info.bounds.unwrap().width, 1200);
    }
}
