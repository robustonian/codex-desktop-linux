---
name: record-and-replay
description: "Use when the user wants Codex to record a Linux desktop or browser workflow and turn it into a reusable skill. Requires the Record & Replay event-stream MCP server."
---

# Record & Replay

Record the user's Linux workflow, inspect the event stream, and turn the
repeatable intent into a reusable Codex skill. This is the same bundled
Record & Replay product shell backed by a Linux-native event-stream server,
not a raw pointer or keyboard macro recorder.

The bundled plugin launches `./bin/SkyLinuxComputerUseClient event-stream mcp`.
That helper is the Linux equivalent of the macOS Sky Computer Use event-stream
client, implemented by the Rust `codex-record-replay-linux` backend.

## Flow

1. Call `doctor` when readiness is uncertain. If Computer Use cannot observe the
   desktop, explain the blocker and offer the diagnostics/setup path before
   recording.
2. Call `skysight_start` when recent activity context should keep accumulating
   before or during the demo, or `skysight_snapshot` when you only need a
   point-in-time local activity summary. Use `skysight_pause` and
   `skysight_resume` to stop or continue Chronicle-compatible resources
   without losing the active session, and use `skysight_status` to find the
   resource paths and local OCR availability. Respect
   `skysight_list_exclusions` and update exclusions before recording sensitive
   apps or domains. Treat OCR as local screen evidence metadata; do not copy raw
   OCR text into durable skill drafts unless it is necessary for the reusable
   workflow and safe to retain.
3. Call `event_stream_start` with a short `goal` when matching the upstream
   Record & Replay flow, or `start` when you need Linux-specific options. The
   Linux app should show the active Record & Replay recording HUD while the
   shared runtime status is active. Native Linux audio evidence is opt-in and
   requires both `include_audio: true` and an affirmative
   `CODEX_RECORD_REPLAY_AUDIO` setting; normal workflow recording should rely
   on transcript `speech_context` instead.
   Tell the user that recording is active, that they should perform the workflow
   normally, and that they can say `done` when finished.
4. During the demonstration, call `desktop_snapshot` at meaningful app/window
   changes, such as after the user opens Chrome or arrives on a target site.
   Call `mark` only for meaningful intent boundaries that will help the future
   skill, such as "source page opened" or "finished selecting rows".
5. When transcript text is explicitly available during the recording, call
   `speech_context` with the transcript. Treat the speech as user
   intent/context, not as audio to replay or Chronicle-compatible resources.
   Do not hijack the composer dictation UI as the recording architecture.
6. For browser workflows, call `browser_trace` when browser/CDP trace evidence
   is available. Treat the trace as semantic evidence for drafting the skill,
   not as a click/coordinate replay script.
7. Use `event_stream_status` or `status` if you need to confirm which bundle is
   active. When the user says they are done, asks to stop, or the HUD sends
   "I'm done recording.", call `event_stream_stop` or `stop` if the bundle is
   still active.
8. If the user discards the recording or the HUD cancel control is used, call
   `event_stream_cancel` or `cancel` with `discarded: true` and treat the bundle
   as canceled evidence only.
9. Call `validate_bundle`, then `draft_skill_prompt`.
10. Use the draft prompt and the bundle evidence to create or update a normal
   `SKILL.md`. Prefer stable app names, URLs, semantic UI labels, and data
   shape descriptions over literal coordinates.
11. Call `inspect_skill` before import. Call `import_skill` only after the user
   approves the generated skill.

## Guardrails

- Do not replay raw mouse coordinates or keystroke timing as the primary plan.
- Avoid exposing private captured content unless it is needed to describe the
  reusable workflow shape. This includes spoken transcript context.
- Keep generated skills source-aware: prefer browser DOM, APIs, files, and
  named UI controls before visual matching.
- If the workflow depends on unsupported Linux desktop capabilities, mark the
  skill conditional and include the Computer Use readiness path.
