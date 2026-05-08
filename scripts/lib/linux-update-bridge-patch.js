const fs = require("fs");
const path = require("path");

function requireName(source, moduleName) {
  const escaped = moduleName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return source.match(new RegExp(`([A-Za-z_$][\\w$]*)=require\\([\\\`'"]${escaped}[\\\`'"]\\)`))?.[1] ?? null;
}

function buildInstallAfterQuitSource(childProcessVar) {
  return `function codexLinuxInstallAfterQuit(){try{let e=${childProcessVar}.spawn(\`/bin/sh\`,[\`-c\`,\`for i in 1 2 3 4 5 6 7 8 9 10;do sleep 1;s="$("$1" status 2>/dev/null||true)";echo "$s"|grep -q "^status: WaitingForAppExit"&&continue;echo "$s"|grep -q "^status: Installing"&&continue;"$1" install-ready||exit $?;s="$("$1" status 2>/dev/null||true)";echo "$s"|grep -q "^status: WaitingForAppExit"&&continue;echo "$s"|grep -q "^status: Installing"&&continue;if echo "$s"|grep -q "^status: Installed";then (/usr/bin/codex-desktop >/dev/null 2>&1 &);fi;exit 0;done\`,\`codex-linux-update-install\`,codexLinuxUpdateManagerPath()],{detached:!0,stdio:\`ignore\`,windowsHide:!0});e.unref?.()}catch{}}`;
}

