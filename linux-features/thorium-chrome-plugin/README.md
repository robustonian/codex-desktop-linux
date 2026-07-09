# Thorium Chrome Plugin Support

This optional Linux feature extends the bundled Chrome plugin to recognize
Thorium as a Chromium-family browser.

It is disabled by default because Thorium is a narrower browser variant that the
core Linux port does not regularly test. Enable it by adding the feature id to
`linux-features/features.json` before building or installing:

```json
{
  "enabled": [
    "thorium-chrome-plugin"
  ]
}
```

When enabled, the feature:

- adds Thorium native-messaging manifest locations for the generated launcher
- patches the staged Chrome plugin scripts to detect Thorium installs, profiles,
  running processes, default-browser desktop IDs, and launch commands
- adds Thorium to the Electron-side Chrome extension settings/status helper

Run the focused tests with:

```bash
node --test linux-features/thorium-chrome-plugin/test.js
```
