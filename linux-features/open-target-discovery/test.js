#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const { EventEmitter } = require("node:events");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const { pathToFileURL } = require("node:url");
const { applyMainBundlePatch } = require("./patch.js");
const {
  enabledLinuxFeatureIds,
  loadLinuxFeatureMainBundlePatches,
} = require("../../scripts/lib/linux-features.js");
const {
  createPatchReport,
  patchExtractedApp,
  patchMainBundleSource,
} = require("../../scripts/patch-linux-window-ui.js");

const mainBundlePrefix =
  "let n=require(`electron`),i=require(`node:path`),o=require(`node:fs`),u=require(`node:child_process`);";
const fileManagerBundle =
  "function jl(e){return e}function il(e){return [e]}var lu=jl({id:`fileManager`,label:`Finder`,icon:`apps/finder.png`,kind:`fileManager`,darwin:{detect:()=>`open`,args:e=>il(e)},win32:{label:`File Explorer`,icon:`apps/file-explorer.png`,detect:uu,args:e=>il(e),open:async({path:e})=>du(e)}});function uu(){}";
const terminalOpenTargetBundle =
  "var uh={id:`terminal`,platforms:{darwin:{label:`Terminal`,icon:`apps/terminal.png`,kind:`terminal`,detect:()=>`open`,args:e=>[`-a`,`Terminal`,e]},win32:{label:`Terminal`,icon:`apps/microsoft-terminal.png`,kind:`terminal`,detect:vh,iconPath:()=>null,args:yh,open:({command:e,path:t})=>bh(e,yh(t))}}};function vh(){return `wt.exe`}function yh(e){return[`-d`,e]}async function bh(){}";
const ideOpenTargetsBundle =
  "function ih({id:e,label:t,icon:n,darwinDetect:r,win32Detect:i,darwinEnv:a,darwinArgs:o,hidden:s}){return{id:e,platforms:{darwin:r?{label:t,icon:n,kind:`editor`,hidden:s,detect:r,env:a,args:o??ah,supportsSsh:!0}:void 0,win32:i?{label:t,icon:n,kind:`editor`,hidden:s,detect:i,args:ah,supportsSsh:!0}:void 0}}}var ah=(e,t)=>t?[`${e}:${t.line}:${t.column}`]:[e];var Og=ih({id:`vscode`,label:`VS Code`,icon:`apps/vscode.png`,darwinDetect:()=>`open`,win32Detect:()=>`Code.exe`});var jh=ih({id:`cursor`,label:`Cursor`,icon:`apps/cursor.png`,darwinDetect:()=>`open`,win32Detect:()=>`Cursor.exe`});function sg({id:e,label:t,icon:n,toolboxTarget:r,macExecutable:i,windowsPathCommands:a,windowsInstallDirPrefixes:o,windowsInstallExecutables:s}){return{id:e,platforms:{darwin:{label:t,icon:n,kind:`editor`,detect:()=>`open`,args:mg},win32:a&&o&&s?{label:t,icon:n,kind:`editor`,detect:()=>`idea.exe`,args:mg}:void 0}}}function mg(e,t){return t?[`--line`,t.line.toString(),`--column`,t.column.toString(),e]:[e]}var $h=sg({id:`intellij`,label:`IntelliJ IDEA`,icon:`apps/intellij.png`,toolboxTarget:`intellij`,macExecutable:`idea`,windowsPathCommands:[`idea`],windowsInstallDirPrefixes:[`idea`],windowsInstallExecutables:[`idea`]});var Wg={id:`zed`,platforms:{darwin:{label:`Zed`,icon:`apps/zed.png`,kind:`editor`,detect:Gg,args:hg},win32:{label:`Zed`,icon:`apps/zed.png`,kind:`editor`,detect:Kg,args:hg}}};function Gg(){}function Kg(){}function hg(e,t){return t?[`${e}:${t.line}:${t.column}`]:[e]}var Xg=[Og,jh,Wg,$h];";
const openTargetsBundle = `${mainBundlePrefix}${fileManagerBundle}${terminalOpenTargetBundle}${ideOpenTargetsBundle}`;

function applyPatchTwice(patchFn, source, ...args) {
  const patched = patchFn(source, ...args);
  assert.equal(patchFn(patched, ...args), patched);
  return patched;
}

function captureWarns(fn) {
  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (...args) => {
    warnings.push(args.map(String).join(" "));
  };
  try {
    return { value: fn(), warnings };
  } finally {
    console.warn = originalWarn;
  }
}

function makeExecutable(dir, name) {
  const file = path.join(dir, name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, "#!/bin/sh\nexit 0\n");
  fs.chmodSync(file, 0o755);
  return file;
}

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-open-target-feature-"));
  let cleanup = true;
  try {
    const result = fn(dir);
    if (result && typeof result.then === "function") {
      cleanup = false;
      return result.finally(() => fs.rmSync(dir, { recursive: true, force: true }));
    }
    return result;
  } finally {
    if (cleanup) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
}

