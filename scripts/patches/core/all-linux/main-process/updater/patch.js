"use strict";

const { applyLinuxAppUpdaterMenuPatch } = require("../../../../../lib/linux-update-bridge-patch.js");

module.exports = [
  {
    id: "linux-app-updater-menu",
    phase: "main-bundle",
    order: 190,
    ciPolicy: "optional",
    apply: applyLinuxAppUpdaterMenuPatch,
  },
];
