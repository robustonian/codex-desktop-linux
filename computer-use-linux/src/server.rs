use crate::atspi_tree::{
    list_accessible_apps, perform_action as invoke_accessibility_action, set_element_value,
    snapshot_tree, AccessibilityAction, AccessibilityNode, AccessibleAppSummary, Bounds,
    ValueSetInvocation,
};
use crate::diagnostics::{doctor_report, setup_accessibility_report, DoctorReport, SetupReport};
use crate::gnome_extension::{setup_window_targeting_report, WindowTargetingSetupReport};
use crate::remote_desktop::{
    click as portal_click, drag as portal_drag, scroll as portal_scroll,
    start_portal_pointer_session, PointerButton, PortalPointerSession, ScrollDirection,
};
use crate::screenshot::{capture_screenshot, ScreenshotCapture};
use crate::windows::{
    focus_window_target, focused_window, list_windows, resolve_window_target,
    window_permission_hint, WindowFocusResult, WindowInfo, WindowTarget,
    GNOME_SHELL_EXTENSION_BACKEND, GNOME_SHELL_INTROSPECT_BACKEND,
};
use anyhow::Result;
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::PathBuf,
    process::{Command, Output},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Clone, Default)]
pub struct ComputerUseLinux {
    last_nodes: Arc<Mutex<Vec<AccessibilityNode>>>,
    portal_pointer_session: Arc<Mutex<Option<PortalPointerSession>>>,
}

