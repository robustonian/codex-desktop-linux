"use strict";

const { patchStatusFromChange } = require("../../../../../lib/patch-report.js");
const { patchCommentPreloadBundle } = require("../../../../webview-assets.js");

module.exports = [
  {
    id: "browser-annotation-screenshot",
    phase: "extracted-app",
    order: 2010,
    ciPolicy: "optional",
    apply: (extractedDir) => patchCommentPreloadBundle(extractedDir),
    status: (result, warnings) => ({
      status: patchStatusFromChange(Boolean(result?.changed), warnings),
      reason: warnings[0] ?? null,
    }),
  },
];
