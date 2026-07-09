# Persistent Status Panel

Keeps the Codex `/status` panel open across thread switches and app restarts
until it is explicitly closed. The existing panel continues to own chat ID,
context usage, and rate-limit rendering.

Enable it in `linux-features/features.json`:

```json
{
  "enabled": ["persistent-status-panel"]
}
```

The webview patch is optional, fail-soft, and idempotent. If the upstream
composer bundle changes shape, the patch warns and leaves the bundle unchanged.
