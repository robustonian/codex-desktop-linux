"use strict";

const {
  CLOSE_GATE_PREFIX_LOOKBACK,
  DIRECT_HANDLER_PROXIMITY,
  HANDLER_PREFIX_LOOKBACK,
  escapeRegExp,
  findDisposableVar,
  findLastRegexMatch,
  findLinuxGlobalStateExpression,
  findMatchingBrace,
  inferModuleAlias,
  linuxSettingsKeys,
} = require("./shared.js");

// Launch-action patches keep second launches, hotkey windows, and persisted
// Linux settings coordinated with the generated launcher.
const linuxQuitStateHelpers =
  "let codexLinuxQuitInProgress=!1,codexLinuxExplicitQuitApproved=!1,codexLinuxMarkQuitInProgress=()=>{codexLinuxQuitInProgress=!0},codexLinuxPrepareForExplicitQuit=()=>{codexLinuxExplicitQuitApproved=!0,codexLinuxMarkQuitInProgress()},codexLinuxShouldBypassQuitPrompt=()=>codexLinuxExplicitQuitApproved===!0,codexLinuxIsQuitInProgress=()=>codexLinuxQuitInProgress===!0,";

function applyLinuxSettingsPersistencePatch(currentSource) {
  let patchedSource = currentSource;

  if (
    !patchedSource.includes('"set-global-state"') &&
    !patchedSource.includes(".codex-global-state.json")
  ) {
    return patchedSource;
  }

  if (!patchedSource.includes("function codexLinuxPersistSettingsState(")) {
    const stateFileRegex = /var ([A-Za-z_$][\w$]*)=`\.codex-global-state\.json`;/;
    const stateFileMatch = patchedSource.match(stateFileRegex);
    const pathVar = inferModuleAlias(patchedSource, "node:path");
    const fsVar = inferModuleAlias(patchedSource, "node:fs");
    if (stateFileMatch == null || pathVar == null || fsVar == null) {
      console.warn("WARN: Could not find Linux settings state file marker — skipping settings persistence patch");
      return patchedSource;
    }
    const stateFilePatch =
      `var ${stateFileMatch[1]}=\`.codex-global-state.json\`;function codexLinuxSettingsPath(){let e=process.env.XDG_CONFIG_HOME||process.env.HOME&&${pathVar}.join(process.env.HOME,\`.config\`);return e?${pathVar}.join(e,\`codex-desktop\`,\`settings.json\`):null}function codexLinuxReadSettingsFile(){let e=codexLinuxSettingsPath();if(!e||!${fsVar}.existsSync(e))return{};try{let t=${fsVar}.readFileSync(e,\`utf8\`),n=JSON.parse(t);return n&&typeof n===\`object\`&&!Array.isArray(n)?n:{}}catch(e){return{}}}function codexLinuxPersistSettingsState(e,t){if(process.platform!==\`linux\`||![${Object.values(linuxSettingsKeys).map((key) => `\`${key}\``).join(",")}].includes(e))return;try{let n=codexLinuxSettingsPath();if(!n)return;let r=codexLinuxReadSettingsFile();t===void 0?delete r[e]:r[e]=t,${fsVar}.mkdirSync(${pathVar}.dirname(n),{recursive:!0,mode:448}),${fsVar}.writeFileSync(n,JSON.stringify(r,null,2)+\`\\n\`,\`utf8\`)}catch(e){}}`;
    patchedSource = patchedSource.replace(stateFileRegex, stateFilePatch);
  }

  if (/"set-global-state":async\(\{key:[A-Za-z_$][\w$]*,value:[A-Za-z_$][\w$]*,origin:[A-Za-z_$][\w$]*\}\)=>\(this\.globalState\.set\([A-Za-z_$][\w$]*,[A-Za-z_$][\w$]*\),codexLinuxPersistSettingsState\(/.test(patchedSource)) {
    return patchedSource;
  }
  const setGlobalStateRegex =
    /"set-global-state":async\(\{key:([A-Za-z_$][\w$]*),value:([A-Za-z_$][\w$]*),origin:([A-Za-z_$][\w$]*)\}\)=>\(this\.globalState\.set\(\1,\2\),/;
  if (!setGlobalStateRegex.test(patchedSource)) {
    console.warn("WARN: Could not find Linux set-global-state needle — skipping settings persistence hook");
    return patchedSource;
  }

  return patchedSource.replace(
    setGlobalStateRegex,
    (_match, keyVar, valueVar, originVar) =>
      `"set-global-state":async({key:${keyVar},value:${valueVar},origin:${originVar}})=>(this.globalState.set(${keyVar},${valueVar}),codexLinuxPersistSettingsState(${keyVar},${valueVar}),`,
  );
}

function applyLinuxTrayCloseSettingPatch(currentSource) {
  let patchedSource = currentSource;

  const patchedCloseGateRegex = new RegExp(
    `canHideLastLocalWindowToTray:\\(\\)=>[A-Za-z_$][\\w$]*&&\\(process\\.platform!==\`linux\`\\|\\|[^,{}]+\\.get\\(\`${escapeRegExp(linuxSettingsKeys.systemTray)}\`\\)!==!1\\),disposables:[A-Za-z_$][\\w$]*`,
  );
  if (patchedCloseGateRegex.test(patchedSource)) {
    return patchedSource;
  }

  const closeGateRegex =
    /canHideLastLocalWindowToTray:\(\)=>([A-Za-z_$][\w$]*),disposables:([A-Za-z_$][\w$]*)/;
  const closeGateMatch = patchedSource.match(closeGateRegex);
  if (closeGateMatch != null) {
    const [, trayReadyVar, disposableVar] = closeGateMatch;
    const prefix = patchedSource.slice(
      Math.max(0, closeGateMatch.index - CLOSE_GATE_PREFIX_LOOKBACK),
      closeGateMatch.index,
    );
    const globalStateExpr = findLinuxGlobalStateExpression(prefix);
    if (globalStateExpr != null) {
      return patchedSource.replace(
        closeGateRegex,
        `canHideLastLocalWindowToTray:()=>${trayReadyVar}&&(process.platform!==\`linux\`||${globalStateExpr}.get(\`${linuxSettingsKeys.systemTray}\`)!==!1),disposables:${disposableVar}`,
      );
    }
  }

  if (patchedSource.includes("canHideLastLocalWindowToTray") && patchedSource.includes("Launching app")) {
    throw new Error("Required Linux tray settings patch failed: could not gate close-to-tray behavior");
  }

  return patchedSource;
}

function buildSemanticLinuxLaunchActionPatch({
  setterVar,
  deepLinksVar,
  fallbackFn,
  openerFn,
  windowManagerVar,
  hostExpr,
  currentWindowVar,
  createdWindowVar,
  routeVar,
  focusFn,
  notificationVar,
  globalStateExpr,
  reporterVar,
  disposableVar,
  pathVar,
  fsVar,
  netVar,
  appVar,
}) {
  const notificationPrefix = notificationVar == null
    ? ""
    : `${notificationVar}.desktopNotificationManager.dismissByNavigationPath(e),`;
  const quitState = linuxQuitStateHelpers;
  const directHandler = appVar == null
    ? ""
    : `,codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{${fallbackFn}()})},codexLinuxBeforeQuitHandler=()=>{typeof codexLinuxMarkQuitInProgress===\`function\`&&codexLinuxMarkQuitInProgress()}`;
  const startup = appVar == null
    ? `process.platform===\`linux\`&&codexLinuxStartLaunchActionSocket();${setterVar}(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{${fallbackFn}()})});`
    : `process.platform===\`linux\`&&(${appVar}.app.on(\`before-quit\`,codexLinuxBeforeQuitHandler),${disposableVar}.add(()=>{${appVar}.app.off(\`before-quit\`,codexLinuxBeforeQuitHandler)}),codexLinuxStartLaunchActionSocket(),${appVar}.app.on(\`second-instance\`,codexLinuxSecondInstanceHandler),${disposableVar}.add(()=>{${appVar}.app.off(\`second-instance\`,codexLinuxSecondInstanceHandler)}));${setterVar}(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{${fallbackFn}()})});`;

  return `${quitState}codexLinuxGetSetting=e=>process.platform!==\`linux\`||${globalStateExpr}.get(e)!==!1,codexLinuxIsTrayEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.systemTray}\`),codexLinuxIsWarmStartEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.warmStart}\`),codexLinuxIsPromptWindowEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.promptWindow}\`),${openerFn}=async(e,t)=>{${windowManagerVar}.hotkeyWindowLifecycleManager.hide();let ${currentWindowVar}=${windowManagerVar}.getPrimaryWindow(${hostExpr}),${createdWindowVar}=${currentWindowVar}??await ${windowManagerVar}.createFreshLocalWindow(e);${createdWindowVar}!=null&&(${notificationPrefix}${currentWindowVar}!=null&&t.navigateExistingWindow&&${routeVar}.navigateToRoute(${createdWindowVar},e),${focusFn}(${createdWindowVar}))},codexLinuxGetHotkeyWindowController=()=>typeof ${windowManagerVar}.hotkeyWindowLifecycleManager.ensureHotkeyWindowController===\`function\`?${windowManagerVar}.hotkeyWindowLifecycleManager.ensureHotkeyWindowController():${windowManagerVar}.hotkeyWindowLifecycleManager,codexLinuxShowHotkeyWindow=async()=>{let e=codexLinuxGetHotkeyWindowController();typeof e.openHome===\`function\`?await e.openHome():typeof e.show===\`function\`?await e.show():await ${windowManagerVar}.ensureHostWindow(${hostExpr})},codexLinuxOpenQuickChat=async()=>{${windowManagerVar}.hotkeyWindowLifecycleManager.hide();let e=${windowManagerVar}.getPrimaryWindow(${hostExpr}),t=e??await ${windowManagerVar}.createFreshLocalWindow(\`/\`);t!=null&&(${windowManagerVar}.windowManager.sendMessageToWindow(t,{type:\`new-quick-chat\`}),${focusFn}(t))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===\`string\`&&(e.startsWith(\`codex://\`)||e.startsWith(\`codex-browser-sidebar://\`))),codexLinuxHandleLaunchActionArgs=async e=>(typeof codexLinuxIsQuitInProgress===\`function\`&&codexLinuxIsQuitInProgress())?!0:codexLinuxHasDeepLink(e)&&${deepLinksVar}.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&(e.includes(\`--prompt-chat\`)||e.includes(\`--hotkey-window\`))?(codexLinuxIsPromptWindowEnabled()?(await codexLinuxShowHotkeyWindow(),!0):!1):Array.isArray(e)&&e.includes(\`--quick-chat\`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(\`--new-chat\`)?(await ${openerFn}(\`/\`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{if(typeof codexLinuxIsQuitInProgress===\`function\`&&codexLinuxIsQuitInProgress())return;codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{${reporterVar}.reportNonFatal(e instanceof Error?e:\`Failed to handle Linux launch action\`,{kind:\`linux-launch-action-failed\`}),t()})},codexLinuxPrewarmHotkeyWindow=()=>{if(!codexLinuxIsPromptWindowEnabled())return;try{let e=codexLinuxGetHotkeyWindowController();typeof e.prewarm===\`function\`&&e.prewarm()}catch(e){${reporterVar}.reportNonFatal(e instanceof Error?e:\`Failed to prewarm Linux hotkey window\`,{kind:\`linux-hotkey-window-prewarm-failed\`})}},codexLinuxStartLaunchActionSocket=()=>{let e=process.env.CODEX_DESKTOP_LAUNCH_ACTION_SOCKET?.trim();if(process.platform!==\`linux\`||!e||!codexLinuxIsWarmStartEnabled())return;try{${fsVar}.mkdirSync(${pathVar}.default.dirname(e),{recursive:!0,mode:448}),${fsVar}.rmSync(e,{force:!0});let t=${netVar}.default.createServer(t=>{let n=\`\`,r=!1,i=()=>{if(r)return;r=!0;let i=[];try{let e=JSON.parse(n.trim());Array.isArray(e.argv)&&(i=e.argv.filter(e=>typeof e===\`string\`))}catch(e){t.end?.(\`error\\n\`);return}codexLinuxHandleLaunchActionArgs(i).then(e=>e?void 0:${fallbackFn}()).then(()=>{t.end?.(\`ok\\n\`)}).catch(e=>{${reporterVar}.reportNonFatal(e instanceof Error?e:\`Failed to handle Linux launch action socket\`,{kind:\`linux-launch-action-socket-failed\`}),t.end?.(\`error\\n\`)})};t.setEncoding?.(\`utf8\`),t.on(\`data\`,e=>{n+=e,n.includes(\`\\n\`)?i():n.length>65536&&t.destroy()}),t.on(\`end\`,i)});t.on(\`error\`,e=>{${reporterVar}.reportNonFatal(e instanceof Error?e:\`Failed Linux launch action socket\`,{kind:\`linux-launch-action-socket-error\`})}),t.listen(e),${disposableVar}.add(()=>{t.close(),${fsVar}.rmSync(e,{force:!0})})}catch(e){${reporterVar}.reportNonFatal(e instanceof Error?e:\`Failed to start Linux launch action socket\`,{kind:\`linux-launch-action-socket-start-failed\`})}}${directHandler};${startup}`;
}

function applySemanticLinuxLaunchActionArgsPatch(currentSource) {
  const handlerRegex =
    /([A-Za-z_$][\w$]*)\(e=>\{([A-Za-z_$][\w$]*)\.deepLinks\.queueProcessArgs\(e\)\|\|([A-Za-z_$][\w$]*)\(\)\}\);let ([A-Za-z_$][\w$]*)=async\(e,t\)=>\{/g;
  let match;
  while ((match = handlerRegex.exec(currentSource)) != null) {
    const [, setterVar, deepLinksVar, fallbackFn, openerFn] = match;
    // handlerRegex ends with `let <openerFn>=async(e,t)=>{`, so the opening
    // brace's position is determined directly by the match.
    const openerBraceIndex = match.index + match[0].length - 1;
    const openerLetIndex = openerBraceIndex - `let ${openerFn}=async(e,t)=>`.length;
    const openerEnd = findMatchingBrace(currentSource, openerBraceIndex);
    if (openerEnd === -1) {
      continue;
    }

    const separator = currentSource[openerEnd + 1];
    if (separator !== ";" && separator !== ",") {
      continue;
    }

    const openerText = currentSource.slice(openerLetIndex, openerEnd + 1);
    const openerVars = openerText.match(
      /([A-Za-z_$][\w$]*)\.hotkeyWindowLifecycleManager\.hide\(\);let ([A-Za-z_$][\w$]*)=\1\.getPrimaryWindow\(([^)]+)\),([A-Za-z_$][\w$]*)=\2\?\?await \1\.createFreshLocalWindow\(e\);/,
    );
    if (openerVars == null) {
      continue;
    }

    const [, windowManagerVar, currentWindowVar, hostExpr, createdWindowVar] = openerVars;
    const routeVar = openerText.match(/([A-Za-z_$][\w$]*)\.navigateToRoute\([A-Za-z_$][\w$]*,e\)/)?.[1];
    const focusFn = openerText.match(new RegExp(`,([A-Za-z_$][\\w$]*)\\(${escapeRegExp(createdWindowVar)}\\)\\)\\}$`))?.[1];
    if (routeVar == null || focusFn == null) {
      continue;
    }

    const prefix = currentSource.slice(Math.max(0, match.index - HANDLER_PREFIX_LOOKBACK), match.index);
    const globalStateExpr = findLinuxGlobalStateExpression(prefix);
    const reporterVar = findLastRegexMatch(
      prefix,
      /([A-Za-z_$][\w$]*)\.reportNonFatal\(e instanceof Error\?e:`Failed to open window on second instance`/g,
    )?.[1] ?? findLastRegexMatch(prefix, /([A-Za-z_$][\w$]*)=\{reportNonFatal/g)?.[1];
    const disposableVar = findDisposableVar(prefix);
    const pathVar = inferModuleAlias(currentSource, "node:path");
    const fsVar = inferModuleAlias(currentSource, "node:fs");
    const netVar = inferModuleAlias(currentSource, "node:net");
    if (globalStateExpr == null || reporterVar == null || disposableVar == null || pathVar == null || fsVar == null || netVar == null) {
      continue;
    }

    let replaceStart = match.index;
    let appVar = null;
    const directStart = currentSource.lastIndexOf("let codexLinuxSecondInstanceHandler=", match.index);
    if (directStart !== -1 && match.index - directStart < DIRECT_HANDLER_PROXIMITY) {
      const directBlock = currentSource.slice(directStart, match.index);
      const appMatch = directBlock.match(/([A-Za-z_$][\w$]*)\.app\.on\(`second-instance`,codexLinuxSecondInstanceHandler\)/);
      replaceStart = directStart;
      appVar = appMatch?.[1] ?? inferModuleAlias(currentSource, "electron");
    }

    const notificationVar = openerText.match(
      /([A-Za-z_$][\w$]*)\.desktopNotificationManager\.dismissByNavigationPath\(e\)/,
    )?.[1] ?? null;
    const replacement = buildSemanticLinuxLaunchActionPatch({
      setterVar,
      deepLinksVar,
      fallbackFn,
      openerFn,
      windowManagerVar,
      hostExpr: hostExpr.trim(),
      currentWindowVar,
      createdWindowVar,
      routeVar,
      focusFn,
      notificationVar,
      globalStateExpr,
      reporterVar,
      disposableVar,
      pathVar,
      fsVar,
      netVar,
      appVar,
    });
    const suffix = separator === "," ? "let " : "";
    return currentSource.slice(0, replaceStart) + replacement + suffix + currentSource.slice(openerEnd + 2);
  }

  return currentSource;
}

function applyCurrentSemanticLinuxLaunchActionArgsPatch(currentSource) {
  const handlerRegex =
    /([A-Za-z_$][\w$]*)\(e=>\{let ([A-Za-z_$][\w$]*)=[^;{}]+;if\(([A-Za-z_$][\w$]*)\.deepLinks\.queueProcessArgs\(e\)\)\{\2&&([A-Za-z_$][\w$]*)\(\);return\}if\(\2\)\{\4\(\);return\}\4\(\)\}\);let ([A-Za-z_$][\w$]*)=async\(e,t\)=>\{/g;
  let match;
  while ((match = handlerRegex.exec(currentSource)) != null) {
    const [, setterVar, , deepLinksVar, fallbackFn, openerFn] = match;
    const openerBraceIndex = match.index + match[0].length - 1;
    const openerLetIndex = openerBraceIndex - `let ${openerFn}=async(e,t)=>`.length;
    const openerEnd = findMatchingBrace(currentSource, openerBraceIndex);
    if (openerEnd === -1) {
      continue;
    }

    const separator = currentSource[openerEnd + 1];
    if (separator !== ";" && separator !== ",") {
      continue;
    }

    const openerText = currentSource.slice(openerLetIndex, openerEnd + 1);
    const openerVars = openerText.match(
      /([A-Za-z_$][\w$]*)\.hotkeyWindowLifecycleManager\.hide\(\);let ([A-Za-z_$][\w$]*)=\1\.getPrimaryWindow\(([^)]+)\),([A-Za-z_$][\w$]*)=\2\?\?await \1\.createFreshLocalWindow\(e\);/,
    );
    if (openerVars == null) {
      continue;
    }

    const [, windowManagerVar, currentWindowVar, hostExpr, createdWindowVar] = openerVars;
    const routeVar = openerText.match(/([A-Za-z_$][\w$]*)\.navigateToRoute\([A-Za-z_$][\w$]*,e\)/)?.[1];
    const focusFn = openerText.match(new RegExp(`,([A-Za-z_$][\\w$]*)\\(${escapeRegExp(createdWindowVar)}\\)\\)\\}$`))?.[1];
    if (routeVar == null || focusFn == null) {
      continue;
    }

    const prefix = currentSource.slice(Math.max(0, match.index - HANDLER_PREFIX_LOOKBACK), match.index);
    const globalStateExpr = findLinuxGlobalStateExpression(prefix);
    const reporterVar = findLastRegexMatch(
      prefix,
      /([A-Za-z_$][\w$]*)\.reportNonFatal\(e instanceof Error\?e:`Failed to open window on second instance`/g,
    )?.[1] ?? findLastRegexMatch(prefix, /([A-Za-z_$][\w$]*)=\{reportNonFatal/g)?.[1];
    const disposableVar = findDisposableVar(prefix);
    const pathVar = inferModuleAlias(currentSource, "node:path");
    const fsVar = inferModuleAlias(currentSource, "node:fs");
    const netVar = inferModuleAlias(currentSource, "node:net");
    if (globalStateExpr == null || reporterVar == null || disposableVar == null || pathVar == null || fsVar == null || netVar == null) {
      continue;
    }

    const notificationVar = openerText.match(
      /([A-Za-z_$][\w$]*)\.desktopNotificationManager\.dismissByNavigationPath\(e\)/,
    )?.[1] ?? null;
    const replacement = buildSemanticLinuxLaunchActionPatch({
      setterVar,
      deepLinksVar,
      fallbackFn,
      openerFn,
      windowManagerVar,
      hostExpr: hostExpr.trim(),
      currentWindowVar,
      createdWindowVar,
      routeVar,
      focusFn,
      notificationVar,
      globalStateExpr,
      reporterVar,
      disposableVar,
      pathVar,
      fsVar,
      netVar,
      appVar: null,
    });
    const suffix = separator === "," ? "let " : "";
    return currentSource.slice(0, match.index) + replacement + suffix + currentSource.slice(openerEnd + 2);
  }

  return currentSource;
}

function applyLinuxLaunchActionArgsPatch(currentSource) {
  let patchedSource = currentSource;

  const launchActionNeedle =
    "let codexLinuxSecondInstanceHandler=(e,t)=>{R.deepLinks.queueProcessArgs(t)||ie()};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{R.deepLinks.queueProcessArgs(e)||ie()});let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},oe=async()=>{";
  const oldLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHandleLaunchActionArgs=async e=>Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{R.deepLinks.queueProcessArgs(t)||ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{R.deepLinks.queueProcessArgs(e)||ie()})});let oe=async()=>{";
  const deepLinkFirstLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHandleLaunchActionArgs=async e=>Array.isArray(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const deepLinkAwareExistingWindowLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===`string`&&(e.startsWith(`codex://`)||e.startsWith(`codex-browser-sidebar://`))),codexLinuxHandleLaunchActionArgs=async e=>codexLinuxHasDeepLink(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const openHomeHotkeyWindowLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxShowHotkeyWindow=async()=>{let e=P.hotkeyWindowLifecycleManager;typeof e.openHome===`function`?await e.openHome():typeof e.show===`function`?await e.show():await P.ensureHostWindow(z)},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===`string`&&(e.startsWith(`codex://`)||e.startsWith(`codex-browser-sidebar://`))),codexLinuxHandleLaunchActionArgs=async e=>codexLinuxHasDeepLink(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&(e.includes(`--prompt-chat`)||e.includes(`--hotkey-window`))?(await codexLinuxShowHotkeyWindow(),!0):Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const socketHotkeyWindowLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxShowHotkeyWindow=async()=>{let e=P.hotkeyWindowLifecycleManager;typeof e.openHome===`function`?await e.openHome():typeof e.show===`function`?await e.show():await P.ensureHostWindow(z)},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===`string`&&(e.startsWith(`codex://`)||e.startsWith(`codex-browser-sidebar://`))),codexLinuxHandleLaunchActionArgs=async e=>codexLinuxHasDeepLink(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&(e.includes(`--prompt-chat`)||e.includes(`--hotkey-window`))?(await codexLinuxShowHotkeyWindow(),!0):Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxStartLaunchActionSocket=()=>{let e=process.env.CODEX_DESKTOP_LAUNCH_ACTION_SOCKET?.trim();if(process.platform!==`linux`||!e)return;try{o.mkdirSync(i.default.dirname(e),{recursive:!0,mode:448}),o.rmSync(e,{force:!0});let t=u.default.createServer(t=>{let n=``,r=!1,i=()=>{if(r)return;r=!0;let i=[];try{let e=JSON.parse(n.trim());Array.isArray(e.argv)&&(i=e.argv.filter(e=>typeof e===`string`))}catch(e){t.end?.(`error\\n`);return}codexLinuxHandleLaunchActionArgs(i).then(e=>e?void 0:ie()).then(()=>{t.end?.(`ok\\n`)}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action socket`,{kind:`linux-launch-action-socket-failed`}),t.end?.(`error\\n`)})};t.setEncoding?.(`utf8`),t.on(`data`,e=>{n+=e,n.includes(`\\n`)?i():n.length>65536&&t.destroy()}),t.on(`end`,i)});t.on(`error`,e=>{g.reportNonFatal(e instanceof Error?e:`Failed Linux launch action socket`,{kind:`linux-launch-action-socket-error`})}),t.listen(e),k.add(()=>{t.close(),o.rmSync(e,{force:!0})})}catch(e){g.reportNonFatal(e instanceof Error?e:`Failed to start Linux launch action socket`,{kind:`linux-launch-action-socket-start-failed`})}},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(codexLinuxStartLaunchActionSocket(),n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const hotkeyWindowLaunchActionPatch = socketHotkeyWindowLaunchActionPatch
    .replace(
      "let ae=async(e,t)=>{",
      `${linuxQuitStateHelpers}codexLinuxGetSetting=e=>process.platform!==\`linux\`||M.globalState.get(e)!==!1,codexLinuxIsTrayEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.systemTray}\`),codexLinuxIsWarmStartEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.warmStart}\`),codexLinuxIsPromptWindowEnabled=()=>codexLinuxGetSetting(\`${linuxSettingsKeys.promptWindow}\`),ae=async(e,t)=>{`,
    )
    .replace(
      "codexLinuxShowHotkeyWindow=async()=>{let e=P.hotkeyWindowLifecycleManager;typeof e.openHome===`function`?await e.openHome():typeof e.show===`function`?await e.show():await P.ensureHostWindow(z)}",
      "codexLinuxGetHotkeyWindowController=()=>typeof P.hotkeyWindowLifecycleManager.ensureHotkeyWindowController===`function`?P.hotkeyWindowLifecycleManager.ensureHotkeyWindowController():P.hotkeyWindowLifecycleManager,codexLinuxShowHotkeyWindow=async()=>{let e=codexLinuxGetHotkeyWindowController();typeof e.openHome===`function`?await e.openHome():typeof e.show===`function`?await e.show():await P.ensureHostWindow(z)}",
    )
    .replace(
      "Array.isArray(e)&&(e.includes(`--prompt-chat`)||e.includes(`--hotkey-window`))?(await codexLinuxShowHotkeyWindow(),!0)",
      "Array.isArray(e)&&(e.includes(`--prompt-chat`)||e.includes(`--hotkey-window`))?(codexLinuxIsPromptWindowEnabled()?(await codexLinuxShowHotkeyWindow(),!0):!1)",
    )
    .replace(
      "codexLinuxHandleLaunchActionArgs=async e=>",
      "codexLinuxHandleLaunchActionArgs=async e=>(typeof codexLinuxIsQuitInProgress===`function`&&codexLinuxIsQuitInProgress())?!0:",
    )
    .replace(
      "codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{",
      "codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{if(typeof codexLinuxIsQuitInProgress===`function`&&codexLinuxIsQuitInProgress())return;",
    )
    .replace(
      "if(process.platform!==`linux`||!e)return;",
      "if(process.platform!==`linux`||!e||!codexLinuxIsWarmStartEnabled())return;",
    )
    .replace(
      "codexLinuxStartLaunchActionSocket=()=>{",
      "codexLinuxPrewarmHotkeyWindow=()=>{try{let e=codexLinuxGetHotkeyWindowController();typeof e.prewarm===`function`&&e.prewarm()}catch(e){g.reportNonFatal(e instanceof Error?e:`Failed to prewarm Linux hotkey window`,{kind:`linux-hotkey-window-prewarm-failed`})}},codexLinuxStartLaunchActionSocket=()=>{",
    )
    .replace(
      "codexLinuxPrewarmHotkeyWindow=()=>{try{",
      "codexLinuxPrewarmHotkeyWindow=()=>{if(!codexLinuxIsPromptWindowEnabled())return;try{",
    )
    .replace(
      "process.platform===`linux`&&(codexLinuxStartLaunchActionSocket(),n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});",
      "let codexLinuxBeforeQuitHandler=()=>{typeof codexLinuxMarkQuitInProgress===`function`&&codexLinuxMarkQuitInProgress()};process.platform===`linux`&&(n.app.on(`before-quit`,codexLinuxBeforeQuitHandler),k.add(()=>{n.app.off(`before-quit`,codexLinuxBeforeQuitHandler)}),codexLinuxStartLaunchActionSocket(),n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});",
    );
  const showBasedHotkeyWindowLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxShowHotkeyWindow=async()=>{P.hotkeyWindowLifecycleManager.show()||await P.ensureHostWindow(z)},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=P.getPrimaryWindow(z),t=e??await P.createFreshLocalWindow(`/`);t!=null&&(P.windowManager.sendMessageToWindow(t,{type:`new-quick-chat`}),re(t))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===`string`&&(e.startsWith(`codex://`)||e.startsWith(`codex-browser-sidebar://`))),codexLinuxHandleLaunchActionArgs=async e=>codexLinuxHasDeepLink(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&(e.includes(`--prompt-chat`)||e.includes(`--hotkey-window`))?(await codexLinuxShowHotkeyWindow(),!0):Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await ae(`/`,{navigateExistingWindow:!0}),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const freshWindowLaunchActionPatch =
    "let ae=async(e,t)=>{P.hotkeyWindowLifecycleManager.hide();let n=P.getPrimaryWindow(z),r=n??await P.createFreshLocalWindow(e);r!=null&&(n!=null&&t.navigateExistingWindow&&R.navigateToRoute(r,e),re(r))},codexLinuxOpenNewChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=await P.createFreshLocalWindow(`/`);e!=null&&re(e)},codexLinuxOpenQuickChat=async()=>{P.hotkeyWindowLifecycleManager.hide();let e=await P.createFreshLocalWindow(`/`);e!=null&&(P.windowManager.sendMessageToWindow(e,{type:`new-quick-chat`}),re(e))},codexLinuxHasDeepLink=e=>Array.isArray(e)&&e.some(e=>typeof e===`string`&&(e.startsWith(`codex://`)||e.startsWith(`codex-browser-sidebar://`))),codexLinuxHandleLaunchActionArgs=async e=>codexLinuxHasDeepLink(e)&&R.deepLinks.queueProcessArgs(e)?!0:Array.isArray(e)&&e.includes(`--quick-chat`)?(await codexLinuxOpenQuickChat(),!0):Array.isArray(e)&&e.includes(`--new-chat`)?(await codexLinuxOpenNewChat(),!0):!1,codexLinuxHandleLaunchActionArgsFallback=(e,t)=>{codexLinuxHandleLaunchActionArgs(e).then(e=>{e||t()}).catch(e=>{g.reportNonFatal(e instanceof Error?e:`Failed to handle Linux launch action`,{kind:`linux-launch-action-failed`}),t()})},codexLinuxSecondInstanceHandler=(e,t)=>{codexLinuxHandleLaunchActionArgsFallback(t,()=>{ie()})};process.platform===`linux`&&(n.app.on(`second-instance`,codexLinuxSecondInstanceHandler),k.add(()=>{n.app.off(`second-instance`,codexLinuxSecondInstanceHandler)})),l(e=>{codexLinuxHandleLaunchActionArgsFallback(e,()=>{ie()})});let oe=async()=>{";
  const launchActionPatch =
    hotkeyWindowLaunchActionPatch;

  if (
    patchedSource.includes("codexLinuxQuitInProgress=!1") &&
    patchedSource.includes("codexLinuxExplicitQuitApproved=!1") &&
    patchedSource.includes("codexLinuxMarkQuitInProgress=()=>{codexLinuxQuitInProgress=!0}") &&
    patchedSource.includes("codexLinuxPrepareForExplicitQuit=()=>{codexLinuxExplicitQuitApproved=!0,codexLinuxMarkQuitInProgress()}") &&
    patchedSource.includes("codexLinuxShouldBypassQuitPrompt=()=>codexLinuxExplicitQuitApproved===!0") &&
    patchedSource.includes("codexLinuxIsQuitInProgress=()=>codexLinuxQuitInProgress===!0") &&
    patchedSource.includes("codexLinuxGetSetting=e=>") &&
    patchedSource.includes("codexLinuxGetHotkeyWindowController=()=>") &&
    patchedSource.includes("codexLinuxPrewarmHotkeyWindow=()=>") &&
    patchedSource.includes("codexLinuxStartLaunchActionSocket=()=>") &&
    (
      patchedSource.includes("n.app.on(`before-quit`,codexLinuxBeforeQuitHandler)") ||
      /process\.platform===`linux`&&codexLinuxStartLaunchActionSocket\(\);[A-Za-z_$][\w$]*\(e=>\{codexLinuxHandleLaunchActionArgsFallback\(e,\(\)=>\{[A-Za-z_$][\w$]*\(\)\}\)\}\)/.test(patchedSource)
    ) &&
    !patchedSource.includes("codexLinuxOpenNewChat")
  ) {
    return patchedSource;
  }

  // Try cheap exact-string legacy needles first; only fall through to the
  // semantic regex+capture pass if no known shape matches.
  if (patchedSource.includes(oldLaunchActionPatch)) {
    patchedSource = patchedSource.replace(oldLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(deepLinkFirstLaunchActionPatch)) {
    patchedSource = patchedSource.replace(deepLinkFirstLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(deepLinkAwareExistingWindowLaunchActionPatch)) {
    patchedSource = patchedSource.replace(deepLinkAwareExistingWindowLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(openHomeHotkeyWindowLaunchActionPatch)) {
    patchedSource = patchedSource.replace(openHomeHotkeyWindowLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(socketHotkeyWindowLaunchActionPatch)) {
    patchedSource = patchedSource.replace(socketHotkeyWindowLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(showBasedHotkeyWindowLaunchActionPatch)) {
    patchedSource = patchedSource.replace(showBasedHotkeyWindowLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(freshWindowLaunchActionPatch)) {
    patchedSource = patchedSource.replace(freshWindowLaunchActionPatch, launchActionPatch);
  } else if (patchedSource.includes(launchActionNeedle)) {
    patchedSource = patchedSource.replace(launchActionNeedle, launchActionPatch);
  } else {
    const semanticLaunchActionPatch = applySemanticLinuxLaunchActionArgsPatch(patchedSource);
    if (semanticLaunchActionPatch !== patchedSource) {
      return semanticLaunchActionPatch;
    }
    const currentSemanticLaunchActionPatch = applyCurrentSemanticLinuxLaunchActionArgsPatch(patchedSource);
    if (currentSemanticLaunchActionPatch !== patchedSource) {
      return currentSemanticLaunchActionPatch;
    }

    const existingLinuxLaunchActionBlock = patchedSource.match(
      /let ae=async\(e,t\)=>\{P\.hotkeyWindowLifecycleManager\.hide\(\);.*?;let oe=async\(\)=>\{/,
    )?.[0];
    if (existingLinuxLaunchActionBlock?.includes("codexLinuxHandleLaunchActionArgs")) {
      patchedSource = patchedSource.replace(existingLinuxLaunchActionBlock, launchActionPatch);
    } else if (
      patchedSource.includes("Launching app") &&
      patchedSource.includes("deepLinks")
    ) {
      console.warn("WARN: Could not find Linux launch action handler - skipping --new-chat/--quick-chat/--prompt-chat patch");
      return patchedSource;
    } else {
      console.warn("WARN: Could not find Linux launch action handler - skipping --new-chat/--quick-chat/--prompt-chat patch");
    }
  }

  if (patchedSource.includes("Launching app") && !patchedSource.includes("codexLinuxGetSetting=e=>")) {
    console.warn("WARN: Linux launch action patch was not settings-gated - skipping --new-chat/--quick-chat/--prompt-chat patch");
  }

  return patchedSource;
}

function applyLinuxHotkeyWindowPrewarmPatch(currentSource) {
  let patchedSource = currentSource;

  if (!patchedSource.includes("codexLinuxPrewarmHotkeyWindow=()=>")) {
    return patchedSource;
  }

  const startupPrewarmPatch =
    "process.platform===`linux`&&codexLinuxPrewarmHotkeyWindow(),A=Date.now(),await R.deepLinks.flushPendingDeepLinks()";

  if (patchedSource.includes(startupPrewarmPatch)) {
    return patchedSource;
  }

  if (
    /process\.platform===`linux`&&codexLinuxPrewarmHotkeyWindow\(\),[A-Za-z_$][\w$]*=Date\.now\(\),await [A-Za-z_$][\w$]*\.deepLinks\.flushPendingDeepLinks\(\)/.test(patchedSource)
  ) {
    return patchedSource;
  }

  const startupPrewarmNeedle =
    "w(`local window ensured`,A,{hostId:z,localWindowVisible:me?.isVisible()??!1}),A=Date.now(),await R.deepLinks.flushPendingDeepLinks()";

  if (patchedSource.includes(startupPrewarmNeedle)) {
    patchedSource = patchedSource.replace(startupPrewarmNeedle, `w(\`local window ensured\`,A,{hostId:z,localWindowVisible:me?.isVisible()??!1}),${startupPrewarmPatch}`);
  } else if (
    patchedSource.includes("process.platform===`linux`&&codexLinuxPrewarmHotkeyWindow(),A=Date.now(),await R.deepLinks.flushPendingDeepLinks()")
  ) {
    // Already patched by an older run.
  } else {
    const dynamicStartupPrewarmRegex =
      /(w\(`local window ensured`,([A-Za-z_$][\w$]*),\{hostId:([A-Za-z_$][\w$]*),localWindowVisible:[^}]+\}\),)\2=Date\.now\(\),await ([A-Za-z_$][\w$]*)\.deepLinks\.flushPendingDeepLinks\(\)/;
    const dynamicStartupPrewarmMatch = patchedSource.match(dynamicStartupPrewarmRegex);
    if (dynamicStartupPrewarmMatch != null) {
      const [, prefix, timeVar, , deepLinksVar] = dynamicStartupPrewarmMatch;
      patchedSource = patchedSource.replace(
        dynamicStartupPrewarmRegex,
        `${prefix}process.platform===\`linux\`&&codexLinuxPrewarmHotkeyWindow(),${timeVar}=Date.now(),await ${deepLinksVar}.deepLinks.flushPendingDeepLinks()`,
      );
    } else {
      console.warn("WARN: Could not find Linux hotkey window prewarm insertion point — skipping startup prewarm patch");
    }
  }

  return patchedSource;
}

module.exports = {
  applyLinuxHotkeyWindowPrewarmPatch,
  applyLinuxLaunchActionArgsPatch,
  applyLinuxSettingsPersistencePatch,
  applyLinuxTrayCloseSettingPatch,
  applySemanticLinuxLaunchActionArgsPatch,
};
