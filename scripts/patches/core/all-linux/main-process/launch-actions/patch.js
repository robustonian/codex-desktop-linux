"use strict";

const {
  applyLinuxTrayCloseSettingPatch,
  applyLinuxSettingsPersistencePatch,
  applyLinuxLaunchActionArgsPatch,
  applyLinuxHotkeyWindowPrewarmPatch,
} = require("../../../../launch-actions.js");

module.exports = [
  {
    id: "linux-tray-close-setting",
    phase: "main-bundle",
    order: 200,
    ciPolicy: "optional",
    apply: applyLinuxTrayCloseSettingPatch,
  },
  {
    id: "linux-settings-persistence",
    phase: "main-bundle",
    order: 210,
    ciPolicy: "optional",
    apply: applyLinuxSettingsPersistencePatch,
  },
  {
    id: "linux-launch-actions",
    phase: "main-bundle",
    order: 220,
    ciPolicy: "optional",
    apply: applyLinuxLaunchActionArgsPatch,
  },
  {
    id: "linux-hotkey-window-prewarm",
    phase: "main-bundle",
    order: 230,
    ciPolicy: "optional",
    apply: applyLinuxHotkeyWindowPrewarmPatch,
  },
];
