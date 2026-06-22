#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const {
  applyLinuxChromeExtensionStatusPatch,
} = require("./browser.js");

test("Linux Chrome extension opener searches Chrome Beta and Unstable commands", () => {
  const source =
    'const fs=require("node:fs"),os=require("node:os"),path=require("node:path");' +
    "function validate(e){return e}" +
    "function profileDir({homeDir:e,localAppDataDir:t,platform:r}){return `/profile`}" +
    "function detect(){return null}" +
    "function run(){}" +
    "function extensionUrl(e){return `chrome://extensions/?id=${e}`}" +
    "const openCommand=`open`,bundleId=`com.google.Chrome`;" +
    "function status({extensionId:e,homeDir:t=os.homedir(),localAppDataDir:r=process.env.LOCALAPPDATA,platform:n=process.platform}){let o=validate(e),i=profileDir({homeDir:t,localAppDataDir:r,platform:n});return i==null||!fs.existsSync(i)?!1:fs.readdirSync(i,{withFileTypes:!0}).some(s=>s.isDirectory()&&fs.existsSync(path.join(i,s.name,`Extensions`,o)))}" +
    "async function openExtension({extensionId:e,platform:t=process.platform,detectChromeCommand:r=detect,runCommand:n=run}){if(t===`darwin`){await n(openCommand,[`-b`,bundleId,extensionUrl(e)]);return}if(t===`win32`){let o=r();if(o==null)throw Error(`Google Chrome is not installed`);await n(o,[extensionUrl(e)]);return}throw new Error(`Opening Chrome extension settings is only supported on macOS and Windows`)}" +
    "function nextHelper(){}";

  const patched = applyLinuxChromeExtensionStatusPatch(source);

  assert.match(
    patched,
    /`google-chrome-stable`,`google-chrome-beta`,`google-chrome-unstable`,`chromium-browser`/,
  );
  assert.match(patched, /`google-chrome-beta`/);
  assert.match(patched, /`google-chrome-unstable`/);
});