function createSpawnRecorder() {
  const calls = [];
  return {
    calls,
    spawn(command, args) {
      calls.push({ command, args });
      const child = new EventEmitter();
      child.unref = () => {};
      process.nextTick(() => child.emit("close", 0));
      return child;
    },
  };
}

function requireStub(spawnRecorder = createSpawnRecorder(), openPathCalls = []) {
  return (name) => {
    if (name === "node:fs") return fs;
    if (name === "node:path") return path;
    if (name === "node:url") return { pathToFileURL };
    if (name === "node:child_process") return spawnRecorder;
    if (name === "electron") {
      return {
        shell: {
          openPath: async (target) => {
            openPathCalls.push(target);
            return "";
          },
        },
      };
    }
    return require(name);
  };
}

function evaluatePatched(source, env, expression, spawnRecorder, openPathCalls) {
  const patched = applyPatchTwice(applyMainBundlePatch, source);
  assert.doesNotThrow(() => new Function("require", "process", `${patched};return ${expression};`));
  return new Function("require", "process", `${patched};return ${expression};`)(
    requireStub(spawnRecorder, openPathCalls),
    { platform: "linux", env },
  );
}

function withTempFeatureConfig(enabled, fn) {
  const originalConfig = process.env.CODEX_LINUX_FEATURES_CONFIG;
  const root = path.resolve(__dirname, "..");
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-open-target-config-"));
  process.env.CODEX_LINUX_FEATURES_CONFIG = path.join(tempDir, "features.json");
  try {
    fs.writeFileSync(process.env.CODEX_LINUX_FEATURES_CONFIG, JSON.stringify({ enabled }, null, 2));
    return fn(root);
  } finally {
    if (originalConfig == null) {
      delete process.env.CODEX_LINUX_FEATURES_CONFIG;
    } else {
      process.env.CODEX_LINUX_FEATURES_CONFIG = originalConfig;
    }
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

function withLinuxFeatureRootEnv(root, fn) {
  const originalRoot = process.env.CODEX_LINUX_FEATURES_ROOT;
  process.env.CODEX_LINUX_FEATURES_ROOT = root;
  try {
    return fn();
  } finally {
    if (originalRoot == null) {
      delete process.env.CODEX_LINUX_FEATURES_ROOT;
    } else {
      process.env.CODEX_LINUX_FEATURES_ROOT = originalRoot;
    }
  }
}

test("open-target discovery directly adds file manager, terminal, and IDE support", () => {
  const patched = applyPatchTwice(applyMainBundlePatch, openTargetsBundle);

  assert.match(patched, /codexLinuxOpenFileManager\(e\)/);
  assert.match(patched, /linux:\{label:`Terminal`/);
  assert.match(patched, /linux:codexLinuxIdePlatform\(/);
  assert.match(patched, /linux:codexLinuxJetBrainsIdePlatform\(/);
  assert.match(patched, /\.\.\.codexLinuxDiscoveredIdeTargets\(\)/);
});

test("open-target discovery prefers xdg-terminal-exec for Terminal", () => {
  withTempDir((tmp) => {
    const binDir = path.join(tmp, "bin");
    const xdgTerminal = makeExecutable(binDir, "xdg-terminal-exec");
    const terminal = evaluatePatched(
      openTargetsBundle,
      { HOME: tmp, PATH: binDir },
      "uh.platforms.linux",
    );

    assert.equal(terminal.detect(), xdgTerminal);
    assert.deepEqual(terminal.args(tmp), []);
  });
});

test("open-target discovery finds terminal emulators from desktop entries", () => {
  withTempDir((tmp) => {
    const dataHome = path.join(tmp, "share");
    const appsDir = path.join(dataHome, "applications");
    const terminalCommand = makeExecutable(path.join(tmp, "terminal", "bin"), "toolbox-terminal");
    fs.mkdirSync(appsDir, { recursive: true });
    fs.writeFileSync(
      path.join(appsDir, "org.example.Terminal.desktop"),
      [
        "[Desktop Entry]",
        "Type=Application",
        "Name=Toolbox Terminal",
        `Exec=${terminalCommand} --new-window %U`,
        "Categories=System;TerminalEmulator;",
        "X-TerminalArgDir=--cwd=",
      ].join("\n"),
    );

    const terminal = evaluatePatched(
      openTargetsBundle,
      { HOME: tmp, PATH: path.join(tmp, "bin"), XDG_DATA_HOME: dataHome, XDG_DATA_DIRS: path.join(tmp, "empty") },
      "uh.platforms.linux",
    );

    assert.equal(terminal.detect(), terminalCommand);
    assert.deepEqual(terminal.args(tmp), ["--new-window", `--cwd=${tmp}`]);
  });
});

test("open-target discovery finds IDEs from desktop entries", () => {
  withTempDir((tmp) => {
    const dataHome = path.join(tmp, "share");
    const appsDir = path.join(dataHome, "applications");
    const editorCommand = makeExecutable(path.join(tmp, "toolbox", "bin"), "fleet");
    const projectFile = path.join(tmp, "project", "src", "main.rs");
    fs.mkdirSync(appsDir, { recursive: true });
    fs.mkdirSync(path.dirname(projectFile), { recursive: true });
    fs.writeFileSync(projectFile, "");
    fs.writeFileSync(
      path.join(appsDir, "com.example.Fleet.desktop"),
      [
        "[Desktop Entry]",
        "Type=Application",
        "Name=Fleet IDE",
        `Exec=${editorCommand} --goto %f`,
        "Categories=Development;IDE;",
      ].join("\n"),
    );

    const targets = evaluatePatched(
      openTargetsBundle,
      { HOME: tmp, PATH: path.join(tmp, "bin"), XDG_DATA_HOME: dataHome, XDG_DATA_DIRS: path.join(tmp, "empty") },
      "Xg.flatMap((target)=>{let platform=target.platforms.linux;return platform?[{id:target.id,label:platform.label,command:platform.detect?.(),args:platform.args}]:[]})",
    );
    const fleet = targets.find((target) => target.label === "Fleet IDE");

    assert.ok(fleet);
    assert.equal(fleet.command, editorCommand);
    assert.deepEqual(fleet.args(projectFile), ["--goto", projectFile]);
  });
});

test("open-target discovery upgrades the baseline file manager target", async () => {
  await withTempDir(async (tmp) => {
    const binDir = path.join(tmp, "bin");
    const dolphin = makeExecutable(binDir, "dolphin");
    const file = path.join(tmp, "project", "src", "main.rs");
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "");
    const spawnRecorder = createSpawnRecorder();
    const fileManager = evaluatePatched(
      openTargetsBundle,
      { HOME: tmp, PATH: binDir },
      "lu.platforms?.linux??lu.linux",
      spawnRecorder,
    );

    assert.equal(fileManager.detect(), dolphin);
    await fileManager.open({ path: file });
    assert.deepEqual(spawnRecorder.calls, [{ command: dolphin, args: ["--select", file] }]);
  });
});

test("open-target discovery stays disabled until listed in features.json", () => {
  withTempFeatureConfig([], (root) => {
    assert.deepEqual(enabledLinuxFeatureIds({ featuresRoot: root }), []);
    assert.deepEqual(loadLinuxFeatureMainBundlePatches({ featuresRoot: root }), []);

    withLinuxFeatureRootEnv(root, () => {
      const patched = captureWarns(() => patchMainBundleSource(openTargetsBundle, null)).value;
      assert.doesNotMatch(patched, /linux:\{label:`Terminal`/);
      assert.doesNotMatch(patched, /\.\.\.codexLinuxDiscoveredIdeTargets\(\)/);
      assert.doesNotMatch(patched, /codexLinuxOpenFileManager\(e\)/);
    });
  });
});

test("open-target discovery participates in feature loading and patch reports", () => {
  withTempFeatureConfig(["open-target-discovery"], (root) => {
    assert.deepEqual(enabledLinuxFeatureIds({ featuresRoot: root }), ["open-target-discovery"]);
    assert.equal(loadLinuxFeatureMainBundlePatches({ featuresRoot: root }).length, 1);

    withLinuxFeatureRootEnv(root, () => {
      const tempApp = fs.mkdtempSync(path.join(os.tmpdir(), "codex-open-target-app-"));
      try {
        const buildDir = path.join(tempApp, ".vite", "build");
        const assetsDir = path.join(tempApp, "webview", "assets");
        fs.mkdirSync(buildDir, { recursive: true });
        fs.mkdirSync(assetsDir, { recursive: true });
        fs.writeFileSync(path.join(buildDir, "main.js"), openTargetsBundle);
        fs.writeFileSync(path.join(tempApp, "package.json"), JSON.stringify({ name: "codex" }));

        const report = createPatchReport();
        captureWarns(() => patchExtractedApp(tempApp, { report }));
        const patched = fs.readFileSync(path.join(buildDir, "main.js"), "utf8");

        assert.match(patched, /linux:\{label:`Terminal`/);
        assert.match(patched, /\.\.\.codexLinuxDiscoveredIdeTargets\(\)/);
        assert.ok(
          report.patches.some((patch) => patch.name === "feature:open-target-discovery" && patch.status === "applied"),
        );
      } finally {
        fs.rmSync(tempApp, { recursive: true, force: true });
      }
    });
  });
});

test("open-target discovery does not add a second built-in Zed target", () => {
  const zedAlreadyLinux = openTargetsBundle.replace(
    "win32:{label:`Zed`,icon:`apps/zed.png`,kind:`editor`,detect:Kg,args:hg}}",
    "win32:{label:`Zed`,icon:`apps/zed.png`,kind:`editor`,detect:Kg,args:hg},linux:{label:`Zed`,icon:`apps/zed.png`,kind:`editor`,detect:Gg,args:hg}}",
  );
  const patched = applyPatchTwice(applyMainBundlePatch, zedAlreadyLinux);

  assert.equal((patched.match(/linux:\{label:`Zed`/g) || []).length, 1);
});
