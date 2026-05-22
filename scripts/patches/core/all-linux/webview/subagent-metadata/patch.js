"use strict";

const {
  applySubagentNicknameMetadataPatch,
} = require("../../../../webview-assets.js");

module.exports = [
  {
    id: "subagent-nickname-metadata-shape",
    phase: "webview-asset",
    order: 1050,
    ciPolicy: "required-upstream",
    pattern: /^app-server-manager-signals-.*\.js$/,
    missingDescription: "app-server manager webview bundle",
    skipDescription: "subagent nickname metadata shape patch",
    apply: applySubagentNicknameMetadataPatch,
  },
];
