"use strict";

const {
  applyLinuxQuitGuardPatch,
  applyLinuxExplicitQuitPromptBypassPatch,
  applyLinuxWillQuitDrainTimeoutPatch,
  applyLinuxExplicitTrayQuitPatch,
  applyLinuxExplicitIpcQuitPatch,
} = require("../../../../main-process.js");

module.exports = [
  {
    id: "linux-quit-guard",
    phase: "main-bundle",
    order: 0,
    ciPolicy: "required-upstream",
    apply: applyLinuxQuitGuardPatch,
  },
  {
    id: "linux-explicit-quit-prompt-bypass",
    phase: "main-bundle",
    order: 10,
    ciPolicy: "required-upstream",
    apply: applyLinuxExplicitQuitPromptBypassPatch,
  },
  {
    id: "linux-explicit-quit-drain-timeout",
    phase: "main-bundle",
    order: 20,
    ciPolicy: "required-upstream",
    apply: applyLinuxWillQuitDrainTimeoutPatch,
  },
  {
    id: "linux-explicit-tray-quit",
    phase: "main-bundle",
    order: 30,
    ciPolicy: "required-upstream",
    apply: applyLinuxExplicitTrayQuitPatch,
  },
  {
    id: "linux-explicit-ipc-quit",
    phase: "main-bundle",
    order: 40,
    ciPolicy: "required-upstream",
    apply: applyLinuxExplicitIpcQuitPatch,
  },
];
