"use strict";

const {
  applyLinuxAppServerBackfillWaitPatch,
} = require("../../../../webview-assets.js");

module.exports = [
  {
    id: "linux-app-server-backfill-wait",
    phase: "webview-asset",
    order: 1042,
    ciPolicy: "optional",
    pattern: /^(app-server-manager|src)-.*\.js$/,
    missingDescription: "app-server manager webview bundle",
    skipDescription: "Linux app-server backfill wait patch",
    apply: applyLinuxAppServerBackfillWaitPatch,
  },
];
