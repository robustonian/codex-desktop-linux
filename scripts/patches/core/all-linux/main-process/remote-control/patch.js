"use strict";

const {
  mainBundlePatch,
} = require("../../../../descriptor.js");
const {
  applyLinuxLocalAppServerFeatureEnablementHandlerPatch,
  applyLinuxProfiledRemoteControlAuthClientPatch,
  applyLinuxRemoteControlConfigPreservationPatch,
} = require("../../../../impl/main-process/misc.js");

module.exports = [
  mainBundlePatch({
    id: "linux-local-app-server-feature-enablement-handler",
    phase: "main-bundle",
    order: 184,
    ciPolicy: "optional",
    apply: applyLinuxLocalAppServerFeatureEnablementHandlerPatch,
  }),
  mainBundlePatch({
    id: "linux-remote-control-config-preservation",
    phase: "main-bundle",
    order: 185,
    ciPolicy: "optional",
    apply: applyLinuxRemoteControlConfigPreservationPatch,
  }),
  mainBundlePatch({
    id: "linux-profiled-remote-control-auth-client",
    phase: "main-bundle",
    order: 186,
    ciPolicy: "optional",
    apply: applyLinuxProfiledRemoteControlAuthClientPatch,
  }),
];
