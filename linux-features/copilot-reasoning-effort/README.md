# Copilot Reasoning Effort Defaults

This optional Linux feature patches Codex webview bundles so Copilot-auth
sessions can persist and select reasoning effort defaults for new chats.

By default, upstream Copilot-auth paths only read and write
`copilot-default-model`, hardcode the loaded reasoning effort to `medium`, and
collapse Copilot model reasoning effort choices to one `medium` entry. This
feature keeps those changes local and opt-in instead of shipping them as a core
Linux compatibility patch.

Enable it by copying `linux-features/features.example.json` to
`linux-features/features.json` and adding the feature id:

```json
{
  "enabled": [
    "copilot-reasoning-effort"
  ]
}
```

Then rerun the install or package build so the ASAR patch step can apply the
feature to the generated app.

## What It Patches

- `use-model-settings-*.js` reads and writes
  `copilot-default-reasoning-effort` next to `copilot-default-model`.
- `font-settings-*.js` keeps the model's full `supportedReasoningEfforts` list
  for Copilot auth instead of forcing only `medium`.
- `index-*.js` keeps reasoning effort dropdown entries and the `/reasoning`
  command enabled when the normal model and effort prerequisites are present.

## Validation

Run the feature tests with:

```bash
node --test linux-features/copilot-reasoning-effort/test.js
```

Or run all feature tests with:

```bash
node --test linux-features/*/test.js
```

The patch is fail-soft. If the upstream minified bundle shape changes, the
build logs a warning and leaves the affected bundle unchanged.
