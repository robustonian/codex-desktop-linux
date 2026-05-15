"use strict";

const {
  requireName,
} = require("../../scripts/patches/shared.js");

const PATCH_NAME = "open-target-discovery feature patch";

function warn(message) {
  console.warn(`WARN: ${message} - skipping ${PATCH_NAME}`);
}

function findBalancedBlock(source, openBraceIndex) {
  if (openBraceIndex < 0 || source[openBraceIndex] !== "{") {
    return null;
  }

  let depth = 0;
  const stack = [{ type: "code" }];
  for (let index = openBraceIndex; index < source.length; index += 1) {
    const char = source[index];
    const top = stack[stack.length - 1];

    if (top.type === "string") {
      if (top.escaped) {
        top.escaped = false;
      } else if (char === "\\") {
        top.escaped = true;
      } else if (char === top.quote) {
        stack.pop();
      }
      continue;
    }

    if (top.type === "template") {
      if (top.escaped) {
        top.escaped = false;
      } else if (char === "\\") {
        top.escaped = true;
      } else if (char === "`") {
        stack.pop();
      } else if (char === "$" && source[index + 1] === "{") {
        stack.push({ type: "templateExpression", depth: 1 });
        index += 1;
      }
      continue;
    }

    if (char === '"' || char === "'") {
      stack.push({ type: "string", quote: char, escaped: false });
      continue;
    }
    if (char === "`") {
      stack.push({ type: "template", escaped: false });
      continue;
    }

    if (top.type === "templateExpression") {
      if (char === "{") {
        top.depth += 1;
      } else if (char === "}") {
        top.depth -= 1;
        if (top.depth === 0) {
          stack.pop();
        }
      }
      continue;
    }

    if (char === "{") {
      depth += 1;
    } else if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        return { start: openBraceIndex, end: index + 1, text: source.slice(openBraceIndex, index + 1) };
      }
    }
  }

  return null;
}

function findDeclarationBlock(source, marker) {
  const markerStart = source.indexOf(marker);
  if (markerStart === -1) {
    return null;
  }

  const blockStart = Math.max(
    source.lastIndexOf("var ", markerStart),
    source.lastIndexOf("let ", markerStart),
    source.lastIndexOf("const ", markerStart),
  );
  const objectStart = source.lastIndexOf("{", markerStart);
  const objectBlock = findBalancedBlock(source, objectStart);
  if (blockStart === -1 || objectBlock == null) {
    return null;
  }
  const callEndMatch = source.slice(objectBlock.end).match(/^\s*\);/u);
  const blockEnd = callEndMatch == null ? objectBlock.end : objectBlock.end + callEndMatch[0].length;

  return {
    start: blockStart,
    end: blockEnd,
    text: source.slice(blockStart, blockEnd),
  };
}

function findPropertyBlock(source, propertyName) {
  const propertyIndex = source.indexOf(`,${propertyName}:{`);
  if (propertyIndex === -1) {
    return null;
  }

  const block = findBalancedBlock(source, source.indexOf("{", propertyIndex));
  if (block == null) {
    return null;
  }

  return {
    start: propertyIndex,
    end: block.end,
    text: source.slice(propertyIndex, block.end),
  };
}

function insertOpenTargetHelpers(currentSource, insertionIndex, { fsVar, pathVar }) {
  if (currentSource.includes("function codexLinuxFindExecutable(")) {
    return currentSource;
  }

  const helpers =
    `function codexLinuxFindExecutable(e){if(process.platform!==\`linux\`||!e)return null;let t=process.env.PATH||\`\`;for(let n of t.split(\`:\`)){if(!n||!${pathVar}.isAbsolute(n))continue;let r=(0,${pathVar}.join)(n,e);try{if((0,${fsVar}.existsSync)(r)){let e=(0,${fsVar}.statSync)(r);if(e.isFile())try{(0,${fsVar}.accessSync)(r,${fsVar}.constants.X_OK);return r}catch{}}}catch{}}return null}` +
    `function codexLinuxResolveExistingTarget(e){if(typeof e!==\`string\`||e.length===0)return null;let t=e;for(;;){try{if((0,${fsVar}.existsSync)(t))return t}catch{}let n=(0,${pathVar}.dirname)(t);if(n===t)return null;t=n}}` +
    `function codexLinuxShouldDropXdgConfigHome(e){let t=e.XDG_CONFIG_HOME,n=e.CODEX_ELECTRON_USER_DATA_DIR;if(typeof t!==\`string\`)return!1;if(typeof n===\`string\`&&t===(0,${pathVar}.join)((0,${pathVar}.dirname)(n),\`xdg-config\`))return!0;let r=e.CODEX_LINUX_APP_ID;return!!(r&&t.endsWith(\`/\${r}/xdg-config\`))}` +
    `function codexLinuxOpenTargetEnv(){let e={...process.env};codexLinuxShouldDropXdgConfigHome(e)&&delete e.XDG_CONFIG_HOME;for(let t of [\`NODE_OPTIONS\`,\`NODE_PATH\`,\`NODE_REPL_EXTERNAL_MODULE\`,\`ELECTRON_RUN_AS_NODE\`,\`ELECTRON_NO_ASAR\`,\`ELECTRON_ENABLE_LOGGING\`,\`VSCODE_NODE_OPTIONS\`,\`VSCODE_NODE_REPL_EXTERNAL_MODULE\`,\`npm_config_node_options\`,\`NPM_CONFIG_NODE_OPTIONS\`,\`CHROME_DESKTOP\`,\`ELECTRON_RENDERER_URL\`,\`CODEX_ELECTRON_RESOURCES_PATH\`,\`CODEX_ELECTRON_USER_DATA_DIR\`,\`CODEX_LINUX_APP_ID\`,\`CODEX_LINUX_APP_DISPLAY_NAME\`,\`CODEX_LINUX_WEBVIEW_PORT\`])delete e[t];return e}` +
    `function codexLinuxLaunchDetached(e,t,n={}){return new Promise((r,i)=>{let a=!1,o;try{let s=require(\`node:child_process\`).spawn(e,t,{detached:!0,stdio:\`ignore\`,windowsHide:!0,cwd:n.cwd,env:codexLinuxOpenTargetEnv()});o=setTimeout(()=>{a=!0,s.unref?.(),r()},400),o.unref?.(),s.on(\`error\`,e=>{a||(clearTimeout(o),i(e))}),s.on(\`close\`,e=>{a||(clearTimeout(o),e===0?r():i(Error(\`Linux open target launch failed\`)))})}catch(e){clearTimeout(o),i(e)}})}` +
    `function codexLinuxTryReveal(e,t){return new Promise((n,r)=>{let i=!1,a;try{let o=require(\`node:child_process\`).spawn(e,t,{stdio:\`ignore\`,windowsHide:!0,env:codexLinuxOpenTargetEnv()});a=setTimeout(()=>{i=!0,o.unref?.(),n()},400),a.unref?.(),o.on(\`error\`,e=>{i||(clearTimeout(a),r(e))}),o.on(\`close\`,e=>{i||(clearTimeout(a),e===0?n():r(Error(\`Linux file manager reveal failed\`)))})}catch(e){clearTimeout(a),r(e)}})}` +
    `async function codexLinuxOpenFileManager(e){let t=codexLinuxResolveExistingTarget(e)??e;if(typeof t!==\`string\`||t.length===0)throw Error(\`No Linux file manager target available\`);let n=!1;try{n=(0,${fsVar}.existsSync)(t)&&(0,${fsVar}.statSync)(t).isFile()}catch{}if(n)for(let e of [[\`dolphin\`,[\`--select\`,t]],[\`nautilus\`,[\`--select\`,t]]]){let t=codexLinuxFindExecutable(e[0]);if(t)try{await codexLinuxTryReveal(t,e[1]);return}catch{}}t=n?(0,${pathVar}.dirname)(t):t;for(let e of [\`nemo\`,\`thunar\`,\`pcmanfm\`,\`caja\`,\`xdg-open\`]){let n=codexLinuxFindExecutable(e);if(n)try{await codexLinuxLaunchDetached(n,[t]);return}catch{}}throw Error(\`No Linux file manager available\`)}`;

  return currentSource.slice(0, insertionIndex) + helpers + currentSource.slice(insertionIndex);
}