#[tool_router]
impl ComputerUseLinux {
    #[tool(
        name = "doctor",
        description = "Report Linux Computer Use desktop integration readiness."
    )]
    fn doctor(&self) -> Json<DoctorReport> {
        Json(doctor_report())
    }

    #[tool(
        name = "setup_accessibility",
        description = "Enable GNOME accessibility through gsettings so Linux Computer Use can read AT-SPI trees."
    )]
    fn setup_accessibility(&self) -> Json<SetupReport> {
        Json(setup_accessibility_report())
    }

    #[tool(
        name = "setup_window_targeting",
        description = "Install and enable the optional GNOME Shell extension used for exact window list/focus targeting when GNOME blocks native introspection."
    )]
    async fn setup_window_targeting(&self) -> Json<WindowTargetingSetupReport> {
        Json(setup_window_targeting_report().await)
    }

    #[tool(
        name = "list_apps",
        description = "List running Linux desktop app candidates visible to the Computer Use backend."
    )]
    async fn list_apps(&self) -> Json<ListAppsOutput> {
        let (accessible_apps, accessibility_error) = match list_accessible_apps(50).await {
            Ok(apps) => (apps, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };

        Json(ListAppsOutput {
            apps: list_process_apps(),
            accessible_apps,
            accessibility_error,
            note: "Linux Computer Use lists process candidates plus AT-SPI application roots when accessibility is enabled.".to_string(),
        })
    }

    #[tool(
        name = "list_windows",
        description = "List compositor windows with title, app id, class, focus state, client type, and known bounds."
    )]
    async fn list_windows(&self) -> Json<ListWindowsOutput> {
        Json(window_list_output().await)
    }

    #[tool(
        name = "focused_window",
        description = "Return the compositor window that currently has keyboard focus."
    )]
    async fn focused_window(&self) -> Json<FocusedWindowOutput> {
        match focused_window().await {
            Ok(window) => {
                let backend = window_backend(window.as_ref().into_iter());
                Json(FocusedWindowOutput {
                    backend,
                    focused_window: window,
                    error: None,
                    permissions_hint: None,
                    message:
                        "Focused window query completed through the available GNOME window backend."
                            .to_string(),
                })
            }
            Err(error) => {
                let error = format!("{error:#}");
                Json(FocusedWindowOutput {
                    backend: GNOME_SHELL_INTROSPECT_BACKEND.to_string(),
                    focused_window: None,
                    permissions_hint: window_permission_hint(&error),
                    error: Some(error),
                    message: "Focused window query failed; targeted keyboard input is unavailable until window introspection works.".to_string(),
                })
            }
        }
    }

    #[tool(
        name = "activate_window",
        description = "Focus a Linux desktop window by window_id, pid, app_id, wm_class, title, or terminal selectors when the compositor permits it."
    )]
    async fn activate_window(
        &self,
        Parameters(params): Parameters<ActivateWindowParams>,
    ) -> Json<ActivateWindowOutput> {
        let target = params.into_target();
        let received = Some(serde_json::json!(target.clone()));
        match focus_window_target(&target).await {
            Ok(focus) => {
                let ok = focus_satisfies_target(&focus, &target);
                Json(ActivateWindowOutput {
                    ok,
                    implemented: true,
                    backend: focus.backend.clone(),
                    focus: Some(focus),
                    error: None,
                    permissions_hint: None,
                    received,
                })
            }
            Err(error) => {
                let error = format!("{error:#}");
                Json(ActivateWindowOutput {
                    ok: false,
                    implemented: true,
                    backend: GNOME_SHELL_INTROSPECT_BACKEND.to_string(),
                    focus: None,
                    permissions_hint: window_permission_hint(&error),
                    error: Some(error),
                    received,
                })
            }
        }
    }

    #[tool(
        name = "get_app_state",
        description = "Start an app use session if needed, then get screenshot and accessibility state for a Linux app."
    )]
    async fn get_app_state(
        &self,
        Parameters(params): Parameters<GetAppStateParams>,
    ) -> Json<GetAppStateOutput> {
        let diagnostics = doctor_report();
        let (window_context, window_error, window_permissions_hint) =
            self.resolve_window_context(&params).await;
        let max_nodes = params.max_nodes.unwrap_or(120).clamp(1, 500);
        let max_depth = params.max_depth.unwrap_or(12).min(12);
        let include_screenshot = params.include_screenshot.unwrap_or(true);
        let app_filter = self
            .resolve_accessibility_app_filter(&params, window_context.as_ref())
            .await;
        let (screenshot, screenshot_error) = if include_screenshot {
            match capture_screenshot().await {
                Ok(capture) => (Some(capture), None),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };
        let (accessibility_tree, accessibility_tree_raw_count, accessibility_error) =
            if diagnostics.readiness.can_build_accessibility_tree {
                let target_pid = window_context.as_ref().and_then(|window| window.pid);
                match snapshot_tree(app_filter.as_deref(), target_pid, max_nodes, max_depth).await {
                    Ok(nodes) => {
                        let raw_count = nodes.len();
                        (compact_accessibility_tree(nodes), raw_count, None)
                    }
                    Err(error) => (Vec::new(), 0, Some(error.to_string())),
                }
            } else {
                (
                    Vec::new(),
                    0,
                    Some(
                        "GNOME accessibility is disabled; call setup_accessibility first."
                            .to_string(),
                    ),
                )
            };
        if accessibility_error.is_none() {
            self.cache_nodes(&accessibility_tree);
        }
        let mut message = if let Some(error) = &accessibility_error {
            format!("MCP registration is working, but AT-SPI tree extraction failed: {error}")
        } else if let Some(capture) = &screenshot {
            format!(
                "MCP registration, screenshot capture, and AT-SPI tree extraction are working. Captured {} accessibility nodes (compacted from {}) and a screenshot through {}.",
                accessibility_tree.len(),
                accessibility_tree_raw_count,
                capture.source
            )
        } else if let Some(error) = &screenshot_error {
            format!(
                "MCP registration and AT-SPI tree extraction are working. Captured {} accessibility nodes (compacted from {}). Screenshot capture failed: {error}",
                accessibility_tree.len(),
                accessibility_tree_raw_count,
            )
        } else {
            format!(
                "MCP registration and AT-SPI tree extraction are working. Captured {} accessibility nodes (compacted from {}). Screenshot capture was not requested.",
                accessibility_tree.len(),
                accessibility_tree_raw_count,
            )
        };
        if let Some(window) = &window_context {
            message.push_str(&format!(
                " Window target resolved to window_id {}.",
                window.window_id
            ));
        } else if let Some(error) = &window_error {
            message.push_str(&format!(" Window target resolution failed: {error}"));
        }

        Json(GetAppStateOutput {
            app_name_or_bundle_identifier: params.app_name_or_bundle_identifier,
            window_context,
            window_error,
            window_permissions_hint,
            backend: "linux-atspi".to_string(),
            screenshot,
            screenshot_error,
            accessibility_tree,
            accessibility_tree_raw_count,
            accessibility_error,
            diagnostics,
            message,
        })
    }

    #[tool(
        name = "click",
        description = "Click an element by index, semantic selector, or pixel coordinates from screenshot."
    )]
    async fn click(&self, Parameters(params): Parameters<ClickParams>) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params));
        let target = match self.resolve_click_target(&params) {
            Ok(target) => target,
            Err(message) => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "click".to_string(),
                    message,
                    received,
                });
            }
        };
        if let ClickTarget::PrimaryAction {
            object_ref,
            action_name,
        } = target
        {
            return match invoke_accessibility_action(&object_ref, Some("0")).await {
                Ok(invocation) => Json(ActionOutput {
                    ok: invocation.ok,
                    implemented: true,
                    action: "click".to_string(),
                    message: if invocation.ok {
                        format!(
                            "No clickable bounds were cached, so I invoked the primary AT-SPI action{}.",
                            action_name
                                .as_deref()
                                .filter(|name| !name.is_empty())
                                .map(|name| format!(" ({name})"))
                                .unwrap_or_default()
                        )
                    } else {
                        format!(
                            "The primary AT-SPI action{} returned false.",
                            action_name
                                .as_deref()
                                .filter(|name| !name.is_empty())
                                .map(|name| format!(" ({name})"))
                                .unwrap_or_default()
                        )
                    },
                    received,
                }),
                Err(error) => Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "click".to_string(),
                    message: error.to_string(),
                    received,
                }),
            };
        }
        let ClickTarget::Coordinates(x, y) = target else {
            unreachable!("click target must resolve to coordinates or an AT-SPI action");
        };
        let button = mouse_button_code(params.button.as_deref());
        let click_count = params.click_count.unwrap_or(1).clamp(1, 10).to_string();
        if let Some(session) = self.cached_portal_pointer_session() {
            match portal_click(
                &session,
                x,
                y,
                PointerButton::from_name(params.button.as_deref()),
                params.click_count.unwrap_or(1).clamp(1, 10),
            )
            .await
            {
                Ok(()) => {
                    return Json(ActionOutput {
                        ok: true,
                        implemented: true,
                        action: "click".to_string(),
                        message: "Action sent through the remote desktop portal.".to_string(),
                        received,
                    });
                }
                Err(_) => self.clear_portal_pointer_session(),
            }
        } else if self.should_prefer_portal_pointer_backend() {
            match self.ensure_portal_pointer_session().await {
                Ok(Some(session)) => match portal_click(
                    &session,
                    x,
                    y,
                    PointerButton::from_name(params.button.as_deref()),
                    params.click_count.unwrap_or(1).clamp(1, 10),
                )
                .await
                {
                    Ok(()) => {
                        return Json(ActionOutput {
                            ok: true,
                            implemented: true,
                            action: "click".to_string(),
                            message: "Action sent through the remote desktop portal.".to_string(),
                            received,
                        });
                    }
                    Err(_) => self.clear_portal_pointer_session(),
                },
                Ok(None) => {}
                Err(_) => {}
            }
        }
        let result = run_ydotool_sequence(&[
            absolute_mousemove_args(x, y),
            vec![
                "click".to_string(),
                "--repeat".to_string(),
                click_count,
                button,
            ],
        ]);
        Json(action_result("click", result, received))
    }

    #[tool(
        name = "perform_action",
        description = "Invoke an accessibility action exposed by an element selected by index, identifier, or semantic selector. Defaults to the primary action unless action is provided."
    )]
    async fn perform_action(
        &self,
        Parameters(params): Parameters<ActionParams>,
    ) -> Json<ActionOutput> {
        self.perform_element_action(&params, params.action.as_deref().or(Some("0")))
            .await
    }

    #[tool(
        name = "set_value",
        description = "Set the value of a settable accessibility element selected by index, identifier, or semantic selector."
    )]
    async fn set_value(
        &self,
        Parameters(params): Parameters<SetValueParams>,
    ) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params.clone()));
        let object_ref = match self.resolve_object_ref(
            params.element_index,
            params.element_identifier.as_deref(),
            &params.selector(),
            ElementResolvePurpose::SetValue,
        ) {
            Ok(object_ref) => object_ref,
            Err(message) => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "set_value".to_string(),
                    message,
                    received,
                });
            }
        };

        match set_element_value(&object_ref, &params.value).await {
            Ok(ValueSetInvocation::Numeric { value }) => Json(ActionOutput {
                ok: true,
                implemented: true,
                action: "set_value".to_string(),
                message: format!("AT-SPI numeric value set to {value}."),
                received,
            }),
            Ok(ValueSetInvocation::EditableText) => Json(ActionOutput {
                ok: true,
                implemented: true,
                action: "set_value".to_string(),
                message: "AT-SPI editable text contents set.".to_string(),
                received,
            }),
            Err(error) => Json(ActionOutput {
                ok: false,
                implemented: true,
                action: "set_value".to_string(),
                message: error.to_string(),
                received,
            }),
        }
    }

    #[tool(
        name = "scroll",
        description = "Scroll an element in a direction by a number of pages."
    )]
    async fn scroll(&self, Parameters(params): Parameters<ScrollParams>) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params));
        let units = ((params.pages.unwrap_or(1.0).abs().max(0.1) * 5.0).round() as i32).max(1);
        let target_point =
            match self.resolve_optional_target_point(params.x, params.y, params.element_index) {
                Ok(point) => point,
                Err(message) => {
                    return Json(ActionOutput {
                        ok: false,
                        implemented: true,
                        action: "scroll".to_string(),
                        message,
                        received,
                    });
                }
            };
        let direction = match params.direction.to_ascii_lowercase().as_str() {
            "up" => ScrollDirection::Up,
            "down" => ScrollDirection::Down,
            "left" => ScrollDirection::Left,
            "right" => ScrollDirection::Right,
            _ => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "scroll".to_string(),
                    message: "Unsupported scroll direction; expected up, down, left, or right."
                        .to_string(),
                    received,
                });
            }
        };
        if let Some(session) = self.cached_portal_pointer_session() {
            match portal_scroll(&session, target_point, direction, units).await {
                Ok(()) => {
                    return Json(ActionOutput {
                        ok: true,
                        implemented: true,
                        action: "scroll".to_string(),
                        message: "Action sent through the remote desktop portal.".to_string(),
                        received,
                    });
                }
                Err(_) => self.clear_portal_pointer_session(),
            }
        } else if self.should_prefer_portal_pointer_backend() {
            match self.ensure_portal_pointer_session().await {
                Ok(Some(session)) => match portal_scroll(&session, target_point, direction, units)
                    .await
                {
                    Ok(()) => {
                        return Json(ActionOutput {
                            ok: true,
                            implemented: true,
                            action: "scroll".to_string(),
                            message: "Action sent through the remote desktop portal.".to_string(),
                            received,
                        });
                    }
                    Err(_) => self.clear_portal_pointer_session(),
                },
                Ok(None) => {}
                Err(_) => {}
            }
        }
        let (dx, dy) = match params.direction.to_ascii_lowercase().as_str() {
            "up" => (0, units),
            "down" => (0, -units),
            "left" => (units, 0),
            "right" => (-units, 0),
            _ => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "scroll".to_string(),
                    message: "Unsupported scroll direction; expected up, down, left, or right."
                        .to_string(),
                    received,
                });
            }
        };
        let mut sequence = Vec::new();
        if let Some((x, y)) = target_point {
            sequence.push(absolute_mousemove_args(x, y));
        }
        sequence.push(wheel_mousemove_args(dx, dy));
        let result = run_ydotool_sequence(&sequence);
        Json(action_result("scroll", result, received))
    }

    #[tool(
        name = "drag",
        description = "Drag from one point to another using pixel coordinates."
    )]
    async fn drag(&self, Parameters(params): Parameters<DragParams>) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params));
        if let Some(session) = self.cached_portal_pointer_session() {
            match portal_drag(
                &session,
                params.start_x,
                params.start_y,
                params.end_x,
                params.end_y,
            )
            .await
            {
                Ok(()) => {
                    return Json(ActionOutput {
                        ok: true,
                        implemented: true,
                        action: "drag".to_string(),
                        message: "Action sent through the remote desktop portal.".to_string(),
                        received,
                    });
                }
                Err(_) => self.clear_portal_pointer_session(),
            }
        } else if self.should_prefer_portal_pointer_backend() {
            match self.ensure_portal_pointer_session().await {
                Ok(Some(session)) => match portal_drag(
                    &session,
                    params.start_x,
                    params.start_y,
                    params.end_x,
                    params.end_y,
                )
                .await
                {
                    Ok(()) => {
                        return Json(ActionOutput {
                            ok: true,
                            implemented: true,
                            action: "drag".to_string(),
                            message: "Action sent through the remote desktop portal.".to_string(),
                            received,
                        });
                    }
                    Err(_) => self.clear_portal_pointer_session(),
                },
                Ok(None) => {}
                Err(_) => {}
            }
        }
        let result = run_ydotool_sequence(&[
            absolute_mousemove_args(params.start_x, params.start_y),
            vec!["click".to_string(), "0x40".to_string()],
            absolute_mousemove_args(params.end_x, params.end_y),
            vec!["click".to_string(), "0x80".to_string()],
        ]);
        Json(action_result("drag", result, received))
    }

    #[tool(
        name = "press_key",
        description = "Press a key or key-combination on the keyboard, optionally after focusing a target window or terminal selector."
    )]
    async fn press_key(
        &self,
        Parameters(params): Parameters<PressKeyParams>,
    ) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params.clone()));
        let focus = match self.focus_target_for_input(&params.window_target()).await {
            Ok(focus) => focus,
            Err(message) => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "press_key".to_string(),
                    message,
                    received,
                });
            }
        };
        let Some(key_events) = key_sequence(&params.key) else {
            return Json(ActionOutput {
                ok: false,
                implemented: true,
                action: "press_key".to_string(),
                message: "Unsupported key. Use names like Enter, Escape, Tab, ArrowLeft, Super, Ctrl+L, or a single US keyboard letter/digit.".to_string(),
                received,
            });
        };
        let mut args = vec!["key".to_string()];
        args.extend(key_events);
        let result = run_ydotool(&args).map(|output| vec![output]);
        Json(action_result_with_focus(
            "press_key",
            result,
            received,
            focus,
        ))
    }

    #[tool(
        name = "type_text",
        description = "Type literal text using keyboard input, optionally after focusing a target window or terminal selector."
    )]
    async fn type_text(
        &self,
        Parameters(params): Parameters<TypeTextParams>,
    ) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params.clone()));
        let focus = match self.focus_target_for_input(&params.window_target()).await {
            Ok(focus) => focus,
            Err(message) => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "type_text".to_string(),
                    message,
                    received,
                });
            }
        };
        let result = run_ydotool(&["type".to_string(), "--".to_string(), params.text])
            .map(|output| vec![output]);
        Json(action_result_with_focus(
            "type_text",
            result,
            received,
            focus,
        ))
    }
}

