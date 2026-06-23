"use strict";

const {
  applyLinuxRemoteControlProfileAvailabilityPatch,
} = require("../../../../webview-assets.js");

module.exports = {
  id: "linux-remote-control-profile-availability",
  phase: "webview-asset",
  order: 1120,
  ciPolicy: "optional",
  pattern: /^(?:app-main|remote-connections-settings|use-plugin-install-flow)-.*\.js$/,
  missingDescription: "remote-control profile availability bundle",
  skipDescription: "Linux remote-control profile availability patch",
  apply: applyLinuxRemoteControlProfileAvailabilityPatch,
};