function applyFileManagerDiscoveryPatch(currentSource, deps) {
  let block = findDeclarationBlock(currentSource, "id:`fileManager`");
  if (block == null) {
    warn("Could not find file manager open target");
    return currentSource;
  }
  if (block.text.includes("codexLinuxOpenFileManager(e)")) {
    return currentSource;
  }

  let patchedSource = insertOpenTargetHelpers(currentSource, block.start, deps);
  if (patchedSource !== currentSource) {
    block = findDeclarationBlock(patchedSource, "id:`fileManager`");
    if (block == null) {
      warn("Could not re-read file manager open target");
      return currentSource;
    }
  }

  const insertionPoint = block.text.lastIndexOf("}});");
  if (insertionPoint === -1) {
    warn("Could not find file manager insertion point");
    return currentSource;
  }

  const { electronVar, fsVar, pathVar } = deps;
  const linuxFileManager =
    `,linux:{label:\`File Manager\`,icon:\`apps/file-explorer.png\`,detect:()=>codexLinuxFindExecutable(\`dolphin\`)??codexLinuxFindExecutable(\`nautilus\`)??codexLinuxFindExecutable(\`nemo\`)??codexLinuxFindExecutable(\`thunar\`)??codexLinuxFindExecutable(\`pcmanfm\`)??codexLinuxFindExecutable(\`caja\`)??codexLinuxFindExecutable(\`xdg-open\`)??\`linux-file-manager\`,args:e=>[e],open:async({path:e})=>{await codexLinuxOpenFileManager(e).catch(async()=>{let t=codexLinuxResolveExistingTarget(e)??e;try{(0,${fsVar}.existsSync)(t)&&(0,${fsVar}.statSync)(t).isFile()&&(t=(0,${pathVar}.dirname)(t))}catch{}let r=await ${electronVar}.shell.openPath(t);if(r)throw Error(r)})}}`;

  const existingLinuxBlock = findPropertyBlock(block.text, "linux");
  const patchedBlock =
    existingLinuxBlock == null
      ? block.text.slice(0, insertionPoint + 1) + linuxFileManager + block.text.slice(insertionPoint + 1)
      : block.text.slice(0, existingLinuxBlock.start) +
        linuxFileManager +
        block.text.slice(existingLinuxBlock.end);

  return patchedSource.slice(0, block.start) + patchedBlock + patchedSource.slice(block.end);
}

