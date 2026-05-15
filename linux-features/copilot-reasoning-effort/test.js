#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");
const {
  applyWebviewAssetPatchDescriptors,
  normalizePatchDescriptors,
} = require("../../scripts/patches/engine.js");
const {
  loadLinuxFeaturePatchDescriptors,
} = require("../../scripts/lib/linux-features.js");
const {
  applyCopilotReasoningEffortModelListPatch,
  applyCopilotReasoningEffortSettingsPatch,
  applyCopilotReasoningEffortUiPatch,
} = require("./patch.js");

function applyPatchTwice(patchFn, source) {
  const patched = patchFn(source);
  assert.equal(patchFn(patched), patched);
  return patched;
}

function copilotReasoningEffortSettingsFixture() {
  return [
    "function bwe(){let e=(0,Y.c)(3),t=wr(),{data:n,isLoading:r}=or(`copilot-default-model`),i=n??t.defaultModel,a;return e[0]!==r||e[1]!==i?(a={model:i,reasoningEffort:`medium`,profile:null,isLoading:r},e[0]=r,e[1]=i,e[2]=a):a=e[2],a}",
    "function $9(e=null){let t=j(fe),m=a?.authMethod===`copilot`,g=(0,q.useCallback)(async(t,n)=>!1,[]),c={profile:null},i=!0,r=`local`,s=`/tmp`,v=()=>{},y=()=>{};return{setModelAndReasoningEffort:(0,q.useCallback)(async(e,n)=>{try{if(await g(e,n))return;if(m){Jn(t,`copilot-default-model`,e);return}if(h.info(`Setting default model and reasoning effort`,{safe:{newModel:e,newEffort:n,profile:c.profile}}),!i)return;await Gt(`set-default-model-config-for-host`,{hostId:r,model:e,reasoningEffort:n,profile:c.profile}),await v(),await t.query.fetch(Ss,{hostId:r,cwd:s})}catch(e){y(e)}},[m,g,c.profile,v,i,r,t,y,s])}}",
  ].join("");
}

function copilotReasoningEffortModelListFixture() {
  return "function Ge(){let s=`copilot`,d={};return e.forEach(e=>{let t=s===`copilot`?[e.supportedReasoningEfforts.find(Ue)??{reasoningEffort:`medium`,description:`medium effort`}]:[...e.supportedReasoningEfforts];d.models.push({...e,supportedReasoningEfforts:t})})}";
}

function currentCopilotReasoningEffortModelListFixture() {
  return "function Ge(){let t=`copilot`;return r.forEach(e=>{let n=t===`copilot`?[e.supportedReasoningEfforts.find(e=>e.reasoningEffort===`medium`)??{reasoningEffort:`medium`,description:`medium effort`}]:[...e.supportedReasoningEfforts];i.push({...e,supportedReasoningEfforts:n})})}";
}

function copilotReasoningEffortUiFixture() {
  return [
    "function qU(){let E=o?.authMethod===`copilot`,D=ZH(T,f.model),O=QH(f.reasoningEffort,D),le=D.map(e=>{let{reasoningEffort:t}=e;return(0,$.jsx)(jm.Item,{\"data-reasoning-selected\":t===O?`true`:void 0,disabled:E,RightIcon:t===O?rg:void 0,onSelect:()=>{i.get(bh).log({eventName:`codex_composer_reasoning_effort_changed`,metadata:{reasoning_effort:t}}),p(f.model,t),H()},children:(0,$.jsx)(nM,{effort:t})},t)})}",
    "function bY(e){let p=o?.authMethod===`copilot`;let w=s&&f&&!p,T;return{enabled:w,dependencies:T}}",
  ].join("");
}

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codex-copilot-reasoning-feature-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function withTempFeatureConfig(enabled, fn) {
  const originalConfig = process.env.CODEX_LINUX_FEATURES_CONFIG;
  return withTempDir((tmp) => {
    process.env.CODEX_LINUX_FEATURES_CONFIG = path.join(tmp, "features.json");
    fs.writeFileSync(process.env.CODEX_LINUX_FEATURES_CONFIG, JSON.stringify({ enabled }, null, 2));
    try {
      return fn();
    } finally {
      if (originalConfig == null) {
        delete process.env.CODEX_LINUX_FEATURES_CONFIG;
      } else {
        process.env.CODEX_LINUX_FEATURES_CONFIG = originalConfig;
      }
    }
  });
}

function writeAsset(extractedDir, name, source) {
  const assetsDir = path.join(extractedDir, "webview", "assets");
  fs.mkdirSync(assetsDir, { recursive: true });
  fs.writeFileSync(path.join(assetsDir, name), source);
}

function readAsset(extractedDir, name) {
  return fs.readFileSync(path.join(extractedDir, "webview", "assets", name), "utf8");
}

