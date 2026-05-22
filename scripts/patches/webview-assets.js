"use strict";

const fs = require("node:fs");
const path = require("node:path");

// Webview asset patches target hashed browser chunks copied out of app.asar.
// They stay fail-soft because upstream chunk names and minified symbols drift.
function applyLinuxOpaqueWindowsDefaultPatch(currentSource) {
  let patchedSource = currentSource;

  const linuxPlatformCheck =
    "(typeof navigator<`u`&&((navigator.userAgentData?.platform??navigator.platform??navigator.userAgent).toLowerCase().includes(`linux`)))";

  const mergeNeedle = "opaqueWindows:e?.opaqueWindows??n.opaqueWindows,semanticColors:";
  const mergePatch =
    `opaqueWindows:e?.opaqueWindows??(${linuxPlatformCheck}?!0:n.opaqueWindows),semanticColors:`;
  const mergeRegex =
    /(opaqueWindows:)([A-Za-z_$][\w$]*)\?\.opaqueWindows\?\?([A-Za-z_$][\w$]*)\.opaqueWindows(,semanticColors:)/u;

  if (patchedSource.includes("opaqueWindows:e?.opaqueWindows??((typeof navigator<`u`&&")) {
    // Already patched.
  } else if (patchedSource.includes(mergeNeedle)) {
    patchedSource = patchedSource.replace(mergeNeedle, mergePatch);
  } else if (mergeRegex.test(patchedSource)) {
    patchedSource = patchedSource.replace(
      mergeRegex,
      (_match, prefix, themePatchVar, defaultThemeVar, suffix) =>
        `${prefix}${themePatchVar}?.opaqueWindows??(${linuxPlatformCheck}?!0:${defaultThemeVar}.opaqueWindows)${suffix}`,
    );
  } else if (patchedSource.includes("opaqueWindows") && patchedSource.includes("semanticColors")) {
    console.warn(
      "WARN: Could not find Linux opaque window default insertion point — skipping settings default patch",
    );
  }

  const settingsNeedle =
    "let d=ot(r,e),f=at(e),p={codeThemeId:tt(a,e).id,theme:d},";
  const settingsPatch =
    "let d=ot(r,e);navigator.userAgent.includes(`Linux`)&&r?.opaqueWindows==null&&(d={...d,opaqueWindows:!0});let f=at(e),p={codeThemeId:tt(a,e).id,theme:d},";
  if (patchedSource.includes("navigator.userAgent.includes(`Linux`)&&r?.opaqueWindows==null")) {
    // Already patched.
  } else if (patchedSource.includes(settingsNeedle)) {
    patchedSource = patchedSource.replace(settingsNeedle, settingsPatch);
  }

  const currentSettingsNeedle = "setThemePatch:b,theme:x}=ne(t),S=$t(i,t),";
  const currentSettingsPatch =
    "setThemePatch:b,theme:x}=ne(t);navigator.userAgent.includes(`Linux`)&&x?.opaqueWindows==null&&(x={...x,opaqueWindows:!0});let S=$t(i,t),";
  if (patchedSource.includes("navigator.userAgent.includes(`Linux`)&&x?.opaqueWindows==null")) {
    // Already patched.
  } else if (patchedSource.includes(currentSettingsNeedle)) {
    patchedSource = patchedSource.replace(currentSettingsNeedle, currentSettingsPatch);
  }

  const currentSettingsRegex =
    /(\{canImportThemeString:[^{}]+setThemePatch:([A-Za-z_$][\w$]*),theme:([A-Za-z_$][\w$]*)\}=([A-Za-z_$][\w$]*)\(([A-Za-z_$][\w$]*)\)),([A-Za-z_$][\w$]*)=/u;
  if (patchedSource.includes("codexLinuxIsPlatform()") || patchedSource.includes("codexLinuxOpaqueThemeDefault")) {
    // Already patched by the current-shape fallback.
  } else {
    patchedSource = patchedSource.replace(
      currentSettingsRegex,
      (_match, prefix, _setThemeVar, themeVar, _themeHookVar, _variantVar, nextVar) =>
        `${prefix};let codexLinuxIsPlatform=()=>${linuxPlatformCheck},codexLinuxOpaqueThemeDefault=${themeVar}?.opaqueWindows==null&&codexLinuxIsPlatform();${themeVar}=codexLinuxOpaqueThemeDefault?{...${themeVar},opaqueWindows:!0}:${themeVar};let ${nextVar}=`,
    );
  }

  const runtimeNeedle =
    "let T=o===`light`?C:w,E;if(T.opaqueWindows&&!XZ()){";
  const runtimePatch =
    "let T=o===`light`?C:w,E;document.documentElement.dataset.codexOs===`linux`&&((o===`light`?l:f)?.opaqueWindows==null&&(T={...T,opaqueWindows:!0}));if(T.opaqueWindows&&!XZ()){";
  if (patchedSource.includes("document.documentElement.dataset.codexOs===`linux`&&((o===`light`?l:f)?.opaqueWindows==null")) {
    // Already patched.
  } else if (patchedSource.includes(runtimeNeedle)) {
    patchedSource = patchedSource.replace(runtimeNeedle, runtimePatch);
  }

  const currentRuntimeNeedle = "let T=s===`light`?S:w,E;";
  const currentRuntimePatch =
    "let T=s===`light`?S:w,E;document.documentElement.dataset.codexOs===`linux`&&((s===`light`?u:p)?.opaqueWindows==null&&(T={...T,opaqueWindows:!0}));";
  if (patchedSource.includes("document.documentElement.dataset.codexOs===`linux`&&((s===`light`?u:p)?.opaqueWindows==null")) {
    // Already patched.
  } else if (patchedSource.includes(currentRuntimeNeedle)) {
    patchedSource = patchedSource.replace(currentRuntimeNeedle, currentRuntimePatch);
  }

  const appMainRuntimeRegex =
    /(let\{data:([A-Za-z_$][\w$]*)\}=[^;]*APPEARANCE_LIGHT_CHROME_THEME[\s\S]{0,1200}?let\{data:([A-Za-z_$][\w$]*)\}=[^;]*APPEARANCE_DARK_CHROME_THEME[\s\S]{0,2000}?let ([A-Za-z_$][\w$]*)=([A-Za-z_$][\w$]*)===`light`\?([A-Za-z_$][\w$]*):([A-Za-z_$][\w$]*)),([A-Za-z_$][\w$]*);/u;
  if (patchedSource.includes("codexLinuxDefaultOpaqueWindows")) {
    // Already patched by the app-main fallback.
  } else {
    patchedSource = patchedSource.replace(
      appMainRuntimeRegex,
      (
        _match,
        prefix,
        lightThemeVar,
        darkThemeVar,
        resolvedThemeVar,
        appearanceVar,
        _lightResolvedVar,
        _darkResolvedVar,
        nextVar,
      ) =>
        `${prefix};document.documentElement.dataset.codexOs===\`linux\`&&(((${appearanceVar}===\`light\`?${lightThemeVar}:${darkThemeVar})?.opaqueWindows)==null&&(${resolvedThemeVar}={...${resolvedThemeVar},opaqueWindows:!0}));let codexLinuxDefaultOpaqueWindows,${nextVar};`,
    );
  }

  return patchedSource;
}

