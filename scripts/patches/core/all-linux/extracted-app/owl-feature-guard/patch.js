"use strict";

const {
  patchLinuxOwlFeatureGuard,
} = require("../../../../bootstrap.js");

module.exports = {
  id: "linux-owl-feature-guard",
  phase: "extracted-app",
  order: 130,
  ciPolicy: "optional",
  apply: patchLinuxOwlFeatureGuard,
};
