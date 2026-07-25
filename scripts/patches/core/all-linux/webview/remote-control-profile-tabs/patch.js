"use strict";

const {
  webviewAssetPatch,
} = require("../../../../descriptor.js");
const {
  applyLinuxRemoteControlProfileTabsPatch,
} = require("../../../../impl/webview/index.js");

module.exports = webviewAssetPatch({
  id: "linux-remote-control-profile-tabs",
  phase: "webview-asset",
  order: 1120,
  ciPolicy: "optional",
  pattern: /^remote-connections-settings-.*\.js$/,
  missingDescription: "remote-control profile tabs bundle",
  skipDescription: "Linux remote-control profile tabs patch",
  apply: applyLinuxRemoteControlProfileTabsPatch,
});
