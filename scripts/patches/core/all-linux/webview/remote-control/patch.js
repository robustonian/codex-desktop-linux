"use strict";

const {
  applyLinuxRemoteControlConfigGatePatch,
} = require("../../../../webview-assets.js");

module.exports = [
  {
    id: "linux-remote-control-config-gate",
    phase: "webview-asset",
    order: 1120,
    ciPolicy: "optional",
    pattern: /^remote-connection-visibility-.*\.js$/,
    missingDescription: "remote connection visibility bundle",
    skipDescription: "Linux remote control config gate patch",
    apply: applyLinuxRemoteControlConfigGatePatch,
  },
];