function insertTerminalHelpers(currentSource, { fsVar, pathVar }) {
  if (currentSource.includes("function codexLinuxTerminalCommand(")) {
    return currentSource;
  }

  const helpers =
    `function codexLinuxTerminalCommand(){for(let e of [\`x-terminal-emulator\`,\`gnome-terminal\`,\`kgx\`,\`konsole\`,\`xfce4-terminal\`,\`mate-terminal\`,\`lxterminal\`,\`tilix\`,\`alacritty\`,\`kitty\`,\`ghostty\`,\`wezterm\`,\`foot\`,\`terminology\`,\`xterm\`]){let t=codexLinuxFindExecutable(e);if(t)return t}return null}` +
    `function codexLinuxTerminalSplitDesktopExec(e){let t=[],n=\`\`,r=null,i=!1;for(let a=0;a<e.length;a++){let o=e[a];if(i){n+=o,i=!1;continue}if(o===\`\\\\\`){i=!0;continue}if(r){o===r?r=null:n+=o;continue}if(o===\`"\`||o===\`'\`){r=o;continue}if(/\\s/u.test(o)){n&&(t.push(n),n=\`\`);continue}n+=o}return n&&t.push(n),t}` +
    `function codexLinuxTerminalDesktopDirs(){if(process.platform!==\`linux\`)return[];let e=process.env.HOME||\`/nonexistent\`,t=process.env.XDG_DATA_HOME&&${pathVar}.isAbsolute(process.env.XDG_DATA_HOME)?[process.env.XDG_DATA_HOME]:[(0,${pathVar}.join)(e,\`.local/share\`)],n=(process.env.XDG_DATA_DIRS&&process.env.XDG_DATA_DIRS.length>0?process.env.XDG_DATA_DIRS:\`/usr/local/share:/usr/share\`).split(\`:\`).filter(Boolean),r=[...t,...n,(0,${pathVar}.join)(e,\`.local/share/flatpak/exports/share\`),\`/var/lib/flatpak/exports/share\`,\`/var/lib/snapd/desktop\`],a=new Set;return r.map(e=>(0,${pathVar}.join)(e,\`applications\`)).filter(e=>e&&${pathVar}.isAbsolute(e)&&!a.has(e)&&(a.add(e),!0))}` +
    `function codexLinuxTerminalDesktopEntryFiles(e,t=0){let n=[];if(t>4)return n;try{for(let r of (0,${fsVar}.readdirSync)(e,{withFileTypes:!0})){let a=(0,${pathVar}.join)(e,r.name);r.isDirectory()?n.push(...codexLinuxTerminalDesktopEntryFiles(a,t+1)):r.isFile()&&r.name.endsWith(\`.desktop\`)&&n.push(a)}}catch{}return n}` +
    `function codexLinuxParseTerminalDesktopEntry(e){let t={Id:(0,${pathVar}.basename)(e).replace(/\\.desktop$/u,\`\`)},n=\`\`;try{for(let r of (0,${fsVar}.readFileSync)(e,\`utf8\`).split(/\\r?\\n/u)){let e=r.trim();if(!e||e.startsWith(\`#\`))continue;if(e.startsWith(\`[\`)&&e.endsWith(\`]\`)){n=e.slice(1,-1);continue}if(n&&n!==\`Desktop Entry\`)continue;let i=e.indexOf(\`=\`);if(i<1)continue;let a=e.slice(0,i).replace(/\\[.*\\]$/u,\`\`),o=e.slice(i+1);t[a]??=o}}catch{return null}let r=e=>(e||\`\`).trim().toLowerCase()===\`true\`;return(t.Type&&t.Type!==\`Application\`)||r(t.NoDisplay)||r(t.Hidden)||!t.Exec||!t.Name?null:t}` +
    `function codexLinuxLooksLikeTerminal(e){let t=(e.Categories||\`\`).toLowerCase(),n=[e.Name,e.GenericName,e.Comment,e.Keywords,e.Exec,e.Id].filter(Boolean).join(\` \`).toLowerCase();return/(^|;)terminalemulator(;|$)/u.test(t)||/\\b(terminal|console|shell|pty|ghostty|wezterm|konsole|alacritty|kitty|foot|xterm)\\b/u.test(n)}` +
    `function codexLinuxTerminalExecutablePath(e){if(!e)return null;if(!(0,${pathVar}.isAbsolute)(e))return codexLinuxFindExecutable(e);try{if((0,${fsVar}.existsSync)(e)){let t=(0,${fsVar}.statSync)(e);if(t.isFile())try{(0,${fsVar}.accessSync)(e,${fsVar}.constants.X_OK);return e}catch{}}}catch{}return null}` +
    `function codexLinuxTerminalCleanDesktopArgs(e){return e.map(e=>e.replace(/%%/gu,\`%\`)).filter(e=>!/^%[fFuUdDnNickvm]$/u.test(e))}` +
    `function codexLinuxResolveTerminalDesktopExec(e){let t=codexLinuxTerminalSplitDesktopExec(e);if(t.length===0)return null;for(;;){if(t[0]===\`env\`){t.shift();continue}if(t[0]&&/^[A-Za-z_][A-Za-z0-9_]*=/u.test(t[0])){t.shift();continue}if(t[0]===\`-u\`||t[0]===\`--unset\`){t.splice(0,2);continue}break}let n=t.shift();if(!n)return null;let r=codexLinuxTerminalExecutablePath(n);return r?{command:r,args:codexLinuxTerminalCleanDesktopArgs(t),base:(0,${pathVar}.basename)(n).replace(/\\.(sh|bin)$/u,\`\`).toLowerCase()}:null}` +
    `function codexLinuxDiscoveredTerminalInfo(){for(let e of codexLinuxTerminalDesktopDirs())for(let t of codexLinuxTerminalDesktopEntryFiles(e)){let e=codexLinuxParseTerminalDesktopEntry(t);if(!e||!codexLinuxLooksLikeTerminal(e))continue;if(e.TryExec&&!codexLinuxTerminalExecutablePath(codexLinuxTerminalSplitDesktopExec(e.TryExec)[0]))continue;let n=codexLinuxResolveTerminalDesktopExec(e.Exec);if(!n)continue;return{command:n.command,args:n.args,dirArg:e[\`X-TerminalArgDir\`]||null}}return null}` +
    `function codexLinuxTerminalInfo(){let e=codexLinuxFindExecutable(\`xdg-terminal-exec\`);if(e)return{command:e,args:[],xdg:!0};let t=codexLinuxTerminalCommand();return t?{command:t,args:[]}:codexLinuxDiscoveredTerminalInfo()}` +
    `function codexLinuxTerminalCwd(e){let t=codexLinuxResolveExistingTarget(e)??e;if(typeof t!==\`string\`||t.length===0)return process.env.HOME||\`/\`;try{if((0,${fsVar}.existsSync)(t)){let e=(0,${fsVar}.statSync)(t);if(e.isDirectory())return t;if(e.isFile())return(0,${pathVar}.dirname)(t)}}catch{}return(0,${pathVar}.dirname)(t)}` +
    `function codexLinuxTerminalArgs(e,t){let n=typeof e===\`string\`?{command:e,args:[]}:e??codexLinuxTerminalInfo(),r=codexLinuxTerminalCwd(t),a=(0,${pathVar}.basename)(n?.command||\`\`).toLowerCase();if(n?.dirArg)return n.dirArg.endsWith(\`=\`)?[...n.args??[],\`\${n.dirArg}\${r}\`]:[...n.args??[],n.dirArg,r];if(n?.args?.length)return n.args;if(n?.xdg)return[];if(a===\`wezterm\`)return[\`start\`,\`--cwd\`,r];if(a===\`konsole\`)return[\`--workdir\`,r];if(a===\`kitty\`)return[\`--directory\`,r];if(a===\`terminology\`)return[\`--workdir\`,r];return[\`gnome-terminal\`,\`kgx\`,\`xfce4-terminal\`,\`mate-terminal\`,\`lxterminal\`,\`tilix\`,\`alacritty\`,\`ghostty\`,\`foot\`].includes(a)?[\`--working-directory\`,r]:[]}`;
  const helperInsertionIndex = currentSource.includes("async function codexLinuxOpenFileManager(")
    ? currentSource.indexOf("async function codexLinuxOpenFileManager(")
    : currentSource.includes("function codexLinuxFindExecutable(")
      ? currentSource.indexOf("function codexLinuxFindExecutable(")
      : 0;
  return currentSource.slice(0, helperInsertionIndex) + helpers + currentSource.slice(helperInsertionIndex);
}

