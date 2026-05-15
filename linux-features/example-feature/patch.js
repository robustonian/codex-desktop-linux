"use strict";

function applyMainBundlePatch(source) {
  const marker = "codexLinuxExampleFeatureDisabled()";
  if (!source.includes(marker)) {
    console.warn("WARN: Example Linux feature marker not found — skipping example feature patch");
    return source;
  }
  return source.replace(marker, "codexLinuxExampleFeatureEnabled()");
}

module.exports = {
  applyMainBundlePatch,
};
