"use strict";

const { applyLinuxXdgDocumentsDirPatch } = require("../../../../main-process.js");

module.exports = {
  id: "linux-xdg-documents-dir",
  phase: "main-bundle",
  order: 245,
  ciPolicy: "optional",
  apply: applyLinuxXdgDocumentsDirPatch,
};