function applyTerminalDiscoveryPatch(currentSource, deps) {
  if (currentSource.includes("linux:{label:`Terminal`")) {
    return currentSource;
  }

  const terminalIndex = currentSource.indexOf("id:`terminal`");
  if (terminalIndex === -1) {
    warn("Could not find terminal open target");
    return currentSource;
  }

  const terminalDeclarationIndex = Math.max(
    currentSource.lastIndexOf("var ", terminalIndex),
    currentSource.lastIndexOf("let ", terminalIndex),
    currentSource.lastIndexOf("const ", terminalIndex),
  );
  let patchedSource = insertOpenTargetHelpers(
    currentSource,
    terminalDeclarationIndex >= 0 ? terminalDeclarationIndex : terminalIndex,
    deps,
  );
  patchedSource = insertTerminalHelpers(patchedSource, deps);

  const patchedTerminalIndex = patchedSource.indexOf("id:`terminal`");
  const platformsIndex = patchedSource.indexOf("platforms:{", patchedTerminalIndex);
  const platformsBlock =
    platformsIndex === -1 ? null : findBalancedBlock(patchedSource, patchedSource.indexOf("{", platformsIndex));
  if (platformsBlock == null || platformsBlock.text.includes("linux:{")) {
    warn("Could not apply terminal open-target patch");
    return currentSource;
  }

  const linuxTerminal =
    `,linux:{label:\`Terminal\`,icon:\`apps/terminal.png\`,kind:\`terminal\`,detect:()=>codexLinuxTerminalInfo()?.command??null,args:e=>codexLinuxTerminalArgs(codexLinuxTerminalInfo(),e),open:async({command:e,path:t})=>{await codexLinuxLaunchDetached(e,codexLinuxTerminalArgs(codexLinuxTerminalInfo()??e,t),{cwd:codexLinuxTerminalCwd(t)})}}`;
  return patchedSource.slice(0, platformsBlock.end - 1) + linuxTerminal + patchedSource.slice(platformsBlock.end - 1);
}

