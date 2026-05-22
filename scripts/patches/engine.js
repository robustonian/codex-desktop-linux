"use strict";

const fs = require("node:fs");
const path = require("node:path");
const {
  captureWarnings,
  patchStatusFromChange,
  recordPatch,
} = require("../lib/patch-report.js");
const {
  linuxTargetSummary,
} = require("../lib/linux-target-context.js");
const {
  patchAssetFiles,
} = require("./shared.js");

const FAILED_REQUIRED = "failed-required";
const REQUIRED_UPSTREAM = "required-upstream";
const SKIPPED_OPTIONAL = "skipped-optional";
const SKIPPED_TARGET = "skipped-target";

function descriptorId(descriptor) {
  return descriptor.id ?? descriptor.name;
}

function normalizeDescriptor(descriptor, sourcePath = null, index = 0) {
  if (descriptor == null || typeof descriptor !== "object") {
    throw new Error(`Invalid patch descriptor from ${sourcePath ?? "inline descriptor"}`);
  }
  const id = descriptorId(descriptor);
  if (typeof id !== "string" || id.length === 0) {
    throw new Error(`Patch descriptor from ${sourcePath ?? "inline descriptor"} must have id or name`);
  }
  if (typeof descriptor.apply !== "function") {
    throw new Error(`Patch descriptor '${id}' must export an apply function`);
  }
  return {
    ...descriptor,
    id,
    name: descriptor.name ?? id,
    phase: descriptor.phase ?? "main-bundle",
    order: descriptor.order ?? 10_000 + index,
    sourcePath,
  };
}

function descriptorListFromExports(moduleExports, sourcePath) {
  const exported = moduleExports?.descriptors ??
    moduleExports?.patches ??
    moduleExports?.default ??
    moduleExports;
  const descriptors = Array.isArray(exported) ? exported : [exported];
  return descriptors.map((descriptor, index) => normalizeDescriptor(descriptor, sourcePath, index));
}

function discoverPatchFiles(root) {
  if (!root || !fs.existsSync(root)) {
    return [];
  }

  const files = [];
  const walk = (dir) => {
    for (const name of fs.readdirSync(dir).sort()) {
      const filePath = path.join(dir, name);
      const stat = fs.statSync(filePath);
      if (stat.isDirectory()) {
        walk(filePath);
      } else if (name === "patch.js") {
        files.push(filePath);
      }
    }
  };
  walk(root);
  return files;
}

function discoverCorePatchDescriptors(options = {}) {
  const root = options.root ?? path.join(__dirname, "core");
  return sortPatchDescriptors(
    discoverPatchFiles(root).flatMap((filePath) => descriptorListFromExports(require(filePath), filePath)),
  );
}

function sortPatchDescriptors(descriptors) {
  return [...descriptors].sort((left, right) => {
    if (left.order !== right.order) {
      return left.order - right.order;
    }
    return `${left.sourcePath ?? ""}:${left.id}`.localeCompare(`${right.sourcePath ?? ""}:${right.id}`);
  });
}

function assertUniquePatchIds(descriptors) {
  const seen = new Map();
  for (const descriptor of descriptors) {
    const previous = seen.get(descriptor.id);
    if (previous != null) {
      throw new Error(
        `Duplicate patch descriptor id '${descriptor.id}' in ${descriptor.sourcePath ?? "inline descriptor"} and ${previous}`,
      );
    }
    seen.set(descriptor.id, descriptor.sourcePath ?? "inline descriptor");
  }
}

function normalizePatchDescriptors(descriptors) {
  const normalized = descriptors.map((descriptor, index) =>
    normalizeDescriptor(descriptor, descriptor.sourcePath ?? null, index),
  );
  assertUniquePatchIds(normalized);
  return sortPatchDescriptors(normalized);
}

function patchTargetSummary(descriptor, context) {
  if (typeof descriptor.targetSummary === "function") {
    return descriptor.targetSummary(context);
  }
  if (typeof descriptor.targetSummary === "string") {
    return descriptor.targetSummary;
  }
  if (descriptor.appliesTo == null) {
    return "all-linux";
  }
  return context?.linux == null
    ? "conditional-linux"
    : `conditional-linux:${linuxTargetSummary(context.linux)}`;
}

function descriptorFailureStatus(descriptor) {
  return descriptor.ciPolicy === REQUIRED_UPSTREAM ? FAILED_REQUIRED : SKIPPED_OPTIONAL;
}

function patchStatusFromDescriptorChange(descriptor, changed, warnings) {
  if (changed) {
    return "applied";
  }
  if (warnings.length > 0) {
    return descriptorFailureStatus(descriptor);
  }
  return "already-applied";
}

function normalizeDescriptorStatus(descriptor, status) {
  if (descriptor.ciPolicy === REQUIRED_UPSTREAM && status === SKIPPED_OPTIONAL) {
    return FAILED_REQUIRED;
  }
  return status;
}