#[tool_handler(
    name = "codex-computer-use-linux",
    version = "0.1.0",
    instructions = "Begin every turn that uses Computer Use by calling get_app_state. If diagnostics report disabled GNOME accessibility, call setup_accessibility before asking the user to retry. Use list_windows/focused_window before targeted keyboard input. If diagnostics report windowing.can_list_windows=false, call setup_window_targeting to install the optional GNOME Shell extension backend, then ask the user to log out and back in if the setup report says a shell reload is required. This Linux backend can capture screenshots through GNOME Shell or XDG Desktop Portal, read AT-SPI trees with action/value metadata, invoke native AT-SPI actions, set AT-SPI values or editable text, list/focus GNOME Shell windows when org.gnome.Shell.Introspect or the Codex GNOME Shell extension permits it, attach best-effort terminal tty/process metadata to terminal windows, and send coordinate or element-targeted click/scroll/drag input through the Wayland remote desktop portal when available or through ydotool otherwise. For element-targeted actions, prefer element_index from the latest get_app_state result; click, perform_action, and set_value can also use semantic role/name/text/states selectors when the target is unique. type_text and press_key accept optional window_id, pid, app_id, wm_class, title, tty, terminal_pid, terminal_command, or terminal_cwd selectors and refuse targeted input if focus cannot be verified."
)]
impl ServerHandler for ComputerUseLinux {}

