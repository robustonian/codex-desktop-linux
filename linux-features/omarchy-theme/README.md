# Omarchy Theme

Optional integration that makes Codex Desktop follow the active
[Omarchy](https://omarchy.org/) color palette. It is disabled by default.

The feature:

- installs an Omarchy `themed/` template in the user's configuration on first
  launch without overwriting an existing customized template;
- asks `omarchy theme refresh` to generate
  `~/.config/omarchy/current/theme/codex-desktop.css` when needed;
- selects that generated file through the loopback-only Codex webview server's
  generic user-stylesheet endpoint;
- injects a guarded renderer stylesheet loader that refreshes the CSS every five
  seconds and when the window regains focus.

It never edits Omarchy source under `~/.local/share/omarchy/`.

## Enable

Add the feature to the gitignored `linux-features/features.json`:

```json
{
  "enabled": ["omarchy-theme"]
}
```

Then rebuild Codex Desktop with `./install.sh`, `make install-native`, or the
corresponding AppImage/Nix workflow. The generated app must be rebuilt after
changing feature selection.

On first launch the prelaunch hook installs:

```text
~/.config/omarchy/themed/codex-desktop.css.tpl
```

If Omarchy cannot be refreshed automatically, run:

```bash
omarchy theme refresh
```

Subsequent `omarchy theme set ...` and `omarchy theme refresh` operations update
the generated stylesheet, and an open Codex window picks it up within five
seconds.

## Configuration

- `CODEX_LINUX_WEBVIEW_USER_STYLESHEET=/absolute/or/~/path.css` overrides the
  generated CSS file served to Codex.
- `CODEX_OMARCHY_THEME_AUTO_REFRESH=0` prevents the first-launch hook from
  invoking `omarchy theme refresh`.
- `CODEX_OMARCHY_THEME_REFRESH_TIMEOUT_SECONDS=15` changes the bounded wait for
  that refresh. Values must be whole seconds between 1 and 60.

The generic stylesheet endpoint returns empty CSS when the configured file is
missing, not a regular file, unreadable, or larger than 256 KiB.

## Disable and cleanup

Remove `omarchy-theme` from `features.json` and rebuild. Declarative feature
resources and runtime hooks are removed automatically. The user-owned Omarchy
template remains so local customizations are not deleted; remove it manually if
desired, then run `omarchy theme refresh`.

## Test

```bash
node --test linux-features/omarchy-theme/test.js
```

Manual acceptance checks:

1. Launch Codex and confirm its palette matches `omarchy theme current`.
2. Change themes or run `omarchy theme refresh`.
3. Confirm the open Codex window updates within five seconds.
4. Rebuild with the feature disabled and confirm Codex uses its normal theme.

## Risks

- Codex CSS variables and utility selectors can drift with upstream webview
  releases; unsupported selectors simply stop affecting those elements.
- User CSS can obscure controls or reduce contrast. Only load trusted,
  user-owned CSS.
- Polling performs one small loopback stylesheet request every five seconds.
- Existing customized Omarchy templates are never overwritten automatically,
  so they may need manual updates when this feature's template changes.
