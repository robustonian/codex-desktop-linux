#!/bin/bash

default_rebuild_report_dir() {
    echo "${REBUILD_REPORT_DIR:-$SCRIPT_DIR/dist-next/rebuild}"
}

prepare_rebuild_report_dir() {
    local report_dir="$1"
    mkdir -p "$report_dir"
    echo "$report_dir"
}

write_rebuild_report_json() {
    local output_path="$1"
    local dmg_path="$2"
    local electron_version="$3"
    local patch_report_path="$4"
    local app_dir="${5:-}"

    mkdir -p "$(dirname "$output_path")"
    node - "$output_path" "$dmg_path" "$electron_version" "$patch_report_path" "$app_dir" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");

const [outputPath, dmgPath, electronVersion, patchReportPath, appDir] = process.argv.slice(2);
const patchReport = fs.existsSync(patchReportPath)
  ? JSON.parse(fs.readFileSync(patchReportPath, "utf8"))
  : { patches: [] };

const report = {
  generatedAt: new Date().toISOString(),
  dmgPath,
  electronVersion,
  appDir: appDir || null,
  mainBundle: patchReport.mainBundle ?? null,
  iconAsset: patchReport.iconAsset ?? null,
  desktopName: patchReport.desktopName ?? null,
  patches: patchReport.patches ?? [],
  patchReportPath: path.resolve(patchReportPath),
};

fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
NODE
}