pub async fn serve_mcp() -> Result<()> {
    ComputerUseLinux::default()
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ListAppsOutput {
    apps: Vec<AppCandidate>,
    accessible_apps: Vec<AccessibleAppSummary>,
    accessibility_error: Option<String>,
    note: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ListWindowsOutput {
    backend: String,
    windows: Vec<WindowInfo>,
    error: Option<String>,
    permissions_hint: Option<String>,
    note: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct FocusedWindowOutput {
    backend: String,
    focused_window: Option<WindowInfo>,
    error: Option<String>,
    permissions_hint: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ActivateWindowParams {
    #[serde(default)]
    window_id: Option<u64>,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    tty: Option<String>,
    #[serde(default)]
    terminal_pid: Option<u32>,
    #[serde(default)]
    terminal_command: Option<String>,
    #[serde(default)]
    terminal_cwd: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    wm_class: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

impl ActivateWindowParams {
    fn into_target(self) -> WindowTarget {
        WindowTarget {
            window_id: self.window_id,
            pid: self.pid,
            tty: self.tty,
            terminal_pid: self.terminal_pid,
            terminal_command: self.terminal_command,
            terminal_cwd: self.terminal_cwd,
            app_id: self.app_id,
            wm_class: self.wm_class,
            title: self.title,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ActivateWindowOutput {
    ok: bool,
    implemented: bool,
    backend: String,
    focus: Option<WindowFocusResult>,
    error: Option<String>,
    permissions_hint: Option<String>,
    received: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct AppCandidate {
    name: String,
    pid: u32,
    command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct GetAppStateParams {
    #[serde(default)]
    app_name_or_bundle_identifier: Option<String>,
    #[serde(default)]
    window_id: Option<u64>,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    tty: Option<String>,
    #[serde(default)]
    terminal_pid: Option<u32>,
    #[serde(default)]
    terminal_command: Option<String>,
    #[serde(default)]
    terminal_cwd: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    wm_class: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    max_nodes: Option<usize>,
    #[serde(default)]
    max_depth: Option<u32>,
    #[serde(default)]
    include_screenshot: Option<bool>,
}

impl GetAppStateParams {
    fn window_target(&self) -> WindowTarget {
        WindowTarget {
            window_id: self.window_id,
            pid: self.pid,
            tty: self.tty.clone(),
            terminal_pid: self.terminal_pid,
            terminal_command: self.terminal_command.clone(),
            terminal_cwd: self.terminal_cwd.clone(),
            app_id: self.app_id.clone(),
            wm_class: self.wm_class.clone(),
            title: self.title.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct GetAppStateOutput {
    app_name_or_bundle_identifier: Option<String>,
    window_context: Option<WindowInfo>,
    window_error: Option<String>,
    window_permissions_hint: Option<String>,
    backend: String,
    screenshot: Option<ScreenshotCapture>,
    screenshot_error: Option<String>,
    accessibility_tree: Vec<AccessibilityNode>,
    accessibility_tree_raw_count: usize,
    accessibility_error: Option<String>,
    diagnostics: DoctorReport,
    message: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
struct ClickParams {
    #[serde(default)]
    element_index: Option<u32>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    states: Vec<String>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    click_count: Option<u32>,
}

impl ClickParams {
    fn selector(&self) -> ElementSelector<'_> {
        ElementSelector {
            role: self.role.as_deref(),
            name: self.name.as_deref(),
            text: self.text.as_deref(),
            states: &self.states,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
struct ActionParams {
    #[serde(default)]
    element_index: Option<u32>,
    #[serde(default)]
    element_identifier: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    states: Vec<String>,
    #[serde(default)]
    action: Option<String>,
}

impl ActionParams {
    fn selector(&self) -> ElementSelector<'_> {
        ElementSelector {
            role: self.role.as_deref(),
            name: self.name.as_deref(),
            text: self.text.as_deref(),
            states: &self.states,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
struct SetValueParams {
    #[serde(default)]
    element_index: Option<u32>,
    #[serde(default)]
    element_identifier: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    states: Vec<String>,
    value: String,
}

impl SetValueParams {
    fn selector(&self) -> ElementSelector<'_> {
        ElementSelector {
            role: self.role.as_deref(),
            name: self.name.as_deref(),
            text: self.text.as_deref(),
            states: &self.states,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ScrollParams {
    #[serde(default)]
    element_index: Option<u32>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    direction: String,
    #[serde(default)]
    pages: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct DragParams {
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct PressKeyParams {
    key: String,
    #[serde(default)]
    window_id: Option<u64>,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    tty: Option<String>,
    #[serde(default)]
    terminal_pid: Option<u32>,
    #[serde(default)]
    terminal_command: Option<String>,
    #[serde(default)]
    terminal_cwd: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    wm_class: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct TypeTextParams {
    text: String,
    #[serde(default)]
    window_id: Option<u64>,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    tty: Option<String>,
    #[serde(default)]
    terminal_pid: Option<u32>,
    #[serde(default)]
    terminal_command: Option<String>,
    #[serde(default)]
    terminal_cwd: Option<String>,
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    wm_class: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

impl PressKeyParams {
    fn window_target(&self) -> WindowTarget {
        WindowTarget {
            window_id: self.window_id,
            pid: self.pid,
            tty: self.tty.clone(),
            terminal_pid: self.terminal_pid,
            terminal_command: self.terminal_command.clone(),
            terminal_cwd: self.terminal_cwd.clone(),
            app_id: self.app_id.clone(),
            wm_class: self.wm_class.clone(),
            title: self.title.clone(),
        }
    }
}

impl TypeTextParams {
    fn window_target(&self) -> WindowTarget {
        WindowTarget {
            window_id: self.window_id,
            pid: self.pid,
            tty: self.tty.clone(),
            terminal_pid: self.terminal_pid,
            terminal_command: self.terminal_command.clone(),
            terminal_cwd: self.terminal_cwd.clone(),
            app_id: self.app_id.clone(),
            wm_class: self.wm_class.clone(),
            title: self.title.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ActionOutput {
    ok: bool,
    implemented: bool,
    action: String,
    message: String,
    received: Option<serde_json::Value>,
}

impl ComputerUseLinux {
    fn should_prefer_portal_pointer_backend(&self) -> bool {
        env::var("CODEX_COMPUTER_USE_FORCE_YDOTOOL_POINTER")
            .ok()
            .as_deref()
            != Some("1")
            && env::var("XDG_SESSION_TYPE")
                .ok()
                .is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
    }

    fn cached_portal_pointer_session(&self) -> Option<PortalPointerSession> {
        self.portal_pointer_session
            .lock()
            .ok()
            .and_then(|cached| cached.clone())
    }

    fn clear_portal_pointer_session(&self) {
        if let Ok(mut cached) = self.portal_pointer_session.lock() {
            *cached = None;
        }
    }

    async fn ensure_portal_pointer_session(&self) -> Result<Option<PortalPointerSession>> {
        if !self.should_prefer_portal_pointer_backend() {
            return Ok(None);
        }
        if let Some(session) = self.cached_portal_pointer_session() {
            return Ok(Some(session));
        }

        let session = start_portal_pointer_session().await?;
        if let Ok(mut cached) = self.portal_pointer_session.lock() {
            *cached = Some(session.clone());
        }
        Ok(Some(session))
    }

    async fn resolve_window_context(
        &self,
        params: &GetAppStateParams,
    ) -> (Option<WindowInfo>, Option<String>, Option<String>) {
        let target = params.window_target();
        if !target.has_target() {
            return (None, None, None);
        }

        match list_windows().await {
            Ok(windows) => match resolve_window_target(&windows, &target) {
                Ok(window) => (Some(window.clone()), None, None),
                Err(error) => (None, Some(format!("{error:#}")), None),
            },
            Err(error) => {
                let error = format!("{error:#}");
                let hint = window_permission_hint(&error);
                (None, Some(error), hint)
            }
        }
    }

    async fn resolve_accessibility_app_filter(
        &self,
        params: &GetAppStateParams,
        window_context: Option<&WindowInfo>,
    ) -> Option<String> {
        if let Some(explicit) = trimmed_nonempty(params.app_name_or_bundle_identifier.as_deref()) {
            return Some(explicit.to_string());
        }

        let target_pid = window_context.and_then(|window| window.pid).or(params.pid);
        let candidates = accessibility_filter_candidates(window_context);

        if let Some(target_pid) = target_pid {
            if let Ok(apps) = list_accessible_apps(200).await {
                if let Some(object_ref) =
                    select_accessibility_object_ref(&apps, target_pid, &candidates)
                {
                    return Some(object_ref);
                }
            }
        }

        candidates.into_iter().next()
    }

    async fn focus_target_for_input(
        &self,
        target: &WindowTarget,
    ) -> std::result::Result<Option<WindowFocusResult>, String> {
        if !target.has_target() {
            return Ok(None);
        }

        let focus = focus_window_target(target).await.map_err(|error| {
            let error = format!("{error:#}");
            if let Some(hint) = window_permission_hint(&error) {
                format!("Did not send input because the target window could not be focused: {error}. {hint}")
            } else {
                format!("Did not send input because the target window could not be focused: {error}")
            }
        })?;

        if focus_satisfies_target(&focus, target) {
            Ok(Some(focus))
        } else {
            let required = if target.requires_exact_focus() {
                "exact target-window focus"
            } else {
                "app-level focus"
            };
            Err(format!(
                "Did not send input because {required} verification failed after activating the target window. Focus result: requested window_id {}, focused window_id {:?}.",
                focus.requested_window.window_id,
                focus.focused_window.as_ref().map(|window| window.window_id)
            ))
        }
    }

    fn cache_nodes(&self, nodes: &[AccessibilityNode]) {
        if let Ok(mut cached) = self.last_nodes.lock() {
            cached.clear();
            cached.extend_from_slice(nodes);
        }
    }

    fn resolve_optional_target_point(
        &self,
        x: Option<i32>,
        y: Option<i32>,
        element_index: Option<u32>,
    ) -> std::result::Result<Option<(i32, i32)>, String> {
        match (x.zip(y), element_index) {
            (Some(point), _) => Ok(Some(point)),
            (None, Some(index)) => self
                .center_for_cached_node(index)
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "No clickable bounds cached for element_index {index}. Call get_app_state first and choose a node with positive width and height."
                    )
                }),
            (None, None) => Ok(None),
        }
    }

    fn resolve_click_target(
        &self,
        params: &ClickParams,
    ) -> std::result::Result<ClickTarget, String> {
        if let Some((x, y)) = params.x.zip(params.y) {
            return Ok(ClickTarget::Coordinates(x, y));
        }

        let selector = params.selector();
        let node = self.resolve_cached_node(
            params.element_index,
            &selector,
            ElementResolvePurpose::Click,
        )?;

        if let Some((x, y)) = node.bounds.as_ref().and_then(bounds_center) {
            return Ok(ClickTarget::Coordinates(x, y));
        }

        if !is_plain_left_click(params.button.as_deref(), params.click_count) {
            return Err(format!(
                "No clickable bounds cached for element_index {}. Call get_app_state first and choose a node with positive width and height.",
                node.index
            ));
        }

        let Some(action_name) = primary_action_name(node.actions.as_slice()) else {
            return Err(format!(
                "No clickable bounds cached for element_index {}, and the element exposes no primary AT-SPI action.",
                node.index
            ));
        };
        Ok(ClickTarget::PrimaryAction {
            object_ref: node.object_ref.clone(),
            action_name: Some(action_name),
        })
    }

    fn center_for_cached_node(&self, element_index: u32) -> Option<(i32, i32)> {
        let cached = self.last_nodes.lock().ok()?;
        let node = cached.iter().find(|node| node.index == element_index)?;
        bounds_center(node.bounds.as_ref()?)
    }

    fn resolve_object_ref(
        &self,
        element_index: Option<u32>,
        element_identifier: Option<&str>,
        selector: &ElementSelector<'_>,
        purpose: ElementResolvePurpose,
    ) -> std::result::Result<String, String> {
        if let Some(element_identifier) = element_identifier
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(element_identifier.to_string());
        }

        self.resolve_cached_node(element_index, selector, purpose)
            .map(|node| node.object_ref)
    }

    fn resolve_cached_node(
        &self,
        element_index: Option<u32>,
        selector: &ElementSelector<'_>,
        purpose: ElementResolvePurpose,
    ) -> std::result::Result<AccessibilityNode, String> {
        let cached = self.last_nodes.lock().map_err(|_| {
            "Could not read cached accessibility nodes. Call get_app_state and retry.".to_string()
        })?;

        if let Some(element_index) = element_index {
            return cached
                .iter()
                .find(|node| node.index == element_index)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "No cached accessibility node for element_index {element_index}. Call get_app_state first."
                    )
                });
        }

        if selector.is_empty() {
            return Err(
                "Pass element_index, element_identifier, or a semantic selector such as role/name/text/states from the latest get_app_state result."
                    .to_string(),
            );
        }

        resolve_semantic_node(cached.as_slice(), selector, purpose)
    }

    async fn perform_element_action(
        &self,
        params: &ActionParams,
        requested_action: Option<&str>,
    ) -> Json<ActionOutput> {
        let received = Some(serde_json::json!(params.clone()));
        let object_ref = match self.resolve_object_ref(
            params.element_index,
            params.element_identifier.as_deref(),
            &params.selector(),
            ElementResolvePurpose::Action,
        ) {
            Ok(object_ref) => object_ref,
            Err(message) => {
                return Json(ActionOutput {
                    ok: false,
                    implemented: true,
                    action: "perform_action".to_string(),
                    message,
                    received,
                });
            }
        };

        match invoke_accessibility_action(&object_ref, requested_action).await {
            Ok(invocation) => Json(ActionOutput {
                ok: invocation.ok,
                implemented: true,
                action: "perform_action".to_string(),
                message: if invocation.ok {
                    format!(
                        "AT-SPI action {} ({}) invoked.",
                        invocation.action_index,
                        invocation
                            .action_name
                            .as_deref()
                            .filter(|name| !name.is_empty())
                            .unwrap_or("unnamed")
                    )
                } else {
                    format!(
                        "AT-SPI action {} ({}) returned false.",
                        invocation.action_index,
                        invocation
                            .action_name
                            .as_deref()
                            .filter(|name| !name.is_empty())
                            .unwrap_or("unnamed")
                    )
                },
                received,
            }),
            Err(error) => Json(ActionOutput {
                ok: false,
                implemented: true,
                action: "perform_action".to_string(),
                message: error.to_string(),
                received,
            }),
        }
    }
}

#[derive(Debug)]
enum ClickTarget {
    Coordinates(i32, i32),
    PrimaryAction {
        object_ref: String,
        action_name: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
enum ElementResolvePurpose {
    Click,
    Action,
    SetValue,
}

#[derive(Debug, Clone, Copy, Default)]
struct ElementSelector<'a> {
    role: Option<&'a str>,
    name: Option<&'a str>,
    text: Option<&'a str>,
    states: &'a [String],
}

impl ElementSelector<'_> {
    fn is_empty(&self) -> bool {
        [self.role, self.name, self.text]
            .into_iter()
            .all(|value| value.map(str::trim).is_none_or(str::is_empty))
            && self.states.iter().all(|value| value.trim().is_empty())
    }
}

fn resolve_semantic_node(
    nodes: &[AccessibilityNode],
    selector: &ElementSelector<'_>,
    purpose: ElementResolvePurpose,
) -> std::result::Result<AccessibilityNode, String> {
    let mut matches = nodes
        .iter()
        .filter(|node| node_matches_selector(node, selector))
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return Err(format!(
            "No cached accessibility node matched semantic selector {}. Call get_app_state first or pass element_index.",
            describe_selector(selector)
        ));
    }

    if let Some(node) =
        unique_preferred_node(&matches, |node| node_matches_resolve_purpose(node, purpose))
    {
        return Ok(node.clone());
    }

    let useful_matches = matches
        .iter()
        .copied()
        .filter(|node| node_matches_resolve_purpose(node, purpose))
        .collect::<Vec<_>>();
    if !useful_matches.is_empty() {
        matches = useful_matches;
    }

    if let Some(node) = unique_preferred_node(&matches, node_is_showing) {
        return Ok(node.clone());
    }

    let visible_matches = matches
        .iter()
        .copied()
        .filter(|node| node_is_showing(node))
        .collect::<Vec<_>>();
    if !visible_matches.is_empty() {
        matches = visible_matches;
    }

    if matches.len() == 1 {
        return Ok(matches[0].clone());
    }

    Err(format!(
        "Semantic selector {} matched multiple cached nodes: {}. Pass element_index or add more selector fields.",
        describe_selector(selector),
        describe_matching_nodes(&matches),
    ))
}

fn unique_preferred_node<'a>(
    nodes: &[&'a AccessibilityNode],
    predicate: impl Fn(&AccessibilityNode) -> bool,
) -> Option<&'a AccessibilityNode> {
    let mut matches = nodes.iter().copied().filter(|node| predicate(node));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn node_matches_selector(node: &AccessibilityNode, selector: &ElementSelector<'_>) -> bool {
    selector
        .role
        .is_none_or(|role| normalized_contains(Some(node.role.as_str()), role))
        && selector
            .name
            .is_none_or(|name| normalized_contains(node.name.as_deref(), name))
        && selector.text.is_none_or(|text| {
            normalized_contains(
                node.text
                    .as_ref()
                    .and_then(|value| value.content.as_deref()),
                text,
            ) || normalized_contains(node.name.as_deref(), text)
                || normalized_contains(node.description.as_deref(), text)
        })
        && selector
            .states
            .iter()
            .filter(|state| !state.trim().is_empty())
            .all(|state| {
                node.states
                    .iter()
                    .any(|node_state| normalized_equals(node_state, state))
            })
}

fn node_matches_resolve_purpose(node: &AccessibilityNode, purpose: ElementResolvePurpose) -> bool {
    match purpose {
        ElementResolvePurpose::Click => {
            node.bounds.as_ref().and_then(bounds_center).is_some()
                || primary_action_name(&node.actions).is_some()
        }
        ElementResolvePurpose::Action => !node.actions.is_empty(),
        ElementResolvePurpose::SetValue => node.supports_editable_text || node.value.is_some(),
    }
}

fn node_is_showing(node: &AccessibilityNode) -> bool {
    node.states
        .iter()
        .any(|state| normalized_equals(state, "showing"))
        && node
            .states
            .iter()
            .any(|state| normalized_equals(state, "visible"))
}

fn normalized_equals(actual: &str, expected: &str) -> bool {
    normalize_text(actual) == normalize_text(expected)
}

fn normalized_contains(actual: Option<&str>, expected: &str) -> bool {
    let expected = normalize_text(expected);
    !expected.is_empty()
        && actual
            .map(normalize_text)
            .is_some_and(|actual| actual.contains(&expected))
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn describe_selector(selector: &ElementSelector<'_>) -> String {
    let mut parts = Vec::new();
    if let Some(role) = selector.role.map(str::trim).filter(|role| !role.is_empty()) {
        parts.push(format!("role={role:?}"));
    }
    if let Some(name) = selector.name.map(str::trim).filter(|name| !name.is_empty()) {
        parts.push(format!("name={name:?}"));
    }
    if let Some(text) = selector.text.map(str::trim).filter(|text| !text.is_empty()) {
        parts.push(format!("text={text:?}"));
    }
    let states = selector
        .states
        .iter()
        .map(|state| state.trim())
        .filter(|state| !state.is_empty())
        .collect::<Vec<_>>();
    if !states.is_empty() {
        parts.push(format!("states={states:?}"));
    }
    if parts.is_empty() {
        "<empty>".to_string()
    } else {
        parts.join(", ")
    }
}

fn describe_matching_nodes(nodes: &[&AccessibilityNode]) -> String {
    nodes
        .iter()
        .take(8)
        .map(|node| {
            format!(
                "element_index {} role={:?} name={:?}",
                node.index, node.role, node.name
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn is_plain_left_click(button: Option<&str>, click_count: Option<u32>) -> bool {
    let button = button.unwrap_or("left");
    let click_count = click_count.unwrap_or(1);
    matches!(button.to_ascii_lowercase().as_str(), "left" | "primary") && click_count == 1
}

fn primary_action_name(actions: &[AccessibilityAction]) -> Option<String> {
    actions.first().map(|action| action.name.clone())
}

fn bounds_center(bounds: &Bounds) -> Option<(i32, i32)> {
    if bounds.width <= 0 || bounds.height <= 0 {
        return None;
    }
    if bounds.x <= i32::MIN / 2 || bounds.y <= i32::MIN / 2 {
        return None;
    }
    Some((
        bounds.x.checked_add(bounds.width / 2)?,
        bounds.y.checked_add(bounds.height / 2)?,
    ))
}

fn compact_accessibility_tree(nodes: Vec<AccessibilityNode>) -> Vec<AccessibilityNode> {
    if nodes.is_empty() {
        return nodes;
    }

    let keep = nodes
        .iter()
        .map(should_keep_accessibility_node)
        .collect::<Vec<_>>();
    let mut old_to_new = vec![None; nodes.len()];
    let mut compacted = Vec::new();

    for (old_index, node) in nodes.iter().enumerate() {
        if !keep[old_index] {
            continue;
        }

        let mut compacted_node = node.clone();
        compacted_node.index = compacted.len() as u32;
        compacted_node.parent_index = nearest_kept_parent(&keep, &nodes, old_index);
        old_to_new[old_index] = Some(compacted_node.index);
        compacted.push(compacted_node);
    }

    for node in &mut compacted {
        node.parent_index = node
            .parent_index
            .and_then(|old_parent| old_to_new.get(old_parent as usize).copied().flatten());
    }

    let child_counts = compacted.iter().filter_map(|node| node.parent_index).fold(
        vec![0_i32; compacted.len()],
        |mut counts, parent_index| {
            counts[parent_index as usize] += 1;
            counts
        },
    );

    for (index, node) in compacted.iter_mut().enumerate() {
        node.child_count = child_counts[index];
    }

    compacted
}

fn nearest_kept_parent(
    keep: &[bool],
    nodes: &[AccessibilityNode],
    old_index: usize,
) -> Option<u32> {
    let mut parent = nodes[old_index].parent_index;
    while let Some(parent_index) = parent {
        let parent_usize = parent_index as usize;
        if keep.get(parent_usize).copied().unwrap_or(false) {
            return Some(parent_index);
        }
        parent = nodes.get(parent_usize).and_then(|node| node.parent_index);
    }
    None
}

fn should_keep_accessibility_node(node: &AccessibilityNode) -> bool {
    if node.depth <= 1 {
        return true;
    }

    if is_actionable_accessibility_node(node) || has_meaningful_node_copy(node) {
        return true;
    }

    matches!(
        node.role.as_str(),
        "page tab" | "menu item" | "menu" | "list item" | "tree item"
    ) && !is_sentinel_or_missing_bounds(node.bounds.as_ref())
}

fn is_actionable_accessibility_node(node: &AccessibilityNode) -> bool {
    !node.actions.is_empty() || node.supports_editable_text || node.value.is_some()
}

fn has_meaningful_node_copy(node: &AccessibilityNode) -> bool {
    has_non_empty_text(node.name.as_deref())
        || has_non_empty_text(node.description.as_deref())
        || has_non_empty_text(node.text.as_ref().and_then(|text| text.content.as_deref()))
}

fn has_non_empty_text(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}

fn is_sentinel_or_missing_bounds(bounds: Option<&Bounds>) -> bool {
    bounds.is_none()
}

fn select_accessibility_object_ref(
    apps: &[AccessibleAppSummary],
    target_pid: u32,
    candidates: &[String],
) -> Option<String> {
    let mut pid_matches = apps.iter().filter(|app| app.pid == Some(target_pid));
    let first = pid_matches.next()?;
    let second = pid_matches.next();

    if second.is_none() {
        return Some(first.object_ref.clone());
    }

    let lowered_candidates = candidates
        .iter()
        .map(|candidate| candidate.to_ascii_lowercase())
        .collect::<Vec<_>>();

    apps.iter()
        .filter(|app| app.pid == Some(target_pid))
        .find(|app| {
            let name = app.name.as_deref().unwrap_or_default().to_ascii_lowercase();
            lowered_candidates
                .iter()
                .any(|candidate| !candidate.is_empty() && name.contains(candidate))
        })
        .map(|app| app.object_ref.clone())
        .or_else(|| Some(first.object_ref.clone()))
}

fn accessibility_filter_candidates(window_context: Option<&WindowInfo>) -> Vec<String> {
    let Some(window) = window_context else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    push_candidate(&mut candidates, window.title.as_deref());
    push_candidate(&mut candidates, window.wm_class.as_deref());

    if let Some(app_id) = trimmed_nonempty(window.app_id.as_deref()) {
        if !app_id.starts_with("window:") {
            push_candidate(&mut candidates, Some(app_id));
            if let Some(stripped) = app_id.strip_suffix(".desktop") {
                push_candidate(&mut candidates, Some(stripped));
                let normalized = stripped.replace(['-', '_', '.'], " ");
                push_candidate(&mut candidates, Some(normalized.as_str()));
            } else {
                let normalized = app_id.replace(['-', '_', '.'], " ");
                push_candidate(&mut candidates, Some(normalized.as_str()));
            }
        }
    }

    candidates
}

fn push_candidate(candidates: &mut Vec<String>, value: Option<&str>) {
    let Some(value) = trimmed_nonempty(value) else {
        return;
    };

    if !candidates.iter().any(|candidate| candidate == value) {
        candidates.push(value.to_string());
    }
}

fn trimmed_nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn action_result(
    action: &str,
    result: std::result::Result<Vec<Output>, String>,
    received: Option<serde_json::Value>,
) -> ActionOutput {
    match result {
        Ok(_) => ActionOutput {
            ok: true,
            implemented: true,
            action: action.to_string(),
            message: "Action sent through ydotool.".to_string(),
            received,
        },
        Err(message) => ActionOutput {
            ok: false,
            implemented: true,
            action: action.to_string(),
            message,
            received,
        },
    }
}

fn action_result_with_focus(
    action: &str,
    result: std::result::Result<Vec<Output>, String>,
    received: Option<serde_json::Value>,
    focus: Option<WindowFocusResult>,
) -> ActionOutput {
    let mut output = action_result(action, result, received);
    if output.ok {
        if let Some(focus) = focus {
            let verification = if focus.exact_window_focused {
                "exact window-focus"
            } else {
                "app-level focus"
            };
            output.message = format!(
                "{} Target window_id {} was focused with {verification} verification before input.",
                output.message, focus.requested_window.window_id,
            );
        }
    }
    output
}

fn focus_satisfies_target(focus: &WindowFocusResult, target: &WindowTarget) -> bool {
    if target.requires_exact_focus() {
        focus.exact_window_focused
    } else {
        focus.exact_window_focused || focus.app_focused
    }
}

async fn window_list_output() -> ListWindowsOutput {
    match list_windows().await {
        Ok(windows) => {
            let backend = window_backend(windows.iter());
            let note = if backend == GNOME_SHELL_EXTENSION_BACKEND {
                "Window list came from the Codex GNOME Shell extension. Terminal windows may include best-effort PTY and active-process context when the process tree is readable."
            } else {
                "Window list came from GNOME Shell Introspect. Terminal windows may include best-effort PTY and active-process context when the process tree is readable."
            };
            ListWindowsOutput {
                backend,
                windows,
                error: None,
                permissions_hint: None,
                note: note.to_string(),
            }
        }
        Err(error) => {
            let error = format!("{error:#}");
            ListWindowsOutput {
                backend: GNOME_SHELL_INTROSPECT_BACKEND.to_string(),
                windows: Vec::new(),
                permissions_hint: window_permission_hint(&error),
                error: Some(error),
                note: "Window listing failed, so targeted keyboard input cannot safely focus or verify a target window."
                    .to_string(),
            }
        }
    }
}

fn window_backend<'a>(windows: impl Iterator<Item = &'a WindowInfo>) -> String {
    windows
        .map(|window| window.backend.clone())
        .next()
        .unwrap_or_else(|| GNOME_SHELL_INTROSPECT_BACKEND.to_string())
}

fn absolute_mousemove_args(x: i32, y: i32) -> Vec<String> {
    vec![
        "mousemove".to_string(),
        "--absolute".to_string(),
        "--".to_string(),
        x.to_string(),
        y.to_string(),
    ]
}

fn wheel_mousemove_args(dx: i32, dy: i32) -> Vec<String> {
    vec![
        "mousemove".to_string(),
        "--wheel".to_string(),
        "--".to_string(),
        dx.to_string(),
        dy.to_string(),
    ]
}

fn run_ydotool_sequence(commands: &[Vec<String>]) -> std::result::Result<Vec<Output>, String> {
    let mut outputs = Vec::new();
    for (index, args) in commands.iter().enumerate() {
        outputs.push(run_ydotool(args)?);
        if index + 1 < commands.len() {
            thread::sleep(Duration::from_millis(35));
        }
    }
    Ok(outputs)
}

fn run_ydotool(args: &[String]) -> std::result::Result<Output, String> {
    let mut command = Command::new("ydotool");
    command.args(args);
    if let Some(socket) = ydotool_socket() {
        command.env("YDOTOOL_SOCKET", socket);
    }

    match command.output() {
        Ok(output) if output.status.success() => Ok(output),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            Err(if detail.is_empty() {
                format!("ydotool exited with {}", output.status)
            } else {
                detail
            })
        }
        Err(error) => Err(format!("failed to run ydotool: {error}")),
    }
}

fn ydotool_socket() -> Option<String> {
    if let Ok(socket) = env::var("YDOTOOL_SOCKET") {
        if !socket.trim().is_empty() {
            return Some(socket);
        }
    }

    let candidates = [
        env::var("XDG_RUNTIME_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| user_id().map(|uid| PathBuf::from(format!("/run/user/{uid}"))))
            .map(|runtime| runtime.join(".ydotool_socket")),
        Some(PathBuf::from("/tmp/.ydotool_socket")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|path| path.exists())
        .map(|path| path.display().to_string())
}

fn mouse_button_code(button: Option<&str>) -> String {
    match button.unwrap_or("left").to_ascii_lowercase().as_str() {
        "right" => "0xC1",
        "middle" => "0xC2",
        "side" => "0xC3",
        "extra" => "0xC4",
        "forward" => "0xC5",
        "back" => "0xC6",
        _ => "0xC0",
    }
    .to_string()
}

fn key_sequence(key: &str) -> Option<Vec<String>> {
    let parts = key
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let (key_part, modifier_parts) = parts.split_last()?;
    if modifier_parts.is_empty() {
        if let Some(modifier) = modifier_keycode(key_part) {
            return Some(vec![format!("{modifier}:1"), format!("{modifier}:0")]);
        }
    }
    let mut modifiers = Vec::new();
    for part in modifier_parts {
        modifiers.push(modifier_keycode(part)?);
    }
    let keycode = keycode(key_part)?;

    let mut events = Vec::new();
    for modifier in &modifiers {
        events.push(format!("{modifier}:1"));
    }
    events.push(format!("{keycode}:1"));
    events.push(format!("{keycode}:0"));
    for modifier in modifiers.iter().rev() {
        events.push(format!("{modifier}:0"));
    }
    Some(events)
}

fn modifier_keycode(key: &str) -> Option<u16> {
    match normalize_key(key).as_str() {
        "ctrl" | "control" => Some(29),
        "alt" | "option" => Some(56),
        "shift" => Some(42),
        "meta" | "super" | "cmd" | "command" => Some(125),
        _ => None,
    }
}

fn keycode(key: &str) -> Option<u16> {
    match normalize_key(key).as_str() {
        "enter" | "return" => Some(28),
        "escape" | "esc" => Some(1),
        "tab" => Some(15),
        "backspace" => Some(14),
        "delete" | "del" => Some(111),
        "space" => Some(57),
        "home" => Some(102),
        "end" => Some(107),
        "pageup" | "page_up" => Some(104),
        "pagedown" | "page_down" => Some(109),
        "arrowleft" | "left" => Some(105),
        "arrowright" | "right" => Some(106),
        "arrowup" | "up" => Some(103),
        "arrowdown" | "down" => Some(108),
        "f1" => Some(59),
        "f2" => Some(60),
        "f3" => Some(61),
        "f4" => Some(62),
        "f5" => Some(63),
        "f6" => Some(64),
        "f7" => Some(65),
        "f8" => Some(66),
        "f9" => Some(67),
        "f10" => Some(68),
        "f11" => Some(87),
        "f12" => Some(88),
        value if value.len() == 1 => keycode_for_ascii(value.as_bytes()[0] as char),
        _ => None,
    }
}

fn normalize_key(key: &str) -> String {
    key.trim().to_ascii_lowercase().replace(['-', ' '], "")
}

fn keycode_for_ascii(value: char) -> Option<u16> {
    match value {
        'a' => Some(30),
        'b' => Some(48),
        'c' => Some(46),
        'd' => Some(32),
        'e' => Some(18),
        'f' => Some(33),
        'g' => Some(34),
        'h' => Some(35),
        'i' => Some(23),
        'j' => Some(36),
        'k' => Some(37),
        'l' => Some(38),
        'm' => Some(50),
        'n' => Some(49),
        'o' => Some(24),
        'p' => Some(25),
        'q' => Some(16),
        'r' => Some(19),
        's' => Some(31),
        't' => Some(20),
        'u' => Some(22),
        'v' => Some(47),
        'w' => Some(17),
        'x' => Some(45),
        'y' => Some(21),
        'z' => Some(44),
        '1' => Some(2),
        '2' => Some(3),
        '3' => Some(4),
        '4' => Some(5),
        '5' => Some(6),
        '6' => Some(7),
        '7' => Some(8),
        '8' => Some(9),
        '9' => Some(10),
        '0' => Some(11),
        _ => None,
    }
}

fn user_id() -> Option<String> {
    let output = Command::new("id").arg("-u").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn list_process_apps() -> Vec<AppCandidate> {
    let output = Command::new("ps")
        .args(["-eo", "pid=,comm=,args="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_process_line)
        .filter(|app| looks_like_desktop_app(&app.name, &app.command))
        .take(50)
        .collect()
}

fn parse_process_line(line: &str) -> Option<AppCandidate> {
    let trimmed = line.trim();
    let mut parts = trimmed.splitn(3, char::is_whitespace);
    let pid = parts.next()?.parse().ok()?;
    let name = parts.next()?.to_string();
    let command = parts.next().unwrap_or("").trim().to_string();
    Some(AppCandidate { name, pid, command })
}

fn looks_like_desktop_app(name: &str, command: &str) -> bool {
    let haystack = format!("{name} {command}").to_ascii_lowercase();
    [
        "codex",
        "electron",
        "chrome",
        "chromium",
        "firefox",
        "brave",
        "code",
        "gnome-terminal",
        "ptyxis",
        "kgx",
        "nautilus",
        "slack",
        "discord",
        "spotify",
        "obsidian",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atspi_tree::{AccessibilityAction, Bounds};
    use crate::windows::WindowBounds;

    fn node(index: u32, bounds: Option<Bounds>) -> AccessibilityNode {
        node_with_actions(index, bounds, Vec::new())
    }

    fn node_with_actions(
        index: u32,
        bounds: Option<Bounds>,
        actions: Vec<AccessibilityAction>,
    ) -> AccessibilityNode {
        AccessibilityNode {
            index,
            parent_index: None,
            depth: 0,
            object_ref: format!(":1.{index}/org/a11y/atspi/accessible/{index}"),
            role: "push button".to_string(),
            name: Some(format!("Button {index}")),
            description: None,
            child_count: 0,
            bounds,
            states: Vec::new(),
            actions,
            value: None,
            text: None,
            supports_editable_text: false,
        }
    }

    fn click_action() -> AccessibilityAction {
        AccessibilityAction {
            index: 0,
            name: "Click".to_string(),
            description: "Clicks the element".to_string(),
            keybinding: String::new(),
        }
    }

    fn window_info(
        window_id: u64,
        title: Option<&str>,
        app_id: Option<&str>,
        wm_class: Option<&str>,
        pid: Option<u32>,
    ) -> WindowInfo {
        WindowInfo {
            window_id,
            title: title.map(str::to_string),
            app_id: app_id.map(str::to_string),
            wm_class: wm_class.map(str::to_string),
            pid,
            bounds: Some(WindowBounds {
                x: Some(10),
                y: Some(20),
                width: 800,
                height: 600,
            }),
            workspace: Some(0),
            focused: false,
            hidden: false,
            client_type: Some("wayland".to_string()),
            backend: GNOME_SHELL_EXTENSION_BACKEND.to_string(),
            terminal: None,
        }
    }

    #[test]
    fn accessibility_filter_candidates_prefer_title_and_skip_synthetic_app_id() {
        let window = window_info(
            42,
            Some("CU ATSPI GTK Test"),
            Some("window:46"),
            Some("cu_atspi_gtk_test.py"),
            Some(2914326),
        );

        let candidates = accessibility_filter_candidates(Some(&window));

        assert_eq!(
            candidates,
            vec![
                "CU ATSPI GTK Test".to_string(),
                "cu_atspi_gtk_test.py".to_string(),
            ]
        );
    }

    #[test]
    fn select_accessibility_object_ref_prefers_exact_pid_match() {
        let apps = vec![
            AccessibleAppSummary {
                object_ref: ":1.31/org/a11y/atspi/accessible/root".to_string(),
                name: Some("electron".to_string()),
                pid: Some(2774076),
                role: "application".to_string(),
                child_count: 1,
                bounds: None,
            },
            AccessibleAppSummary {
                object_ref: ":1.64/org/a11y/atspi/accessible/root".to_string(),
                name: Some("cu_atspi_gtk_test.py".to_string()),
                pid: Some(2914326),
                role: "application".to_string(),
                child_count: 1,
                bounds: None,
            },
        ];

        let object_ref = select_accessibility_object_ref(
            &apps,
            2914326,
            &[
                "CU ATSPI GTK Test".to_string(),
                "cu_atspi_gtk_test.py".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(object_ref, ":1.64/org/a11y/atspi/accessible/root");
    }

    #[test]
    fn compact_accessibility_tree_reparents_actionable_descendants() {
        let nodes = vec![
            AccessibilityNode {
                index: 0,
                parent_index: None,
                depth: 0,
                object_ref: ":1.0/root".to_string(),
                role: "application".to_string(),
                name: Some("demo-app".to_string()),
                description: None,
                child_count: 1,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 1,
                parent_index: Some(0),
                depth: 1,
                object_ref: ":1.1/frame".to_string(),
                role: "frame".to_string(),
                name: Some("Demo Frame".to_string()),
                description: None,
                child_count: 1,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 2,
                parent_index: Some(1),
                depth: 2,
                object_ref: ":1.2/filler".to_string(),
                role: "filler".to_string(),
                name: None,
                description: None,
                child_count: 1,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 3,
                parent_index: Some(2),
                depth: 3,
                object_ref: ":1.3/button".to_string(),
                role: "button".to_string(),
                name: Some("Run".to_string()),
                description: None,
                child_count: 0,
                bounds: Some(Bounds {
                    x: 10,
                    y: 20,
                    width: 100,
                    height: 40,
                }),
                states: Vec::new(),
                actions: vec![AccessibilityAction {
                    index: 0,
                    name: "Click".to_string(),
                    description: "Clicks the button".to_string(),
                    keybinding: String::new(),
                }],
                value: None,
                text: None,
                supports_editable_text: false,
            },
        ];

        let compacted = compact_accessibility_tree(nodes);

        assert_eq!(compacted.len(), 3);
        assert_eq!(compacted[0].role, "application");
        assert_eq!(compacted[1].role, "frame");
        assert_eq!(compacted[2].role, "button");
        assert_eq!(compacted[2].parent_index, Some(1));
        assert_eq!(compacted[1].child_count, 1);
    }

    #[test]
    fn compact_accessibility_tree_drops_structural_noise() {
        let nodes = vec![
            AccessibilityNode {
                index: 0,
                parent_index: None,
                depth: 0,
                object_ref: ":1.0/root".to_string(),
                role: "application".to_string(),
                name: Some("demo-app".to_string()),
                description: None,
                child_count: 2,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 1,
                parent_index: Some(0),
                depth: 1,
                object_ref: ":1.1/frame".to_string(),
                role: "frame".to_string(),
                name: Some("Demo Frame".to_string()),
                description: None,
                child_count: 2,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 2,
                parent_index: Some(1),
                depth: 2,
                object_ref: ":1.2/tab".to_string(),
                role: "page tab".to_string(),
                name: Some("Hidden".to_string()),
                description: None,
                child_count: 0,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
            AccessibilityNode {
                index: 3,
                parent_index: Some(1),
                depth: 2,
                object_ref: ":1.3/separator".to_string(),
                role: "separator".to_string(),
                name: None,
                description: None,
                child_count: 0,
                bounds: None,
                states: Vec::new(),
                actions: Vec::new(),
                value: None,
                text: None,
                supports_editable_text: false,
            },
        ];

        let compacted = compact_accessibility_tree(nodes);

        assert_eq!(compacted.len(), 3);
        assert_eq!(compacted[2].role, "page tab");
        assert_eq!(compacted[2].name.as_deref(), Some("Hidden"));
    }

    #[test]
    fn cached_element_index_resolves_to_bounds_center() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(
            7,
            Some(Bounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
        )]);

        let point = backend
            .resolve_optional_target_point(None, None, Some(7))
            .unwrap()
            .unwrap();

        assert_eq!(point, (60, 40));
    }

    #[test]
    fn coordinate_target_overrides_cached_element_index() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(
            7,
            Some(Bounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
        )]);

        let point = backend
            .resolve_optional_target_point(Some(200), Some(300), Some(7))
            .unwrap()
            .unwrap();

        assert_eq!(point, (200, 300));
    }

    #[test]
    fn cached_element_index_requires_positive_bounds() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(
            7,
            Some(Bounds {
                x: 10,
                y: 20,
                width: 0,
                height: 40,
            }),
        )]);

        let error = backend
            .resolve_optional_target_point(None, None, Some(7))
            .unwrap_err();

        assert!(error.contains("No clickable bounds cached for element_index 7"));
    }

    #[test]
    fn cached_element_index_ignores_sentinel_bounds() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(
            7,
            Some(Bounds {
                x: i32::MIN,
                y: i32::MIN,
                width: 1,
                height: 1,
            }),
        )]);

        let error = backend
            .resolve_optional_target_point(None, None, Some(7))
            .unwrap_err();

        assert!(error.contains("No clickable bounds cached for element_index 7"));
    }

    #[test]
    fn empty_node_cache_clears_stale_element_index() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(
            7,
            Some(Bounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
        )]);
        backend.cache_nodes(&[]);

        let error = backend
            .resolve_optional_target_point(None, None, Some(7))
            .unwrap_err();

        assert!(error.contains("No clickable bounds cached for element_index 7"));
    }

    #[test]
    fn click_target_falls_back_to_primary_action_without_bounds() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node_with_actions(
            7,
            None,
            vec![AccessibilityAction {
                index: 0,
                name: "Click".to_string(),
                description: "Clicks the button".to_string(),
                keybinding: String::new(),
            }],
        )]);

        let target = backend
            .resolve_click_target(&ClickParams {
                element_index: Some(7),
                ..Default::default()
            })
            .unwrap();

        match target {
            ClickTarget::PrimaryAction {
                object_ref,
                action_name,
            } => {
                assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
                assert_eq!(action_name.as_deref(), Some("Click"));
            }
            ClickTarget::Coordinates(_, _) => {
                panic!("expected AT-SPI primary-action fallback")
            }
        }
    }

    #[test]
    fn click_target_falls_back_to_primary_action_with_sentinel_bounds() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node_with_actions(
            7,
            Some(Bounds {
                x: i32::MIN,
                y: i32::MIN,
                width: 1,
                height: 1,
            }),
            vec![AccessibilityAction {
                index: 0,
                name: "Click".to_string(),
                description: "Clicks the button".to_string(),
                keybinding: String::new(),
            }],
        )]);

        let target = backend
            .resolve_click_target(&ClickParams {
                element_index: Some(7),
                ..Default::default()
            })
            .unwrap();

        match target {
            ClickTarget::PrimaryAction {
                object_ref,
                action_name,
            } => {
                assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
                assert_eq!(action_name.as_deref(), Some("Click"));
            }
            ClickTarget::Coordinates(_, _) => {
                panic!("expected AT-SPI primary-action fallback")
            }
        }
    }

    #[test]
    fn click_target_requires_bounds_for_non_plain_clicks() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node_with_actions(
            7,
            None,
            vec![AccessibilityAction {
                index: 0,
                name: "Click".to_string(),
                description: "Clicks the button".to_string(),
                keybinding: String::new(),
            }],
        )]);

        let error = backend
            .resolve_click_target(&ClickParams {
                element_index: Some(7),
                button: Some("right".to_string()),
                ..Default::default()
            })
            .unwrap_err();

        assert!(error.contains("No clickable bounds cached for element_index 7"));
    }

    #[test]
    fn absolute_mousemove_uses_coordinate_separator() {
        assert_eq!(
            absolute_mousemove_args(200, 300),
            vec![
                "mousemove".to_string(),
                "--absolute".to_string(),
                "--".to_string(),
                "200".to_string(),
                "300".to_string(),
            ]
        );
    }

    #[test]
    fn wheel_mousemove_uses_coordinate_separator_for_negative_values() {
        assert_eq!(
            wheel_mousemove_args(0, -3),
            vec![
                "mousemove".to_string(),
                "--wheel".to_string(),
                "--".to_string(),
                "0".to_string(),
                "-3".to_string(),
            ]
        );
    }

    #[test]
    fn pointer_actions_keep_pixel_coordinates_for_ydotool_absolute_moves() {
        assert_eq!(
            absolute_mousemove_args(1550, 930),
            vec![
                "mousemove".to_string(),
                "--absolute".to_string(),
                "--".to_string(),
                "1550".to_string(),
                "930".to_string(),
            ]
        );
    }

    #[test]
    fn key_sequence_presses_modifiers_around_key() {
        assert_eq!(
            key_sequence("Ctrl+Shift+P"),
            Some(vec![
                "29:1".to_string(),
                "42:1".to_string(),
                "25:1".to_string(),
                "25:0".to_string(),
                "42:0".to_string(),
                "29:0".to_string(),
            ])
        );
    }

    #[test]
    fn key_sequence_presses_bare_modifier() {
        assert_eq!(
            key_sequence("Super"),
            Some(vec!["125:1".to_string(), "125:0".to_string()])
        );
    }

    #[test]
    fn element_identifier_overrides_cached_object_ref() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(7, None)]);

        let object_ref = backend
            .resolve_object_ref(
                Some(7),
                Some(":1.99/org/a11y/atspi/accessible/3"),
                &ElementSelector::default(),
                ElementResolvePurpose::Action,
            )
            .unwrap();

        assert_eq!(object_ref, ":1.99/org/a11y/atspi/accessible/3");
    }

    #[test]
    fn element_index_resolves_to_cached_object_ref() {
        let backend = ComputerUseLinux::default();
        backend.cache_nodes(&[node(7, None)]);

        let object_ref = backend
            .resolve_object_ref(
                Some(7),
                None,
                &ElementSelector::default(),
                ElementResolvePurpose::Action,
            )
            .unwrap();

        assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
    }

    #[test]
    fn semantic_selector_resolves_unique_cached_node_by_role_and_name() {
        let backend = ComputerUseLinux::default();
        let mut search_entry = node(7, None);
        search_entry.role = "entry".to_string();
        search_entry.name = Some("Search files".to_string());
        search_entry.supports_editable_text = true;
        backend.cache_nodes(&[search_entry]);

        let object_ref = backend
            .resolve_object_ref(
                None,
                None,
                &ElementSelector {
                    role: Some("entry"),
                    name: Some("search"),
                    ..Default::default()
                },
                ElementResolvePurpose::SetValue,
            )
            .unwrap();

        assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
    }

    #[test]
    fn semantic_selector_prefers_actionable_match() {
        let backend = ComputerUseLinux::default();
        let mut label = node(4, None);
        label.role = "label".to_string();
        label.name = Some("Close".to_string());
        let mut button = node_with_actions(7, None, vec![click_action()]);
        button.role = "push button".to_string();
        button.name = Some("Close".to_string());
        backend.cache_nodes(&[label, button]);

        let object_ref = backend
            .resolve_object_ref(
                None,
                None,
                &ElementSelector {
                    name: Some("close"),
                    ..Default::default()
                },
                ElementResolvePurpose::Action,
            )
            .unwrap();

        assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
    }

    #[test]
    fn semantic_selector_prefers_editable_match() {
        let backend = ComputerUseLinux::default();
        let mut label = node(4, None);
        label.role = "label".to_string();
        label.name = Some("Search".to_string());
        let mut entry = node(7, None);
        entry.role = "entry".to_string();
        entry.name = Some("Search".to_string());
        entry.supports_editable_text = true;
        backend.cache_nodes(&[label, entry]);

        let object_ref = backend
            .resolve_object_ref(
                None,
                None,
                &ElementSelector {
                    name: Some("search"),
                    ..Default::default()
                },
                ElementResolvePurpose::SetValue,
            )
            .unwrap();

        assert_eq!(object_ref, ":1.7/org/a11y/atspi/accessible/7");
    }

    #[test]
    fn semantic_selector_reports_ambiguous_matches() {
        let backend = ComputerUseLinux::default();
        let mut first = node_with_actions(7, None, vec![click_action()]);
        first.name = Some("Close".to_string());
        let mut second = node_with_actions(9, None, vec![click_action()]);
        second.name = Some("Close".to_string());
        backend.cache_nodes(&[first, second]);

        let error = backend
            .resolve_object_ref(
                None,
                None,
                &ElementSelector {
                    name: Some("close"),
                    ..Default::default()
                },
                ElementResolvePurpose::Action,
            )
            .unwrap_err();

        assert!(error.contains("matched multiple cached nodes"));
        assert!(error.contains("element_index 7"));
        assert!(error.contains("element_index 9"));
    }

    #[test]
    fn semantic_click_selector_resolves_coordinates() {
        let backend = ComputerUseLinux::default();
        let mut button = node_with_actions(
            7,
            Some(Bounds {
                x: 10,
                y: 20,
                width: 100,
                height: 40,
            }),
            vec![click_action()],
        );
        button.name = Some("Run".to_string());
        backend.cache_nodes(&[button]);

        let target = backend
            .resolve_click_target(&ClickParams {
                role: Some("button".to_string()),
                name: Some("run".to_string()),
                ..Default::default()
            })
            .unwrap();

        assert!(matches!(target, ClickTarget::Coordinates(60, 40)));
    }
}
