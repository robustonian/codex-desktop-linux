"use strict";

const fs = require("fs");
const path = require("path");

const { patchPackageJson } = require("../../../../package-json.js");

function readDesktopName(extractedDir) {
  const packageJsonPath = path.join(extractedDir, "package.json");
  if (!fs.existsSync(packageJsonPath)) {
    return null;
  }

  try {
    const parsed = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
    return parsed.desktopName ?? null;
  } catch {
    return null;
  }
}

module.exports = [
  {
    id: "package-desktop-name",
    phase: "extracted-app",
    order: 2040,
    ciPolicy: "required-upstream",
    apply: (extractedDir, context) => {
      const before = readDesktopName(extractedDir);
      patchPackageJson(extractedDir);
      const desktopName = readDesktopName(extractedDir);
      const changed = desktopName !== before;

      if (context?.report) {
        context.report.desktopName = desktopName;
      }
      if (context) {
        context.desktopName = desktopName;
      }

      return { changed, desktopName, matched: desktopName != null };
    },
    status: (result) => ({
      status: result?.desktopName == null
        ? "skipped-optional"
        : result?.changed
          ? "applied"
          : "already-applied",
      reason: result?.desktopName == null ? "package.json missing or unreadable" : null,
    }),
  },
];
