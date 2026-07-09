# Skill fixture pack

This directory contains import/classification fixtures for the record-replay skill importer.

## Cases

- `instruction-only/` — pure instruction payload, should classify as
  `instruction-only` and default import target is `user`.
- `platform-macos/` — platform-specific payload for `macos`, should be rejected
  when importing on Linux.
- `desktop-act/` — desktop action payload, should classify as `desktop-act`.
- `unsafe-symlink/` — contains traversal/symlink risk, should be rejected
  before any file or script execution.
- `collision-tripwire/` — destination collision with existing skill must fail and
  tripwire script is fixture data only (must never execute).
