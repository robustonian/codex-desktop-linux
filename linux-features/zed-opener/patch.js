"use strict";

const {
  escapeRegExp,
  findMatchingBrace,
} = require("../../scripts/patches/shared.js");

const PATCH_NAME = "zed-opener feature patch";

function warn(message) {
  console.warn(`WARN: ${message} - skipping ${PATCH_NAME}`);
}

function findZedOpenerBlock(source) {
  const markerStart = source.indexOf("id:`zed`");
  if (markerStart === -1) {
    warn("Could not find Zed opener block");
    return null;
  }

  const blockStart = Math.max(
    source.lastIndexOf("var ", markerStart),
    source.lastIndexOf("let ", markerStart),
    source.lastIndexOf("const ", markerStart),
  );
  const objectStart = blockStart === -1 ? -1 : source.indexOf("{", blockStart);
  const objectEnd = objectStart === -1 ? -1 : findMatchingBrace(source, objectStart);
  if (blockStart === -1 || objectStart === -1 || objectEnd === -1) {
    warn("Could not parse Zed opener block");
    return null;
  }

  const blockEnd = source[objectEnd + 1] === ";" ? objectEnd + 2 : objectEnd + 1;
  return {
    start: blockStart,
    end: blockEnd,
    text: source.slice(blockStart, blockEnd),
  };
}

function findZedPathLookupFunction(source, detectFn) {
  const detectFunctionRegex = new RegExp(
    `function ${escapeRegExp(detectFn)}\\(\\)\\{return ([A-Za-z_$][\\w$]*)\\(\\\`zed\\\`\\)`,
  );
  return source.match(detectFunctionRegex)?.[1] ?? null;
}

function applyMainBundlePatch(currentSource) {
  const block = findZedOpenerBlock(currentSource);
  if (block == null) {
    return currentSource;
  }
  if (block.text.includes("linux:{")) {
    return currentSource;
  }

  const argsFn = block.text.match(/\bargs:([A-Za-z_$][\w$]*)/)?.[1];
  const detectFn = block.text.match(/\bdarwin:\{[^}]*\bdetect:([A-Za-z_$][\w$]*)/)?.[1];
  if (argsFn == null || detectFn == null) {
    warn("Could not identify Zed opener helpers");
    return currentSource;
  }

  const pathLookupFn = findZedPathLookupFunction(currentSource, detectFn);
  if (pathLookupFn == null) {
    warn("Could not identify Zed path lookup helper");
    return currentSource;
  }

  let insertionPoint = block.text.lastIndexOf("}}};");
  if (insertionPoint === -1) {
    insertionPoint = block.text.lastIndexOf("}}}");
  }
  if (insertionPoint === -1) {
    warn("Could not find Zed opener insertion point");
    return currentSource;
  }

  const linuxZed =
    `,linux:{label:\`Zed\`,icon:\`apps/zed.png\`,kind:\`editor\`,detect:()=>${pathLookupFn}(\`zed\`)??${pathLookupFn}(\`zeditor\`)??${pathLookupFn}(\`zedit\`)??${pathLookupFn}(\`zed-cli\`),args:${argsFn}}`;
  const patchedBlock =
    block.text.slice(0, insertionPoint + 1) + linuxZed + block.text.slice(insertionPoint + 1);
  const patchedSource =
    currentSource.slice(0, block.start) + patchedBlock + currentSource.slice(block.end);
  const patchedBlockCheck = patchedSource.slice(block.start, block.start + patchedBlock.length);
  if (
    !patchedBlockCheck.includes("id:`zed`") ||
    !patchedBlockCheck.includes("linux:{label:`Zed`") ||
    !patchedBlockCheck.includes(`${pathLookupFn}(\`zeditor\`)`) ||
    !patchedBlockCheck.includes(`args:${argsFn}`)
  ) {
    console.warn(`WARN: Failed to apply ${PATCH_NAME}`);
    return currentSource;
  }

  return patchedSource;
}

module.exports = {
  applyMainBundlePatch,
};