function recordDescriptorPatch(report, descriptor, status, reason, context) {
  recordPatch(report, descriptor.id, normalizeDescriptorStatus(descriptor, status), reason, {
    phase: descriptor.phase,
    targetSummary: patchTargetSummary(descriptor, context),
  });
}

function descriptorAppliesTo(descriptor, context) {
  if (descriptor.appliesTo == null) {
    return true;
  }
  return descriptor.appliesTo(context) !== false;
}

function descriptorEnabled(descriptor, context) {
  if (descriptor.enabled == null) {
    return true;
  }
  return descriptor.enabled(context) !== false;
}

function applyMainBundlePatchDescriptors(source, descriptors, context, report) {
  let patched = source;
  const warnings = [];
  for (const descriptor of descriptors.filter((patch) => patch.phase === "main-bundle")) {
    if (!descriptorAppliesTo(descriptor, context)) {
      recordDescriptorPatch(report, descriptor, SKIPPED_TARGET, null, context);
      continue;
    }
    if (!descriptorEnabled(descriptor, context)) {
      continue;
    }

    const before = patched;
    const result = captureWarnings(() => descriptor.apply(patched, context));
    patched = result.value;
    warnings.push(...result.warnings);
    recordDescriptorPatch(
      report,
      descriptor,
      patchStatusFromDescriptorChange(descriptor, patched !== before, result.warnings),
      result.warnings[0] ?? null,
      context,
    );
  }
  return { patchedSource: patched, warnings };
}

function defaultWebviewMissingWarning(extractedDir, descriptor) {
  const missingDescription = descriptor.missingDescription ?? "webview asset bundle";
  const skipDescription = descriptor.skipDescription ?? descriptor.id;
  return `WARN: Could not find ${missingDescription} in ${path.join(extractedDir, "webview", "assets")} — skipping ${skipDescription}`;
}

function recordAssetDescriptorPatch(report, descriptor, patchResult, warnings, context) {
  if (patchResult.matched === 0) {
    recordDescriptorPatch(report, descriptor, descriptorFailureStatus(descriptor), warnings[0] ?? "no matching bundle found", context);
    return;
  }
  recordDescriptorPatch(
    report,
    descriptor,
    patchStatusFromDescriptorChange(descriptor, patchResult.changed > 0, warnings),
    warnings[0] ?? null,
    context,
  );
}

function applyWebviewAssetPatchDescriptors(extractedDir, descriptors, context, report) {
  for (const descriptor of descriptors.filter((patch) => patch.phase === "webview-asset")) {
    if (!descriptorAppliesTo(descriptor, context)) {
      recordDescriptorPatch(report, descriptor, SKIPPED_TARGET, null, context);
      continue;
    }
    if (!descriptorEnabled(descriptor, context)) {
      continue;
    }

    const pattern = descriptor.assetPattern ?? descriptor.pattern;
    if (pattern == null) {
      throw new Error(`Webview asset patch '${descriptor.id}' must define assetPattern or pattern`);
    }
    const missingWarning = descriptor.missingWarning ??
      defaultWebviewMissingWarning(extractedDir, descriptor);
    const { value: result, warnings } = captureWarnings(() =>
      patchAssetFiles(extractedDir, pattern, (source) => descriptor.apply(source, context), missingWarning),
    );
    recordAssetDescriptorPatch(report, descriptor, result, warnings, context);
  }
}

function applyExtractedAppPatchDescriptors(extractedDir, descriptors, context, report) {
  for (const descriptor of descriptors.filter((patch) => patch.phase === "extracted-app")) {
    if (!descriptorAppliesTo(descriptor, context)) {
      recordDescriptorPatch(report, descriptor, SKIPPED_TARGET, null, context);
      continue;
    }
    if (!descriptorEnabled(descriptor, context)) {
      continue;
    }

    const { value: result, warnings } = captureWarnings(() => descriptor.apply(extractedDir, context));
    const statusResult = typeof descriptor.status === "function"
      ? descriptor.status(result, warnings, context)
      : result?.changed != null
        ? patchStatusFromChange(Boolean(result.changed), warnings)
        : "applied";
    const status = typeof statusResult === "object" && statusResult != null
      ? statusResult.status
      : statusResult;
    const reason = typeof statusResult === "object" && statusResult != null
      ? statusResult.reason
      : result?.reason ?? warnings[0] ?? null;
    recordDescriptorPatch(report, descriptor, status, reason, context);
  }
}

module.exports = {
  SKIPPED_TARGET,
  applyExtractedAppPatchDescriptors,
  applyMainBundlePatchDescriptors,
  applyWebviewAssetPatchDescriptors,
  assertUniquePatchIds,
  descriptorAppliesTo,
  descriptorEnabled,
  descriptorId,
  discoverCorePatchDescriptors,
  discoverPatchFiles,
  normalizeDescriptor,
  normalizePatchDescriptors,
  patchTargetSummary,
  sortPatchDescriptors,
};