function applyIdeDiscoveryPatch(currentSource, deps) {
  if (currentSource.includes("...codexLinuxDiscoveredIdeTargets()")) {
    return currentSource;
  }

  const { fsVar, pathVar } = deps;
  const editorFactoryIndex = currentSource.search(/function\s+[A-Za-z_$][\w$]*\(\{id:[A-Za-z_$][\w$]*,label:[A-Za-z_$][\w$]*,icon:[A-Za-z_$][\w$]*,darwinDetect:/u);
  const jetBrainsFactoryIndex = currentSource.search(/function\s+[A-Za-z_$][\w$]*\(\{id:[A-Za-z_$][\w$]*,label:[A-Za-z_$][\w$]*,icon:[A-Za-z_$][\w$]*,toolboxTarget:/u);
  const hasEditorFactory = editorFactoryIndex !== -1;
  const hasJetBrainsFactory = jetBrainsFactoryIndex !== -1;
  const hasZedTarget = currentSource.includes("id:`zed`");
  if (!hasEditorFactory && !hasJetBrainsFactory && !hasZedTarget) {
    warn("Could not find IDE open-target factories");
    return currentSource;
  }

  const zedIndex = currentSource.indexOf("id:`zed`");
  const zedDeclarationIndex =
    zedIndex === -1
      ? -1
      : Math.max(
          currentSource.lastIndexOf("var ", zedIndex),
          currentSource.lastIndexOf("let ", zedIndex),
          currentSource.lastIndexOf("const ", zedIndex),
        );
  const openTargetHelperInsertionIndex =
    [editorFactoryIndex, jetBrainsFactoryIndex].filter((index) => index >= 0).sort((a, b) => a - b)[0] ??
    (zedDeclarationIndex >= 0 ? zedDeclarationIndex : 0);
  let patchedSource = insertOpenTargetHelpers(
    currentSource,
    openTargetHelperInsertionIndex,
    deps,
  );

  const ideCoreHelpers = patchedSource.includes("function codexLinuxIdeCommand(")
    ? ""
    : `function codexLinuxIdeCommand(e){let t={cursor:[\`cursor\`],vscode:[\`code\`,\`codium\`],vscodeInsiders:[\`code-insiders\`],windsurf:[\`windsurf\`],antigravity:[\`antigravity\`],zed:[\`zed\`,\`zeditor\`,\`zedit\`,\`zed-cli\`],intellij:[\`idea\`],webstorm:[\`webstorm\`],pycharm:[\`pycharm\`],goland:[\`goland\`],clion:[\`clion\`],rustrover:[\`rustrover\`],rider:[\`rider\`],phpstorm:[\`phpstorm\`],androidStudio:[\`studio\`,\`studio.sh\`]}[e]??[];for(let e of t){let t=codexLinuxFindExecutable(e);if(t)return t}return null}` +
      `function codexLinuxIdePlatform(e,t,n,r,i){let a=codexLinuxIdeCommand(e);return a?{label:t,icon:n,kind:\`editor\`,hidden:r,detect:()=>a,args:i,supportsSsh:!0}:void 0}` +
      `function codexLinuxJetBrainsIdePlatform(e,t,n,r){let i=codexLinuxIdeCommand(e);return i?{label:t,icon:n,kind:\`editor\`,detect:()=>i,args:r}:void 0}`;
  const dynamicDiscoveryHelpers = patchedSource.includes("function codexLinuxDiscoveredIdeTargets(")
    ? ""
    : `function codexLinuxSplitDesktopExec(e){let t=[],n=\`\`,r=null,i=!1;for(let a=0;a<e.length;a++){let o=e[a];if(i){n+=o,i=!1;continue}if(o===\`\\\\\`){i=!0;continue}if(r){o===r?r=null:n+=o;continue}if(o===\`"\`||o===\`'\`){r=o;continue}if(/\\s/u.test(o)){n&&(t.push(n),n=\`\`);continue}n+=o}return n&&t.push(n),t}` +
    `function codexLinuxDesktopDirs(){if(process.platform!==\`linux\`)return[];let e=process.env.HOME||\`/nonexistent\`,t=process.env.XDG_DATA_HOME&&(0,${pathVar}.isAbsolute)(process.env.XDG_DATA_HOME)?[process.env.XDG_DATA_HOME]:[(0,${pathVar}.join)(e,\`.local/share\`)],n=(process.env.XDG_DATA_DIRS&&process.env.XDG_DATA_DIRS.length>0?process.env.XDG_DATA_DIRS:\`/usr/local/share:/usr/share\`).split(\`:\`).filter(Boolean),r=[...t,...n,(0,${pathVar}.join)(e,\`.local/share/flatpak/exports/share\`),\`/var/lib/flatpak/exports/share\`,\`/var/lib/snapd/desktop\`],a=new Set;return r.map(e=>(0,${pathVar}.join)(e,\`applications\`)).filter(e=>e&&(0,${pathVar}.isAbsolute)(e)&&!a.has(e)&&(a.add(e),!0))}` +
    `function codexLinuxDesktopEntryFiles(e,t=0){let n=[];if(t>4)return n;try{for(let r of (0,${fsVar}.readdirSync)(e,{withFileTypes:!0})){let a=(0,${pathVar}.join)(e,r.name);r.isDirectory()?n.push(...codexLinuxDesktopEntryFiles(a,t+1)):r.isFile()&&r.name.endsWith(\`.desktop\`)&&n.push(a)}}catch{}return n}` +
    `function codexLinuxParseDesktopEntry(e){let t={Id:(0,${pathVar}.basename)(e).replace(/\\.desktop$/u,\`\`)},n=\`\`;try{for(let r of (0,${fsVar}.readFileSync)(e,\`utf8\`).split(/\\r?\\n/u)){let e=r.trim();if(!e||e.startsWith(\`#\`))continue;if(e.startsWith(\`[\`)&&e.endsWith(\`]\`)){n=e.slice(1,-1);continue}if(n&&n!==\`Desktop Entry\`)continue;let i=e.indexOf(\`=\`);if(i<1)continue;let a=e.slice(0,i).replace(/\\[.*\\]$/u,\`\`),o=e.slice(i+1);t[a]??=o}}catch{return null}let r=e=>(e||\`\`).trim().toLowerCase()===\`true\`;return(t.Type&&t.Type!==\`Application\`)||r(t.NoDisplay)||r(t.Terminal)||!r(t.Hidden)&&(!t.Exec||!t.Name)?null:t}` +
    `function codexLinuxLooksLikeIde(e){let t=\`;\${(e.Categories||\`\`).toLowerCase()};\`,n=[e.Name,e.GenericName,e.Comment,e.Keywords,e.Exec].filter(Boolean).join(\` \`).toLowerCase(),r=/(;)(office|wordprocessor|spreadsheet|presentation|graphics|audiovideo|audiovideoediting|building)(;)/u.test(t),i=/;ide;/u.test(t);if(r&&!i)return!1;if(i)return!0;return/(;)(development|texteditor)(;)/u.test(t)&&/\\b(code|coding|codium|cursor|zed|ide|jetbrains|sublime|emacs|vim|neovim|neovide|kate|builder|windsurf|antigravity|rstudio|positron|agent|agents|agentic|workspace|workspaces)\\b/u.test(n)}` +
    `function codexLinuxExecutablePath(e){if(!e)return null;if(!(0,${pathVar}.isAbsolute)(e))return codexLinuxFindExecutable(e);try{if((0,${fsVar}.existsSync)(e)){let t=(0,${fsVar}.statSync)(e);if(t.isFile())try{(0,${fsVar}.accessSync)(e,${fsVar}.constants.X_OK);return e}catch{}}}catch{}return null}` +
    `function codexLinuxResolveDesktopExec(e){let t=codexLinuxSplitDesktopExec(e);if(t.length===0)return null;for(;;){if(t[0]===\`env\`){t.shift();continue}if(t[0]&&/^[A-Za-z_][A-Za-z0-9_]*=/u.test(t[0])){t.shift();continue}if(t[0]===\`-u\`||t[0]===\`--unset\`){t.splice(0,2);continue}break}let n=t.shift();if(!n)return null;let r=codexLinuxExecutablePath(n);return r?{command:r,args:t,base:(0,${pathVar}.basename)(n).replace(/\\.(sh|bin)$/u,\`\`).toLowerCase()}:null}` +
    `function codexLinuxPathToFileUri(e){try{return require(\`node:url\`).pathToFileURL(e).toString()}catch{return e}}` +
    `function codexLinuxDesktopArgs(e,t){let n=[],r=codexLinuxPathToFileUri(t);for(let a of e){if(a===\`%%\`){n.push(\`%\`);continue}if(/^%[fF]$/u.test(a)){n.push(t);continue}if(/^%[uU]$/u.test(a)){n.push(r);continue}if(/^%[dD]$/u.test(a)){n.push((0,${pathVar}.dirname)(t));continue}if(/^%[nN]$/u.test(a)){n.push((0,${pathVar}.basename)(t));continue}if(/^%[ickvm]$/u.test(a))continue;let o=a.replace(/%[fF]/gu,t).replace(/%[uU]/gu,r).replace(/%[dD]/gu,(0,${pathVar}.dirname)(t)).replace(/%[nN]/gu,(0,${pathVar}.basename)(t)).replace(/%%/gu,\`%\`).replace(/%[A-Za-z]/gu,\`\`);o&&n.push(o)}return n}` +
    `function codexLinuxDesktopEntryLaunchId(e){return(0,${pathVar}.basename)(e).replace(/\\.desktop$/u,\`\`)}` +
    `function codexLinuxDesktopLaunchOptions(){return{cwd:process.env.HOME||void 0}}` +
    `async function codexLinuxLaunchDesktopEntry(e,t,n,r){let i=codexLinuxFindExecutable(\`gio\`),a=codexLinuxDesktopLaunchOptions();if(i)try{await codexLinuxLaunchDetached(i,[\`launch\`,e,t],a);return}catch{}let o=codexLinuxFindExecutable(\`gtk-launch\`);if(o)try{await codexLinuxLaunchDetached(o,[codexLinuxDesktopEntryLaunchId(e),codexLinuxPathToFileUri(t)],a);return}catch{}await codexLinuxLaunchDetached(n,codexLinuxDesktopArgs(r,t),a)}` +
    `function codexLinuxKnownIdeDesktopDuplicate(e){let t=new Set([\`cursor\`,\`code\`,\`codium\`,\`code-insiders\`,\`windsurf\`,\`antigravity\`,\`zed\`,\`zeditor\`,\`zedit\`,\`zed-cli\`,\`idea\`,\`webstorm\`,\`pycharm\`,\`goland\`,\`clion\`,\`rustrover\`,\`rider\`,\`phpstorm\`,\`studio\`,\`studio.sh\`]);return t.has(e.base)&&codexLinuxFindExecutable(e.base)!=null}` +
    `function codexLinuxDesktopIdeIcon(e,t){let n=\`\${e.Name||\`\`} \${e.Id||\`\`} \${t.base||\`\`}\`.toLowerCase();for(let[e,t]of [[\`cursor\`,\`apps/cursor.png\`],[\`code-insiders\`,\`apps/vscode-insiders.png\`],[\`vscode\`,\`apps/vscode.png\`],[\`visual studio code\`,\`apps/vscode.png\`],[\`codium\`,\`apps/vscode.png\`],[\`zed\`,\`apps/zed.png\`],[\`sublime\`,\`apps/sublime-text.png\`],[\`emacs\`,\`apps/emacs.png\`],[\`intellij\`,\`apps/intellij.png\`],[\`webstorm\`,\`apps/webstorm.svg\`],[\`pycharm\`,\`apps/pycharm.png\`],[\`goland\`,\`apps/goland.png\`],[\`clion\`,\`apps/clion.png\`],[\`rustrover\`,\`apps/rustrover.png\`],[\`rider\`,\`apps/rider.png\`],[\`phpstorm\`,\`apps/phpstorm.png\`],[\`android studio\`,\`apps/android-studio.png\`],[\`windsurf\`,\`apps/windsurf.png\`],[\`antigravity\`,\`apps/antigravity.png\`]])if(n.includes(e))return t;return\`apps/terminal.png\`}` +
    `function codexLinuxIconSearchRoots(){let e=process.env.HOME||\`/nonexistent\`,t=process.env.XDG_DATA_HOME&&(0,${pathVar}.isAbsolute)(process.env.XDG_DATA_HOME)?process.env.XDG_DATA_HOME:(0,${pathVar}.join)(e,\`.local/share\`),n=(process.env.XDG_DATA_DIRS&&process.env.XDG_DATA_DIRS.length>0?process.env.XDG_DATA_DIRS:\`/usr/local/share:/usr/share\`).split(\`:\`).filter(Boolean),r=[(0,${pathVar}.join)(t,\`icons\`),(0,${pathVar}.join)(e,\`.icons\`),...n.map(e=>(0,${pathVar}.join)(e,\`icons\`)),(0,${pathVar}.join)(t,\`pixmaps\`),...n.map(e=>(0,${pathVar}.join)(e,\`pixmaps\`)),(0,${pathVar}.join)(e,\`.local/share/flatpak/exports/share/icons\`),(0,${pathVar}.join)(e,\`.local/share/flatpak/exports/share/pixmaps\`),\`/var/lib/flatpak/exports/share/icons\`,\`/var/lib/flatpak/exports/share/pixmaps\`,\`/var/lib/snapd/desktop/icons\`],a=new Set;return r.filter(e=>e&&(0,${pathVar}.isAbsolute)(e)&&!a.has(e)&&(a.add(e),!0))}` +
    `function codexLinuxFindIconFile(e,t,n=0){if(n>6)return null;try{for(let r of (0,${fsVar}.readdirSync)(e,{withFileTypes:!0})){let a=(0,${pathVar}.join)(e,r.name);if(r.isDirectory()){let e=codexLinuxFindIconFile(a,t,n+1);if(e)return e}else if(r.isFile()&&r.name.replace(/\\.(png|svg|xpm)$/iu,\`\`)===t&&/\\.(png|svg|xpm)$/iu.test(r.name))return a}}catch{}return null}` +
    `function codexLinuxDesktopIconPath(e){let t=(e.Icon||\`\`).trim();if(!t)return null;if((0,${pathVar}.isAbsolute)(t)){try{if((0,${fsVar}.existsSync)(t))return t}catch{}return null}let n=t.replace(/\\.(png|svg|xpm)$/iu,\`\`),r=[\`png\`,\`svg\`,\`xpm\`];for(let e of codexLinuxIconSearchRoots())for(let t of r){let r=(0,${pathVar}.join)(e,\`\${n}.\${t}\`);try{if((0,${fsVar}.existsSync)(r))return r}catch{}}for(let e of codexLinuxIconSearchRoots()){let t=codexLinuxFindIconFile(e,n);if(t)return t}return null}` +
    `function codexLinuxDesktopIdeId(e){let t=(e.Id||e.Name||e.Exec||\`app\`).toLowerCase().replace(/\\.desktop$/u,\`\`).replace(/[^a-z0-9]+/gu,\`-\`).replace(/^-|-$/gu,\`\`).slice(0,64)||\`app\`;return\`linux-desktop-\${t}\`}` +
    `function codexLinuxUniqueDesktopIdeId(e,t){let n=codexLinuxDesktopIdeId(e),r=n,i=2;for(;t.has(r);)r=\`\${n}-\${i++}\`;return t.add(r),r}` +
    `function codexLinuxDiscoveredIdeTargets(){if(process.platform!==\`linux\`)return[];let e=[],t=new Set,n=new Set,r=new Set;for(let a of codexLinuxDesktopDirs())for(let o of codexLinuxDesktopEntryFiles(a)){let a=codexLinuxParseDesktopEntry(o),s=a?.Id?.toLowerCase();if(!a)continue;if((a.Hidden||\`\`).trim().toLowerCase()===\`true\`){s&&r.add(s);continue}if(s&&r.has(s)||!codexLinuxLooksLikeIde(a))continue;if(a.TryExec&&!codexLinuxExecutablePath(codexLinuxSplitDesktopExec(a.TryExec)[0]))continue;let i=codexLinuxResolveDesktopExec(a.Exec);if(!i||codexLinuxKnownIdeDesktopDuplicate(i))continue;let c=\`\${a.Name}|${"${i.command}"}|${"${i.args.join(` `)}"}\`.toLowerCase();if(t.has(c))continue;t.add(c);let l=a.Name.trim(),u=codexLinuxDesktopIdeIcon(a,i),d=codexLinuxUniqueDesktopIdeId(a,n),f=codexLinuxDesktopIconPath(a);e.push({id:d,platforms:{linux:{label:l,icon:u,iconPath:f?()=>f:void 0,kind:\`editor\`,detect:()=>i.command,args:e=>codexLinuxDesktopArgs(i.args,e),open:async({command:e,path:t})=>{await codexLinuxLaunchDesktopEntry(o,t,e,i.args)}}}})}return e}`;

  const helpers = ideCoreHelpers + dynamicDiscoveryHelpers;
  if (helpers.length > 0) {
    const helperInsertionIndex = patchedSource.includes("function codexLinuxFindExecutable(")
      ? patchedSource.indexOf("function codexLinuxFindExecutable(")
      : 0;
    const helperEnd = patchedSource.indexOf("async function codexLinuxOpenFileManager(", helperInsertionIndex);
    const ideHelperInsertionIndex = helperEnd === -1 ? helperInsertionIndex : helperEnd;
    patchedSource = patchedSource.slice(0, ideHelperInsertionIndex) + helpers + patchedSource.slice(ideHelperInsertionIndex);
  }

  patchedSource = patchedSource.replace(
    /(function\s+[A-Za-z_$][\w$]*\(\{id:([A-Za-z_$][\w$]*),label:([A-Za-z_$][\w$]*),icon:([A-Za-z_$][\w$]*),darwinDetect:[^)]*?hidden:([A-Za-z_$][\w$]*)\}\)\{return\{id:\2,platforms:\{[^]*?win32:[^]*?args:([A-Za-z_$][\w$]*),supportsSsh:!0\}:void 0)(\}\}\})/u,
    "$1,linux:codexLinuxIdePlatform($2,$3,$4,$5,$6)$7",
  );

  patchedSource = patchedSource.replace(
    /(function\s+[A-Za-z_$][\w$]*\(\{id:([A-Za-z_$][\w$]*),label:([A-Za-z_$][\w$]*),icon:([A-Za-z_$][\w$]*),toolboxTarget:[^)]*?\}\)\{return\{id:\2,platforms:\{[^]*?args:([A-Za-z_$][\w$]*)\}:void 0)(\}\}\})/u,
    "$1,linux:codexLinuxJetBrainsIdePlatform($2,$3,$4,$5)$6",
  );

  const patchedZedIndex = patchedSource.indexOf("id:`zed`");
  if (patchedZedIndex !== -1) {
    const zedPlatformsIndex = patchedSource.indexOf("platforms:{", patchedZedIndex);
    const zedPlatformsBlock = findBalancedBlock(patchedSource, patchedSource.indexOf("{", zedPlatformsIndex));
    if (zedPlatformsBlock != null && !zedPlatformsBlock.text.includes("linux:{")) {
      const argsVar = zedPlatformsBlock.text.match(/win32:\{[^}]*args:([A-Za-z_$][\w$]*)/u)?.[1];
      if (argsVar != null) {
        const linuxZed = `,linux:{label:\`Zed\`,icon:\`apps/zed.png\`,kind:\`editor\`,detect:()=>codexLinuxIdeCommand(\`zed\`),args:${argsVar}}`;
        patchedSource =
          patchedSource.slice(0, zedPlatformsBlock.end - 1) +
          linuxZed +
          patchedSource.slice(zedPlatformsBlock.end - 1);
      }
    }
  }

  if (!patchedSource.includes("...codexLinuxDiscoveredIdeTargets()")) {
    const targetArraySearchStart = Math.min(
      ...[editorFactoryIndex, jetBrainsFactoryIndex, zedDeclarationIndex].filter((index) => index >= 0),
    );
    const targetArrayMatch = patchedSource
      .slice(Number.isFinite(targetArraySearchStart) ? targetArraySearchStart : 0)
      .match(/var\s+[A-Za-z_$][\w$]*=\[[A-Za-z_$][\w$]*(?:,[A-Za-z_$][\w$]*)+\](?=,|;)/u);
    if (targetArrayMatch != null) {
      const targetArrayStart =
        (Number.isFinite(targetArraySearchStart) ? targetArraySearchStart : 0) + targetArrayMatch.index;
      const targetArrayText = targetArrayMatch[0];
      const patchedTargetArray = `${targetArrayText.slice(0, -1)},...codexLinuxDiscoveredIdeTargets()]`;
      patchedSource =
        patchedSource.slice(0, targetArrayStart) +
        patchedTargetArray +
        patchedSource.slice(targetArrayStart + targetArrayText.length);
    } else {
      warn("Could not append dynamic IDE desktop discovery");
    }
  }

  if (hasEditorFactory && !patchedSource.includes("linux:codexLinuxIdePlatform(")) {
    warn("Could not apply generic IDE factory patch");
  }
  if (hasJetBrainsFactory && !patchedSource.includes("linux:codexLinuxJetBrainsIdePlatform(")) {
    warn("Could not apply JetBrains IDE factory patch");
  }
  if (hasZedTarget && !patchedSource.includes("linux:{label:`Zed`")) {
    warn("Could not apply Zed IDE target patch");
  }

  return patchedSource;
}