function replaceInstallAfterQuitSource(source, childProcessVar) {
  const pattern =
    /function codexLinuxInstallAfterQuit\(\)\{try\{let e=[A-Za-z_$][\w$]*\.spawn\(`\/bin\/sh`,\[`-c`,[^]*?e\.unref\?\.\(\)\}catch\{\}\}/;
  return source.replace(pattern, buildInstallAfterQuitSource(childProcessVar));
}

function buildQuitForUpdateSource(electronVar, callInstallAfterQuit) {
  if (electronVar == null) {
    return "function codexLinuxQuitForUpdate(){codexLinuxInstallAfterQuit()}";
  }
  const prefix = callInstallAfterQuit ? "codexLinuxInstallAfterQuit();" : "";
  return `function codexLinuxQuitForUpdate(){try{${prefix}let e=setTimeout(()=>${electronVar}.app?.exit?.(0),1500);e.unref?.(),${electronVar}.app?.quit?.()}catch{}}`;
}

function buildBridgeSource({ childProcessVar, electronVar, fsVar, pathVar }) {
  const showUpdateMessage =
    electronVar == null
      ? "async function codexLinuxShowUpdateMessage(){}"
      : `async function codexLinuxShowUpdateMessage(e,n){try{await ${electronVar}.dialog?.showMessageBox({type:\`info\`,buttons:[\`OK\`],defaultId:0,noLink:!0,message:e,detail:n})}catch{}}`;
  const installAfterQuit = buildInstallAfterQuitSource(childProcessVar);
  const quitForUpdate = buildQuitForUpdateSource(electronVar, true);
  return `function codexLinuxUpdateStatePath(){let e=process.env.XDG_STATE_HOME||process.env.HOME&&(0,${pathVar}.join)(process.env.HOME,\`.local\`,\`state\`);return e?(0,${pathVar}.join)(e,\`codex-update-manager\`,\`state.json\`):null}function codexLinuxReadUpdateState(){let e=codexLinuxUpdateStatePath();if(!e||!${fsVar}.existsSync(e))return null;try{let t=JSON.parse(${fsVar}.readFileSync(e,\`utf8\`));return t&&typeof t===\`object\`&&!Array.isArray(t)?t:null}catch{return null}}function codexLinuxUpdateLifecycleState(e){switch(e){case\`ready_to_install\`:case\`waiting_for_app_exit\`:return\`ready\`;case\`installing\`:return\`installing\`;case\`checking_upstream\`:case\`update_detected\`:case\`downloading_dmg\`:case\`preparing_workspace\`:case\`patching_app\`:case\`building_package\`:return\`checking\`;default:return\`idle\`}}function codexLinuxUpdateManagerPath(){let e=process.env.CODEX_UPDATE_MANAGER_PATH;return typeof e===\`string\`&&e.trim().length>0?e:\`codex-update-manager\`}${showUpdateMessage}${installAfterQuit}${quitForUpdate}function codexLinuxRunUpdateManager(e){return new Promise((t,n)=>{${childProcessVar}.execFile(codexLinuxUpdateManagerPath(),e,{encoding:\`utf8\`,windowsHide:!0},(e,r,i)=>{if(e){e.stdout=r,e.stderr=i,n(e);return}t({stdout:r??\`\`,stderr:i??\`\`})})})}`;
}

function applyLinuxAppUpdaterBridgePatch(currentSource) {
  if (!currentSource.includes("var tD=class{") || !currentSource.includes("initializeMacSparkle")) {
    return currentSource;
  }

  const childProcessVar =
    requireName(currentSource, "node:child_process") ?? requireName(currentSource, "child_process");
  const electronVar = requireName(currentSource, "electron") ?? requireName(currentSource, "node:electron");
  const fsVar = requireName(currentSource, "node:fs") ?? requireName(currentSource, "fs");
  const pathVar = requireName(currentSource, "node:path") ?? requireName(currentSource, "path");
  if (childProcessVar == null || fsVar == null || pathVar == null) {
    console.warn("WARN: Could not find updater bridge module bindings - skipping Linux updater bridge patch");
    return currentSource;
  }

  let patchedSource = currentSource;
  if (!patchedSource.includes("function codexLinuxUpdateLifecycleState(")) {
    const classNeedle = "var tD=class{";
    patchedSource = patchedSource.replace(
      classNeedle,
      `${buildBridgeSource({ childProcessVar, electronVar, fsVar, pathVar })};${classNeedle}`,
    );
  }
  if (!patchedSource.includes("function codexLinuxQuitForUpdate(")) {
    const quitSource = `${buildInstallAfterQuitSource(childProcessVar)}${buildQuitForUpdateSource(electronVar, true)}`;
    const runManagerNeedle = "function codexLinuxRunUpdateManager(";
    if (patchedSource.includes(runManagerNeedle)) {
      patchedSource = patchedSource.replace(runManagerNeedle, `${quitSource}${runManagerNeedle}`);
    }
  } else {
    if (!patchedSource.includes("function codexLinuxInstallAfterQuit(")) {
      patchedSource = patchedSource.replace(
        "function codexLinuxQuitForUpdate(",
        `${buildInstallAfterQuitSource(childProcessVar)}function codexLinuxQuitForUpdate(`,
      );
    }
    if (electronVar != null) {
      patchedSource = patchedSource.replace(
        buildQuitForUpdateSource(electronVar, false),
        buildQuitForUpdateSource(electronVar, true),
      );
    }
  }
  if (patchedSource.includes("function codexLinuxInstallAfterQuit(")) {
    patchedSource = replaceInstallAfterQuitSource(patchedSource, childProcessVar);
  }
  patchedSource = patchedSource.replace(
    "this.setInstallProgressPercent(null),this.options.onInstallUpdatesRequested?.();return",
    "this.setInstallProgressPercent(null),codexLinuxQuitForUpdate();return",
  );

  const initializeNeedle =
    "if(process.platform===`win32`?await this.initializeWindowsUpdater():await this.initializeMacSparkle(),t.ipcMain.handle(";
  const initializePatch =
    "if(process.platform===`linux`?await this.initializeLinuxPackageUpdater():process.platform===`win32`?await this.initializeWindowsUpdater():await this.initializeMacSparkle(),t.ipcMain.handle(";
  if (patchedSource.includes(initializePatch)) {
    // Already patched.
  } else if (patchedSource.includes(initializeNeedle)) {
    patchedSource = patchedSource.replace(initializeNeedle, initializePatch);
  } else {
    console.warn("WARN: Could not find updater initialize platform branch - skipping Linux updater bridge patch");
    return currentSource;
  }

  const disabledGateNeedle = "if(!this.options.enableUpdater){this.lastUnavailableReason=process.platform!==`darwin`&&process.platform!==`win32`?";
  const disabledGatePatch = "if(!this.options.enableUpdater&&process.platform!==`linux`){this.lastUnavailableReason=process.platform!==`darwin`&&process.platform!==`win32`?";
  if (patchedSource.includes(disabledGatePatch)) {
    // Already patched.
  } else if (patchedSource.includes(disabledGateNeedle)) {
    patchedSource = patchedSource.replace(disabledGateNeedle, disabledGatePatch);
  } else {
    console.warn("WARN: Could not find updater enable gate - skipping Linux updater enable patch");
    return currentSource;
  }

  if (!patchedSource.includes("async initializeLinuxPackageUpdater(){")) {
    const methodNeedle = "async initializeWindowsUpdater(){";
    const methodPatch =
      "async initializeLinuxPackageUpdater(){if(process.platform!==`linux`){this.lastUnavailableReason=`unsupported platform`;return}let e=()=>{let e=codexLinuxReadUpdateState(),t=e?.status;this.setUpdateReady(t===`ready_to_install`||t===`waiting_for_app_exit`),this.setUpdateLifecycleState(codexLinuxUpdateLifecycleState(t)),this.lastUnavailableReason=null;return e};try{await codexLinuxRunUpdateManager([`--help`]),e()}catch(e){this.lastUnavailableReason=e?.code===`ENOENT`?`codex-update-manager not found`:`codex-update-manager unavailable`,ZE().warning(`Linux updater unavailable`,{safe:{reason:this.lastUnavailableReason},sensitive:{error:e}});return}this.updater={checkForUpdates:async()=>{this.setUpdateLifecycleState(`checking`);try{await codexLinuxRunUpdateManager([`check-now`]),e()}catch(t){this.setUpdateLifecycleState(this.isUpdateReady?`ready`:`idle`);throw t}},installUpdatesIfAvailable:async()=>{e();if(!this.isUpdateReady)return;this.setInstallProgressPercent(0),this.setUpdateLifecycleState(`installing`);try{let n=await codexLinuxRunUpdateManager([`install-ready`]),t=e();if(t?.status===`waiting_for_app_exit`){this.setInstallProgressPercent(null),codexLinuxQuitForUpdate();return}this.setInstallProgressPercent(null),n.stdout?.includes(`already installed`)?await codexLinuxShowUpdateMessage(`Codex Desktop update`,`The ready update is already installed.`):n.stdout?.includes(`No Codex Desktop update is ready`)&&await codexLinuxShowUpdateMessage(`Codex Desktop update`,`There is no rebuilt update waiting to install.`)}catch(e){this.setInstallProgressPercent(null),this.setUpdateLifecycleState(this.isUpdateReady?`ready`:`idle`);throw e}}};let t=setInterval(()=>{try{e()}catch(e){ZE().warning(`Linux updater state refresh failed`,{safe:{},sensitive:{error:e}})}},3e4);t.unref?.()}";
    if (!patchedSource.includes(methodNeedle)) {
      console.warn("WARN: Could not find updater method insertion point - skipping Linux updater bridge patch");
      return currentSource;
    }
    patchedSource = patchedSource.replace(methodNeedle, `${methodPatch}${methodNeedle}`);
  }

  return patchedSource;
}

function applyLinuxAppUpdaterMenuPatch(currentSource) {
  const menuNeedle = "d=t.C.shouldIncludeSparkle(a,process.platform,process.env)";
  const menuPatch = "d=t.C.shouldIncludeSparkle(a,process.platform,process.env)||process.platform===`linux`";

  if (currentSource.includes(menuPatch)) {
    return currentSource;
  }
  if (!currentSource.includes(menuNeedle)) {
    if (currentSource.includes("enableSparkle") && currentSource.includes("shouldIncludeSparkle")) {
      console.warn("WARN: Could not find update menu feature gate - skipping Linux update menu patch");
    }
    return currentSource;
  }
  return currentSource.replace(menuNeedle, menuPatch);
}

function patchLinuxAppUpdaterBridge(extractedDir) {
  const buildDir = path.join(extractedDir, ".vite", "build");
  if (!fs.existsSync(buildDir)) {
    console.warn(`WARN: Could not find build directory in ${buildDir} - skipping Linux updater bridge patch`);
    return { matched: 0, changed: 0 };
  }

  let matched = 0;
  let changed = 0;
  for (const fileName of fs.readdirSync(buildDir).filter((name) => name.endsWith(".js")).sort()) {
    const filePath = path.join(buildDir, fileName);
    const source = fs.readFileSync(filePath, "utf8");
    if (!source.includes("var tD=class{") && !source.includes("shouldIncludeSparkle")) {
      continue;
    }
    matched += 1;
    const patched = applyLinuxAppUpdaterBridgePatch(applyLinuxAppUpdaterMenuPatch(source));
    if (patched !== source) {
      fs.writeFileSync(filePath, patched, "utf8");
      changed += 1;
    }
  }

  return { matched, changed };
}

module.exports = {
  applyLinuxAppUpdaterBridgePatch,
  applyLinuxAppUpdaterMenuPatch,
  patchLinuxAppUpdaterBridge,
};
