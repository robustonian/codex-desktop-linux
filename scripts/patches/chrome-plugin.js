"use strict";

function hasChromePluginLiteral(source) {
  return /(?:`chrome`|"chrome"|'chrome')/.test(source);
}

function isChromeNameExpr(nameExpr, chromeNameVar) {
  return /^(?:`chrome`|"chrome"|'chrome')$/.test(nameExpr) ||
    nameExpr === chromeNameVar;
}

function hasChromeAutoInstall(source, chromeNameVar) {
  const namePatterns = [String.raw`\`chrome\``, "\"chrome\"", "'chrome'"];
  if (chromeNameVar != null) {
    namePatterns.push(chromeNameVar);
  }
  return new RegExp(String.raw`installWhenMissing:!0,name:(?:${namePatterns.join("|")})`).test(source);
}

function applyLinuxChromePluginAutoInstallPatch(currentSource) {
  if (!hasChromePluginLiteral(currentSource)) {
    console.warn(
      "WARN: Could not find Chrome plugin gate literal — skipping Linux Chrome plugin auto-install patch",
    );
    return currentSource;
  }

  const chromeNameVar = currentSource.match(/([A-Za-z_$][\w$]*)=(?:`chrome`|"chrome"|'chrome')/)?.[1] ?? null;
  const nameExpressionPattern = String.raw`(?:[A-Za-z_$][\w$]*|` +
    String.raw`\`chrome\`|"chrome"|'chrome')`;
  const gateRegex =
    new RegExp(String.raw`\{([^{}]*?)(installWhenMissing:!0,)?name:(${nameExpressionPattern}),(isEnabled|isAvailable):\(\{([^}]*)\}\)=>([^{}]*?externalBrowserUseAllowed[^{}]*?)(,migrate:[A-Za-z_$][\w$]*)?\}`, "g");

  let sawChromeGate = false;
  let sawAlreadyInstalledGate = false;
  const patched = currentSource.replace(
    gateRegex,
    (gateSource, prefix, installWhenMissing, nameExpr, availabilityProp, paramsText, expression, migrateSuffix = "") => {
      if (!isChromeNameExpr(nameExpr, chromeNameVar)) {
        return gateSource;
      }

      sawChromeGate = true;
      if (installWhenMissing != null || prefix.includes("installWhenMissing:!0")) {
        sawAlreadyInstalledGate = true;
        return gateSource;
      }

      return `{${prefix}installWhenMissing:!0,name:${nameExpr},${availabilityProp}:({${paramsText}})=>${expression}${migrateSuffix}}`;
    },
  );

  if (patched !== currentSource || (sawChromeGate && sawAlreadyInstalledGate)) {
    return patched;
  }

  if (hasChromeAutoInstall(currentSource, chromeNameVar)) {
    return currentSource;
  }

  if (currentSource.includes("externalBrowserUseAllowed")) {
    throw new Error("Required Linux Chrome plugin auto-install patch failed: could not enable bundled Chrome auto-install");
  }

  console.warn(
    "WARN: Could not find Chrome plugin auto-install gate — skipping Linux Chrome plugin auto-install patch",
  );
  return currentSource;
}

module.exports = {
  applyLinuxChromePluginAutoInstallPatch,
};
