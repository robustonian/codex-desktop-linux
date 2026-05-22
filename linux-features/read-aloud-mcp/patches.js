"use strict";

const READ_ALOUD_PLUGIN_NAME = "read-aloud";

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function hasReadAloudPluginGate(source) {
  const pluginGateArray = findBundledPluginGateArray(source);
  const target = pluginGateArray?.text ?? source;
  const nameExpression = pluginNameExpressionRegex(source, READ_ALOUD_PLUGIN_NAME);
  return new RegExp(
    String.raw`\{(?:[^{}]*,)?name:${nameExpression},(?:isEnabled|isAvailable):`,
  ).test(target);
}

function pluginNameExpressionRegex(source, pluginName) {
  const escapedPluginName = escapeRegExp(pluginName);
  const boundName = sourceBoundName(source, pluginName);
  return boundName == null
    ? String.raw`(?:\`${escapedPluginName}\`|"${escapedPluginName}"|'${escapedPluginName}')`
    : String.raw`(?:${escapeRegExp(boundName)}|\`${escapedPluginName}\`|"${escapedPluginName}"|'${escapedPluginName}')`;
}

function sourceBoundName(source, pluginName) {
  return source.match(
    new RegExp(String.raw`([A-Za-z_$][\w$]*)=(?:\`${escapeRegExp(pluginName)}\`|"${escapeRegExp(pluginName)}"|'${escapeRegExp(pluginName)}')`),
  )?.[1] ?? null;
}

function buildReadAloudDescriptor(availabilityProp) {
  return `{installWhenMissing:!0,name:\`${READ_ALOUD_PLUGIN_NAME}\`,${availabilityProp}:({platform:e})=>e===\`linux\`}`;
}

function findMatchingBracket(source, openIndex) {
  let depth = 0;
  let quote = null;
  let escaped = false;

  for (let index = openIndex; index < source.length; index += 1) {
    const char = source[index];
    if (quote != null) {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }

    if (char === "'" || char === "\"" || char === "`") {
      quote = char;
    } else if (char === "[") {
      depth += 1;
    } else if (char === "]") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }

  return -1;
}

function findBundledPluginGateArray(source) {
  let markerIndex = source.indexOf(".computerUse");
  while (markerIndex !== -1) {
    const openIndex = source.lastIndexOf("[", markerIndex);
    if (openIndex === -1) {
      return null;
    }
    const closeIndex = findMatchingBracket(source, openIndex);
    if (closeIndex !== -1 && markerIndex < closeIndex) {
      const text = source.slice(openIndex + 1, closeIndex);
      if (
        text.includes("installWhenMissing") &&
        text.includes("name:") &&
        /(?:isEnabled|isAvailable):/.test(text)
      ) {
        return {
          start: openIndex + 1,
          end: closeIndex,
          text,
        };
      }
    }
    markerIndex = source.indexOf(".computerUse", markerIndex + ".computerUse".length);
  }

  return null;
}

function findAlwaysOnBundledDescriptor(pluginGateArray) {
  const pluginNameExpression =
    "(?:[A-Za-z_$][\\w$]*(?:\\.[A-Za-z_$][\\w$]*)?|`[^`]+`|\"[^\"]+\"|'[^']+')";
  const alwaysOnDescriptorRegex = new RegExp(
    String.raw`\{name:(${pluginNameExpression}),(isEnabled|isAvailable):\(\)=>!0\}`,
    "g",
  );
  let lastMatch = null;
  for (const match of pluginGateArray.text.matchAll(alwaysOnDescriptorRegex)) {
    lastMatch = match;
  }
  return lastMatch;
}

function applyLinuxReadAloudPluginGatePatch(currentSource) {
  if (hasReadAloudPluginGate(currentSource)) {
    return currentSource;
  }

  const pluginGateArray = findBundledPluginGateArray(currentSource);
  if (pluginGateArray == null) {
    if (currentSource.includes(".computerUse")) {
      throw new Error("Required Linux Read Aloud plugin gate patch failed: could not find bundled plugin descriptor array");
    }
    return currentSource;
  }

  const match = findAlwaysOnBundledDescriptor(pluginGateArray);
  if (match == null) {
    throw new Error("Required Linux Read Aloud plugin gate patch failed: could not find bundled plugin descriptor insertion point");
  }

  const [_descriptor, _pluginName, availabilityProp] = match;
  const insertionIndex = pluginGateArray.start + match.index;
  return `${currentSource.slice(0, insertionIndex)}${buildReadAloudDescriptor(availabilityProp)},${currentSource.slice(insertionIndex)}`;
}

const descriptors = [
  {
    id: "linux-read-aloud-plugin-gate",
    phase: "main-bundle",
    order: 155,
    ciPolicy: "required-upstream",
    apply: applyLinuxReadAloudPluginGatePatch,
  },
];

module.exports = {
  READ_ALOUD_PLUGIN_NAME,
  applyLinuxReadAloudPluginGatePatch,
  descriptors,
};
