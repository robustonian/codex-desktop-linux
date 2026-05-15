"use strict";

const { patchKeybindsSettingsAssets } = require("../../../../keybinds-settings.js");

module.exports = [
  {
    id: "keybinds-settings",
    phase: "extracted-app",
    order: 2030,
    ciPolicy: "optional",
    apply: (extractedDir) => patchKeybindsSettingsAssets(extractedDir),
    status: (result, warnings) => ({
      status: result?.changed
        ? "applied"
        : result?.matched
          ? "already-applied"
          : "skipped-optional",
      reason: result?.reason ?? warnings[0] ?? null,
    }),
  },
];
