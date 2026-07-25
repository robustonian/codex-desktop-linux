# Remote Control UI

Opt-in Linux UI patches for upstream `remote_control` and related settings surfaces.

This feature only opens the Linux UI gates. It does not fake backend state such
as connected clients, MFA completion, or remote control environments.

Enable it locally with:

```json
{
  "enabled": [
    "remote-control-ui"
  ]
}
```

Run the feature tests with:

```bash
node --test linux-features/remote-control-ui/test.js
```
