"use strict";

const fs = require("node:fs");
const path = require("node:path");

function applyLinuxMultiInstanceBootstrapPatch(currentSource) {
  const unguardedLock =
    "if(!(!S||n.app.requestSingleInstanceLock()))";
  const guardedLock =
    "if(!(!S||process.platform===`linux`&&process.env.CODEX_LINUX_MULTI_LAUNCH===`1`||n.app.requestSingleInstanceLock()))";

  if (currentSource.includes(guardedLock)) {
    return currentSource;
  }
  if (currentSource.includes(unguardedLock)) {
    return currentSource.replace(unguardedLock, guardedLock);
  }

  if (
    currentSource.includes("requestSingleInstanceLock") &&
    currentSource.includes("Exiting second desktop instance")
  ) {
    console.warn(
      "WARN: Could not find bootstrap single-instance lock — skipping Linux multi-instance bootstrap patch",
    );
  }
  return currentSource;
}

function patchLinuxMultiInstanceBootstrap(extractedDir) {
  const target = path.join(extractedDir, ".vite", "build", "bootstrap.js");
  if (!fs.existsSync(target)) {
    return { changed: false, reason: "bootstrap.js not found" };
  }

  const source = fs.readFileSync(target, "utf8");
  const patched = applyLinuxMultiInstanceBootstrapPatch(source);
  if (patched === source) {
    return { changed: false };
  }

  fs.writeFileSync(target, patched, "utf8");
  return { changed: true };
}

const unguardedOwlFeatureLookup =
  "function Qe(){let e=process._linkedBinding;if(typeof e!=`function`)throw Error(`Owl feature binding is unavailable`);return Ge.parse(e.call(process,`electron_common_owl_features`))}";
const guardedOwlFeatureLookup =
  "function Qe(){let e=process._linkedBinding;if(typeof e!=`function`)return{isOwlFeatureEnabled:()=>false};try{return Ge.parse(e.call(process,`electron_common_owl_features`))}catch(t){return{isOwlFeatureEnabled:()=>false}}}";

function applyLinuxOwlFeatureGuardPatch(currentSource) {
  if (currentSource.includes(guardedOwlFeatureLookup)) {
    return currentSource;
  }
  if (currentSource.includes(unguardedOwlFeatureLookup)) {
    return currentSource.replace(unguardedOwlFeatureLookup, guardedOwlFeatureLookup);
  }

  if (currentSource.includes("electron_common_owl_features")) {
    console.warn(
      "WARN: Could not find bootstrap Owl feature binding lookup in expected shape — skipping Linux Owl feature guard patch",
    );
  }
  return currentSource;
}

function findFileContaining(dir, needle) {
  if (!fs.existsSync(dir)) {
    return null;
  }
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = findFileContaining(entryPath, needle);
      if (found != null) {
        return found;
      }
    } else if (entry.isFile() && entry.name.endsWith(".js")) {
      if (fs.readFileSync(entryPath, "utf8").includes(needle)) {
        return entryPath;
      }
    }
  }
  return null;
}

function patchLinuxOwlFeatureGuard(extractedDir) {
  // The Owl feature-binding lookup lives in whichever chunk the main bundle
  // pulls it into (content-hashed filename, not a fixed path), so locate it
  // by content instead of assuming it's in bootstrap.js.
  const target = findFileContaining(
    path.join(extractedDir, ".vite", "build"),
    "electron_common_owl_features",
  );
  if (target == null) {
    return { changed: false, reason: "owl feature binding lookup not found" };
  }

  const source = fs.readFileSync(target, "utf8");
  const patched = applyLinuxOwlFeatureGuardPatch(source);
  if (patched === source) {
    return { changed: false };
  }

  fs.writeFileSync(target, patched, "utf8");
  return { changed: true, target };
}

module.exports = {
  applyLinuxMultiInstanceBootstrapPatch,
  patchLinuxMultiInstanceBootstrap,
  applyLinuxOwlFeatureGuardPatch,
  patchLinuxOwlFeatureGuard,
};
