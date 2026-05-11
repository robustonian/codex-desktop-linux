# Linux Features

`linux-features/` contains opt-in Linux integration modules for this wrapper.
These are not upstream Codex plugins; they are Linux-side extensions that can
add ASAR patches, staged resources, or build/install hooks.

By default, no optional Linux features are enabled. Copy
`features.example.json` to `features.json` before running `./install.sh` or
building packages, then list the feature ids you want:

```json
{
  "enabled": [
    "example-feature"
  ]
}
```

`features.json` is ignored by git so local choices do not leak into commits.
Feature choices are read during the install/build pipeline; if you change this
file after an app has already been generated, rerun the install/build step.

Each feature directory should include:

- `feature.json` — metadata and entrypoints
- `README.md` — what it does, how to test it, and known risks
- optional `patch.js` — exports `applyMainBundlePatch(source, context)`
- optional `stage.sh` — install/build staging hook
- optional `test.js` — self-contained tests for the feature

`stage.sh` hooks run with `SCRIPT_DIR`, `INSTALL_DIR`, `WORK_DIR`, `ARCH`, and
`CODEX_UPSTREAM_APP_DIR` in the environment.

Feature self-tests live inside each feature directory. Run them with:

```bash
node --test linux-features/*/test.js
```

Core Linux compatibility patches should stay in `scripts/patches/` until they
are deliberately migrated. Use `linux-features/` for additions that are useful
for some users but not mandatory for every Linux build.