test("persists Copilot reasoning effort with the default Copilot model", () => {
  const patched = applyPatchTwice(
    applyCopilotReasoningEffortSettingsPatch,
    copilotReasoningEffortSettingsFixture(),
  );

  assert.match(patched, /or\(`copilot-default-reasoning-effort`\)/);
  assert.match(patched, /reasoningEffort:codexCopilotReasoningEffortValue/);
  assert.match(patched, /isLoading:r\|\|codexCopilotReasoningEffortLoading/);
  assert.match(
    patched,
    /Jn\(t,`copilot-default-model`,e\),Jn\(t,`copilot-default-reasoning-effort`,n\);return/,
  );
  assert.doesNotMatch(patched, /reasoningEffort:`medium`,profile:null,isLoading:r/);
  assert.doesNotMatch(patched, /Jn\(t,`copilot-default-model`,e\);return/);
});

test("keeps all model reasoning efforts available for Copilot auth", () => {
  const patched = applyPatchTwice(
    applyCopilotReasoningEffortModelListPatch,
    copilotReasoningEffortModelListFixture(),
  );

  assert.match(patched, /let t=\[\.\.\.e\.supportedReasoningEfforts\]/);
  assert.doesNotMatch(patched, /s===`copilot`\?\[/);
  assert.doesNotMatch(patched, /description:`medium effort`/);
});

test("keeps all model reasoning efforts for current Copilot model query chunks", () => {
  const patched = applyPatchTwice(
    applyCopilotReasoningEffortModelListPatch,
    currentCopilotReasoningEffortModelListFixture(),
  );

  assert.match(patched, /let n=\[\.\.\.e\.supportedReasoningEfforts\]/);
  assert.doesNotMatch(patched, /t===`copilot`\?\[/);
  assert.doesNotMatch(patched, /description:`medium effort`/);
});

test("allows Copilot auth to change reasoning effort from the UI", () => {
  const patched = applyPatchTwice(
    applyCopilotReasoningEffortUiPatch,
    copilotReasoningEffortUiFixture(),
  );

  assert.match(patched, /disabled:!1,RightIcon:t===O\?rg:void 0/);
  assert.match(patched, /let w=s&&f,T;/);
  assert.doesNotMatch(patched, /disabled:E,RightIcon:t===O\?rg:void 0/);
  assert.doesNotMatch(patched, /let w=s&&f&&!p,T;/);
});

test("feature descriptor loader exposes the Copilot webview asset patches only when enabled", () => {
  const featuresRoot = path.resolve(__dirname, "..");

  withTempFeatureConfig([], () => {
    assert.deepEqual(loadLinuxFeaturePatchDescriptors({ featuresRoot }), []);
  });

  withTempFeatureConfig(["copilot-reasoning-effort"], () => {
    const descriptors = loadLinuxFeaturePatchDescriptors({ featuresRoot });

    assert.deepEqual(
      descriptors.map((descriptor) => descriptor.id),
      [
        "feature:copilot-reasoning-effort:settings",
        "feature:copilot-reasoning-effort:model-list",
        "feature:copilot-reasoning-effort:ui",
      ],
    );
    assert.deepEqual(
      descriptors.map((descriptor) => descriptor.phase),
      ["webview-asset", "webview-asset", "webview-asset"],
    );
    assert.ok(descriptors.every((descriptor) => descriptor.ciPolicy === "optional"));
  });
});

test("enabled feature descriptors patch matching webview assets", () => {
  const featuresRoot = path.resolve(__dirname, "..");

  withTempFeatureConfig(["copilot-reasoning-effort"], () => {
    withTempDir((extractedDir) => {
      writeAsset(extractedDir, "use-collaboration-mode-fixture.js", copilotReasoningEffortSettingsFixture());
      writeAsset(extractedDir, "model-queries-fixture.js", currentCopilotReasoningEffortModelListFixture());
      writeAsset(extractedDir, "index-fixture.js", copilotReasoningEffortUiFixture());

      const descriptors = normalizePatchDescriptors(
        loadLinuxFeaturePatchDescriptors({ featuresRoot }),
      );
      applyWebviewAssetPatchDescriptors(extractedDir, descriptors, {}, null);

      assert.match(
        readAsset(extractedDir, "use-collaboration-mode-fixture.js"),
        /copilot-default-reasoning-effort/,
      );
      assert.match(
        readAsset(extractedDir, "model-queries-fixture.js"),
        /\[\.\.\.e\.supportedReasoningEfforts\]/,
      );
      assert.match(
        readAsset(extractedDir, "index-fixture.js"),
        /disabled:!1,RightIcon:t===O\?rg:void 0/,
      );
    });
  });
});
