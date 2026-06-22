"use strict";

const {
  applyLinuxLocalAppServerFeatureEnablementHandlerPatch,
  applyLinuxRemoteControlConfigPreservationPatch,
} = require("../../../../main-process.js");

module.exports = [
  {
    id: "linux-local-app-server-feature-enablement-handler",
    phase: "main-bundle",
    order: 184,
    ciPolicy: "optional",
    apply: applyLinuxLocalAppServerFeatureEnablementHandlerPatch,
  },
  {
    id: "linux-remote-control-config-preservation",
    phase: "main-bundle",
    order: 185,
    ciPolicy: "optional",
    apply: applyLinuxRemoteControlConfigPreservationPatch,
  },
];
