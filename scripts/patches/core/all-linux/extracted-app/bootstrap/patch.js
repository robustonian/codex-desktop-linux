"use strict";

const {
  patchLinuxMultiInstanceBootstrap,
} = require("../../../../bootstrap.js");

module.exports = {
  id: "linux-multi-instance-bootstrap-lock",
  phase: "extracted-app",
  order: 125,
  ciPolicy: "optional",
  apply: patchLinuxMultiInstanceBootstrap,
};