function applyLinuxIconPathResolutionPatch(currentSource) {
  if (!currentSource.includes("iconPath?") || !currentSource.includes("async function d_(")) {
    return currentSource;
  }

  let patchedSource = currentSource;
  const linuxIconPlatformNeedle = "return(e===`win32`||e===`linux`)?Promise.all";
  if (!patchedSource.includes(linuxIconPlatformNeedle)) {
    const win32IconPlatformNeedle = "return e===`win32`?Promise.all";
    if (patchedSource.includes(win32IconPlatformNeedle)) {
      patchedSource = patchedSource.replace(
        win32IconPlatformNeedle,
        linuxIconPlatformNeedle,
      );
    } else {
      warn("Could not find open target icon platform gate");
    }
  }

  if (!patchedSource.includes("codexLinuxOpenTargetIconImage(")) {
    const iconResolverNeedle =
      "let r=e.toLowerCase().endsWith(`.lnk`)?await f_(e):await n.app.getFileIcon(e,{size:`normal`});return!r||r.isEmpty()?t:r.toDataURL()";
    if (patchedSource.includes(iconResolverNeedle)) {
      patchedSource = patchedSource.replace(
        iconResolverNeedle,
        "let r=codexLinuxOpenTargetIconImage(e)??(e.toLowerCase().endsWith(`.lnk`)?await f_(e):await n.app.getFileIcon(e,{size:`normal`}));return!r||r.isEmpty()?t:r.toDataURL()",
      );
      const resolverIndex = patchedSource.indexOf("async function d_(");
      if (resolverIndex >= 0) {
        patchedSource =
          patchedSource.slice(0, resolverIndex) +
          "function codexLinuxOpenTargetIconImage(e){if(process.platform!==`linux`||typeof e!==`string`||!/\\.(png|svg|jpe?g|bmp|ico)$/iu.test(e))return null;try{let t=n.nativeImage.createFromPath(e);return t.isEmpty()?null:t}catch{return null}}" +
          patchedSource.slice(resolverIndex);
      }
    } else {
      warn("Could not find open target icon resolver");
    }
  }

  return patchedSource;
}

function applyMainBundlePatch(currentSource) {
  const electronVar = requireName(currentSource, "electron");
  const fsVar = requireName(currentSource, "node:fs");
  const pathVar = requireName(currentSource, "node:path");
  if (fsVar == null || pathVar == null) {
    warn("Could not find node:fs/node:path dependencies");
    return currentSource;
  }

  const deps = { electronVar, fsVar, pathVar };
  let patchedSource = currentSource;
  if (electronVar != null) {
    patchedSource = applyFileManagerDiscoveryPatch(patchedSource, deps);
  }
  patchedSource = applyTerminalDiscoveryPatch(patchedSource, deps);
  patchedSource = applyIdeDiscoveryPatch(patchedSource, deps);
  patchedSource = applyLinuxIconPathResolutionPatch(patchedSource);
  return patchedSource;
}

module.exports = {
  applyMainBundlePatch,
};
