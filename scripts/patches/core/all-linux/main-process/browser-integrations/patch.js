"use strict";

const {
  applyBrowserUseNodeReplApprovalPatch,
  applyLinuxChromeExtensionStatusPatch,
} = require("../../../../main-process.js");
const { applyLinuxChromePluginAutoInstallPatch } = require("../../../../chrome-plugin.js");

module.exports = [
  {
    id: "linux-chrome-plugin-auto-install",
    phase: "main-bundle",
    order: 150,
    ciPolicy: "required-upstream",
    apply: applyLinuxChromePluginAutoInstallPatch,
  },
  {
    id: "browser-use-node-repl-approval",
    phase: "main-bundle",
    order: 160,
    ciPolicy: "optional",
    apply: applyBrowserUseNodeReplApprovalPatch,
  },
  {
    id: "linux-chrome-extension-status",
    phase: "main-bundle",
    order: 180,
    ciPolicy: "required-upstream",
    apply: applyLinuxChromeExtensionStatusPatch,
  },
];
