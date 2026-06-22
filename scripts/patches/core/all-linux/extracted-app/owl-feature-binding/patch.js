"use strict";

const { patchStatusFromChange } = require("../../../../../lib/patch-report.js");
const { patchLinuxOwlFeatureBindingFallbackAssets } = require("../../../../main-process.js");

module.exports = {
  id: "linux-owl-feature-binding-fallback",
  phase: "extracted-app",
  order: 190,
  ciPolicy: "required-upstream",
  apply: patchLinuxOwlFeatureBindingFallbackAssets,
  status: (result, warnings) => ({
    status: result?.matched === 0
      ? "failed-required"
      : patchStatusFromChange(Boolean(result?.changed), warnings, "required-upstream"),
    reason: result?.matched === 0
      ? "Owl feature binding loader bundle missing"
      : result?.reason ?? warnings[0] ?? null,
  }),
};
