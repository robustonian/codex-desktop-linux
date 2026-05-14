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

function loadFeatureEntrypointModule(feature, key) {
  const entrypoint = resolveFeatureEntrypoint(feature, key);
  if (entrypoint == null) {
    return null;
  }

  try {
    return {
      entrypoint,
      moduleExports: require(entrypoint),
    };
  } catch (error) {
    console.warn(`WARN: Could not load Linux feature '${feature.id}' ${key}: ${error.message}`);
    return null;
  }
}

function featureContext(context, feature) {
  return { ...context, feature };
}

function prefixedFeaturePatchId(feature, descriptorId) {
  return descriptorId.startsWith(`feature:${feature.id}`)
    ? descriptorId
    : `feature:${feature.id}:${descriptorId}`;
}

function wrapFeaturePatchDescriptor(feature, descriptor, sourcePath, index, featureIndex) {
  if (descriptor == null || typeof descriptor !== "object") {
    console.warn(`WARN: Linux feature '${feature.id}' patch descriptor ${index + 1} must be an object`);
    return null;
  }
  if (typeof descriptor.apply !== "function") {
    console.warn(`WARN: Linux feature '${feature.id}' patch descriptor ${index + 1} must export apply`);
    return null;
  }

  const descriptorId = descriptor.id ?? descriptor.name;
  if (typeof descriptorId !== "string" || descriptorId.length === 0) {
    console.warn(`WARN: Linux feature '${feature.id}' patch descriptor ${index + 1} must have id or name`);
    return null;
  }

  const wrapped = {
    ...descriptor,
    id: prefixedFeaturePatchId(feature, descriptorId),
    ciPolicy: descriptor.ciPolicy ?? "optional",
    order: descriptor.order ?? 20_000 + featureIndex * 100 + index * 10,
    sourcePath,
    apply: (target, context) => descriptor.apply(target, featureContext(context, feature)),
  };

  if (typeof descriptor.appliesTo === "function") {
    wrapped.appliesTo = (context) => descriptor.appliesTo(featureContext(context, feature));
  }
  if (typeof descriptor.enabled === "function") {
    wrapped.enabled = (context) => descriptor.enabled(featureContext(context, feature));
  }
  if (typeof descriptor.targetSummary === "function") {
    wrapped.targetSummary = (context) => descriptor.targetSummary(featureContext(context, feature));
  }
  if (typeof descriptor.status === "function") {
    wrapped.status = (result, warnings, context) =>
      descriptor.status(result, warnings, featureContext(context, feature));
  }

  return wrapped;
}

function featurePatchDescriptorListFromExports(feature, moduleExports, sourcePath, featureIndex) {
  const exported = moduleExports?.descriptors ??
    moduleExports?.patches ??
    moduleExports?.default ??
    moduleExports;
  if (exported == null) {
    console.warn(`WARN: Linux feature '${feature.id}' patchDescriptors entrypoint must export descriptors`);
    return [];
  }

  const descriptors = Array.isArray(exported) ? exported : [exported];
  return descriptors
    .map((descriptor, index) =>
      wrapFeaturePatchDescriptor(feature, descriptor, sourcePath, index, featureIndex),
    )
    .filter(Boolean);
}

function loadLinuxFeaturePatchDescriptors(options = {}) {
  const descriptors = [];
  for (const [featureIndex, feature] of loadEnabledLinuxFeatures(options).entries()) {
    const loaded = loadFeatureEntrypointModule(feature, "patchDescriptors");
    if (loaded == null) {
      continue;
    }
    descriptors.push(
      ...featurePatchDescriptorListFromExports(
        feature,
        loaded.moduleExports,
        loaded.entrypoint,
        featureIndex,
      ),
    );
  }
  return descriptors;
}

function loadLinuxFeatureMainBundlePatches(options = {}) {
  const patches = [];
  for (const feature of loadEnabledLinuxFeatures(options)) {
    const loaded = loadFeatureEntrypointModule(feature, "mainBundlePatch");
    if (loaded == null) {
      continue;
    }

    const moduleExports = loaded.moduleExports;
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
  loadLinuxFeaturePatchDescriptors,
  loadLinuxFeatureMainBundlePatches,
  linuxFeaturesConfigPath,
  linuxFeaturesRoot,
  resolveFeatureEntrypoint,
};
