"use strict";

const fs = require("node:fs");
const path = require("node:path");

const FEATURE_ID_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

function defaultLinuxFeaturesRoot() {
  return path.resolve(__dirname, "..", "..", "linux-features");
}

function linuxFeaturesRoot(options = {}) {
  if (options.featuresRoot != null) {
    return path.resolve(options.featuresRoot);
  }
  if (process.env.CODEX_LINUX_FEATURES_ROOT?.trim()) {
    return path.resolve(process.env.CODEX_LINUX_FEATURES_ROOT.trim());
  }
  return defaultLinuxFeaturesRoot();
}

function linuxFeaturesConfigPath(featuresRoot) {
  if (process.env.CODEX_LINUX_FEATURES_CONFIG?.trim()) {
    return path.resolve(process.env.CODEX_LINUX_FEATURES_CONFIG.trim());
  }
  const localConfig = path.join(featuresRoot, "features.json");
  if (fs.existsSync(localConfig)) {
    return localConfig;
  }
  return path.join(featuresRoot, "features.example.json");
}

function readJsonFile(filePath, label) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (error) {
    console.warn(`WARN: Could not read ${label} at ${filePath}: ${error.message}`);
    return null;
  }
}

function normalizeEnabledFeatureIds(value, sourcePath) {
  if (!Array.isArray(value)) {
    console.warn(`WARN: Linux features config ${sourcePath} must contain an enabled array`);
    return [];
  }

  const seen = new Set();
  const ids = [];
  for (const item of value) {
    if (typeof item !== "string" || !FEATURE_ID_PATTERN.test(item)) {
      console.warn(`WARN: Invalid Linux feature id in ${sourcePath}: ${String(item)}`);
      continue;
    }
    if (seen.has(item)) {
      continue;
    }
    seen.add(item);
    ids.push(item);
  }
  return ids;
}

function enabledLinuxFeatureIds(options = {}) {
  const featuresRoot = linuxFeaturesRoot(options);
  const configPath = linuxFeaturesConfigPath(featuresRoot);
  if (!fs.existsSync(configPath)) {
    return [];
  }

  const config = readJsonFile(configPath, "Linux features config");
  if (config == null) {
    return [];
  }
  return normalizeEnabledFeatureIds(config.enabled, configPath);
}

function loadLinuxFeatureManifest(featuresRoot, id) {
  const featureDir = path.join(featuresRoot, id);
  const manifestPath = path.join(featureDir, "feature.json");
  if (!fs.existsSync(manifestPath)) {
    console.warn(`WARN: Enabled Linux feature '${id}' does not have feature.json`);
    return null;
  }

  const manifest = readJsonFile(manifestPath, `Linux feature '${id}' manifest`);
  if (manifest == null) {
    return null;
  }
  if (manifest.id !== id) {
    console.warn(`WARN: Linux feature '${id}' manifest id mismatch: ${String(manifest.id)}`);
    return null;
  }

  return { id, dir: featureDir, manifestPath, manifest };
}

function loadEnabledLinuxFeatures(options = {}) {
  const featuresRoot = linuxFeaturesRoot(options);
  return enabledLinuxFeatureIds({ ...options, featuresRoot })
    .map((id) => loadLinuxFeatureManifest(featuresRoot, id))
    .filter(Boolean);
}

function resolveFeatureEntrypoint(feature, key) {
  const relativePath = feature.manifest.entrypoints?.[key];
  if (relativePath == null) {
    return null;
  }
  if (typeof relativePath !== "string" || relativePath.trim().length === 0) {
    console.warn(`WARN: Linux feature '${feature.id}' has invalid ${key} entrypoint`);
    return null;
  }
  if (path.isAbsolute(relativePath) || relativePath.split(/[\\/]/).includes("..")) {
    console.warn(`WARN: Linux feature '${feature.id}' ${key} entrypoint must stay inside the feature directory`);
    return null;
  }
  const entrypoint = path.resolve(feature.dir, relativePath);
  if (!fs.existsSync(entrypoint)) {
    console.warn(`WARN: Linux feature '${feature.id}' ${key} entrypoint not found: ${entrypoint}`);
    return null;
  }
  return entrypoint;
}

function loadLinuxFeatureMainBundlePatches(options = {}) {
  const patches = [];
  for (const feature of loadEnabledLinuxFeatures(options)) {
    const entrypoint = resolveFeatureEntrypoint(feature, "mainBundlePatch");
    if (entrypoint == null) {
      continue;
    }

    let moduleExports;
    try {
      moduleExports = require(entrypoint);
    } catch (error) {
      console.warn(`WARN: Could not load Linux feature '${feature.id}' mainBundlePatch: ${error.message}`);
      continue;
    }

    const apply = moduleExports.applyMainBundlePatch ?? moduleExports.apply ?? moduleExports;
    if (typeof apply !== "function") {
      console.warn(`WARN: Linux feature '${feature.id}' mainBundlePatch must export a function`);
      continue;
    }

    patches.push({
      name: `feature:${feature.id}`,
      ciPolicy: "optional",
      apply: (source, context) => apply(source, { ...context, feature }),
    });
  }
  return patches;
}

function enabledLinuxFeatureStageHooks(options = {}) {
  return loadEnabledLinuxFeatures(options)
    .map((feature) => ({
      id: feature.id,
      path: resolveFeatureEntrypoint(feature, "stageHook"),
    }))
    .filter((hook) => hook.path != null);
}

function main() {
  const command = process.argv[2];
  if (command === "--stage-hooks") {
    for (const hook of enabledLinuxFeatureStageHooks()) {
      process.stdout.write(`${hook.id}\t${hook.path}\n`);
    }
    return;
  }
  if (command === "--enabled") {
    for (const id of enabledLinuxFeatureIds()) {
      process.stdout.write(`${id}\n`);
    }
    return;
  }
  console.error("Usage: linux-features.js --enabled | --stage-hooks");
  process.exit(1);
}

if (require.main === module) {
  main();
}

module.exports = {
  enabledLinuxFeatureIds,
  enabledLinuxFeatureStageHooks,
  loadEnabledLinuxFeatures,
  loadLinuxFeatureMainBundlePatches,
  linuxFeaturesConfigPath,
  linuxFeaturesRoot,
  resolveFeatureEntrypoint,
};