function applyLinuxAppSunsetPatch(currentSource) {
  const statsigKey = "2929582856";
  const disabledGatePattern = /if\(!1&&([A-Za-z_$][\w$]*)\(`2929582856`\)\)\{/u;
  const gatePattern = /if\(([A-Za-z_$][\w$]*)\(`2929582856`\)\)\{/u;

  if (disabledGatePattern.test(currentSource)) {
    return currentSource;
  }

  if (gatePattern.test(currentSource)) {
    return currentSource.replace(gatePattern, "if(!1&&$1(`2929582856`)){");
  }

  if (currentSource.includes(statsigKey)) {
    console.warn("WARN: Could not find app sunset gate needle — skipping Linux app sunset patch");
  }

  return currentSource;
}

function applyLinuxRemoteControlConfigGatePatch(currentSource) {
  if (currentSource.includes("features.remote_control")) {
    return currentSource;
  }

  const remoteConnectionsFunctionMatch = currentSource.match(
    /function\s+([A-Za-z_$][\w$]*)\(\)\{let\s+[A-Za-z_$][\w$]*=\(0,[A-Za-z_$][\w$]*\.c\)\(3\),\{data:([A-Za-z_$][\w$]*)\}=([A-Za-z_$][\w$]*)\(([A-Za-z_$][\w$]*),([A-Za-z_$][\w$]*)\(([A-Za-z_$][\w$]*)\)\),[A-Za-z_$][\w$]*=([A-Za-z_$][\w$]*)\(`4114442250`\);if\(\2\?\.config\[`features\.remote_connections`\]===!0\)return!0;/u,
  );

  const remoteControlGateRegex = /function\s+([A-Za-z_$][\w$]*)\(\)\{return\s+([A-Za-z_$][\w$]*)\(`1042620455`\)\}/u;
  const remoteControlGateMatch = currentSource.match(remoteControlGateRegex);
  if (remoteConnectionsFunctionMatch != null && remoteControlGateMatch != null) {
    const [
      ,
      ,
      configVar,
      useConfigQueryFn,
      configQueryArg,
      hostConfigFn,
      hostConfigArg,
      statsigFn,
    ] = remoteConnectionsFunctionMatch;
    const [, remoteControlFunctionName, remoteControlStatsigFn] = remoteControlGateMatch;
    if (statsigFn === remoteControlStatsigFn) {
      return currentSource.replace(
        remoteControlGateRegex,
        `function ${remoteControlFunctionName}(){let{data:codexLinuxRemoteControlConfig}=${useConfigQueryFn}(${configQueryArg},${hostConfigFn}(${hostConfigArg})),codexLinuxRemoteControlStatsig=${remoteControlStatsigFn}(\`1042620455\`);if(codexLinuxRemoteControlConfig?.config[\`features.remote_control\`]===!0)return!0;let codexLinuxRemoteControlFeatures=codexLinuxRemoteControlConfig?.config.features;return typeof codexLinuxRemoteControlFeatures==\`object\`&&!!codexLinuxRemoteControlFeatures&&!Array.isArray(codexLinuxRemoteControlFeatures)?Object.getOwnPropertyDescriptor(codexLinuxRemoteControlFeatures,\`remote_control\`)?.value===!0||codexLinuxRemoteControlStatsig:codexLinuxRemoteControlStatsig}`,
      );
    }
  }

  const fallbackGateMatch = currentSource.match(remoteControlGateRegex);
  if (remoteConnectionsFunctionMatch != null && fallbackGateMatch != null) {
    const [, remoteConnectionsFunctionName] = remoteConnectionsFunctionMatch;
    const [, remoteControlFunctionName, remoteControlStatsigFn] = fallbackGateMatch;
    return currentSource.replace(
      remoteControlGateRegex,
      `function ${remoteControlFunctionName}(){return ${remoteConnectionsFunctionName}()||${remoteControlStatsigFn}(\`1042620455\`)}`,
    );
  }

  if (currentSource.includes("1042620455")) {
    console.warn(
      "WARN: Could not find remote control visibility gate needle — skipping Linux remote control config gate patch",
    );
  }

  return currentSource;
}

function applyBrowserAnnotationScreenshotPatch(currentSource) {
  let patchedSource = currentSource;

  const liveElementScreenshotNeedle =
    "if(M&&j?.anchor.kind===`element`){let e=qu(j,y.current)??null,t=e==null?null:rd(e);he=t?.rect??md(j.anchor),_e=t?.borderRadius}";
  const storedAnchorScreenshotPatch =
    "if(M&&j?.anchor.kind===`element`){he=md(j.anchor),_e=void 0}";
  if (patchedSource.includes(storedAnchorScreenshotPatch)) {
    // Already patched.
  } else if (
    /if\([A-Za-z_$][\w$]*&&[A-Za-z_$][\w$]*\?\.anchor\.kind===`element`\)\{[A-Za-z_$][\w$]*=[A-Za-z_$][\w$]*\([A-Za-z_$][\w$]*\.anchor\),[A-Za-z_$][\w$]*=void 0\}/.test(patchedSource)
  ) {
    // Already patched with the current upstream symbol names.
  } else if (patchedSource.includes(liveElementScreenshotNeedle)) {
    patchedSource = patchedSource.replace(liveElementScreenshotNeedle, storedAnchorScreenshotPatch);
  } else {
    const currentElementScreenshotRegex =
      /if\(([A-Za-z_$][\w$]*)&&([A-Za-z_$][\w$]*)\?\.anchor\.kind===`element`\)\{let e=[^;{}]+?\?\?null,t=e==null\?null:[A-Za-z_$][\w$]*\(e\);([A-Za-z_$][\w$]*)=t\?\.rect\?\?([A-Za-z_$][\w$]*)\(\2\.anchor\),([A-Za-z_$][\w$]*)=t\?\.borderRadius\}/;
    const currentElementScreenshotMatch = patchedSource.match(currentElementScreenshotRegex);
    if (currentElementScreenshotMatch != null) {
      const [, screenshotModeVar, selectedCommentVar, rectVar, anchorRectFn, radiusVar] = currentElementScreenshotMatch;
      patchedSource = patchedSource.replace(
        currentElementScreenshotRegex,
        `if(${screenshotModeVar}&&${selectedCommentVar}?.anchor.kind===\`element\`){${rectVar}=${anchorRectFn}(${selectedCommentVar}.anchor),${radiusVar}=void 0}`,
      );
    } else {
      console.warn("WARN: Could not find browser annotation screenshot element highlight — skipping screenshot anchor patch");
    }
  }

  const allMarkersInScreenshotNeedle =
    "de=u?.target.mode===`create`?ce.find(e=>Sd(e.anchor,u.anchor.value))??null:null,fe=!M&&de!=null?ce.filter(e=>e.id!==de.id):ce,";
  const selectedMarkerInScreenshotPatch =
    "de=u?.target.mode===`create`?ce.find(e=>Sd(e.anchor,u.anchor.value))??null:null,fe=M?ue:!M&&de!=null?ce.filter(e=>e.id!==de.id):ce,";
  if (patchedSource.includes(selectedMarkerInScreenshotPatch)) {
    // Already patched.
  } else if (/=\([A-Za-z_$][\w$]*\?[A-Za-z_$][\w$]*:![A-Za-z_$][\w$]*&&[A-Za-z_$][\w$]*!=null\?[A-Za-z_$][\w$]*\.filter\(e=>e\.id!==[A-Za-z_$][\w$]*\.id\):[A-Za-z_$][\w$]*\)\.flatMap/.test(patchedSource)) {
    // Already patched with the current upstream symbol names.
  } else if (patchedSource.includes(allMarkersInScreenshotNeedle)) {
    patchedSource = patchedSource.replace(allMarkersInScreenshotNeedle, selectedMarkerInScreenshotPatch);
  } else {
    const currentMarkersNeedle = "be=(!ge&&ye!=null?A.filter(e=>e.id!==ye.id):A).flatMap";
    const currentMarkersPatch = "be=(ge?he:!ge&&ye!=null?A.filter(e=>e.id!==ye.id):A).flatMap";
    if (patchedSource.includes(currentMarkersPatch)) {
      // Already patched.
    } else if (patchedSource.includes(currentMarkersNeedle)) {
      patchedSource = patchedSource.replace(currentMarkersNeedle, currentMarkersPatch);
    } else {
      console.warn("WARN: Could not find browser annotation screenshot markers — skipping screenshot marker patch");
    }
  }

  return patchedSource;
}

function patchCommentPreloadBundle(extractedDir) {
  const commentPreloadBundle = path.join(extractedDir, ".vite", "build", "comment-preload.js");
  if (!fs.existsSync(commentPreloadBundle)) {
    console.warn(
      `WARN: Could not find comment preload bundle in ${path.dirname(commentPreloadBundle)} — skipping annotation screenshot patch`,
    );
    return { matched: false, changed: false };
  }

  const source = fs.readFileSync(commentPreloadBundle, "utf8");
  const patchedSource = applyBrowserAnnotationScreenshotPatch(source);
  if (patchedSource !== source) {
    fs.writeFileSync(commentPreloadBundle, patchedSource, "utf8");
    return { matched: true, changed: true };
  }
  return { matched: true, changed: false };
}

module.exports = {
  applyBrowserAnnotationScreenshotPatch,
  applyLinuxAppSunsetPatch,
  applyLinuxOpaqueWindowsDefaultPatch,
  applyLinuxRemoteControlConfigGatePatch,
  patchCommentPreloadBundle,
};
