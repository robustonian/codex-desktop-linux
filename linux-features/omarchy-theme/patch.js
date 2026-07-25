"use strict";

const RUNTIME_VERSION = "omarchy-theme-v1";
const STYLE_LINK_ID = "codex-linux-omarchy-theme-link";
const THEME_CSS_ENDPOINT = "/__codex_user_stylesheet.css";

function omarchyThemeRuntimeSource() {
  return [
    ";(()=>{",
    `const VERSION=${JSON.stringify(RUNTIME_VERSION)};`,
    "if(globalThis.codexLinuxOmarchyThemeVersion===VERSION)return;",
    "try{globalThis.codexLinuxOmarchyThemeCleanup?.()}catch{}",
    "globalThis.codexLinuxOmarchyThemeVersion=VERSION;",
    `const STYLE_LINK_ID=${JSON.stringify(STYLE_LINK_ID)};`,
    `const THEME_CSS_ENDPOINT=${JSON.stringify(THEME_CSS_ENDPOINT)};`,
    "let interval=null,installed=false;",
    "function refresh(){if(typeof document===`undefined`)return;const target=document.head||document.documentElement;if(!target)return;let link=document.getElementById(STYLE_LINK_ID);if(!link){link=document.createElement(`link`);link.id=STYLE_LINK_ID;link.rel=`stylesheet`;link.type=`text/css`;target.appendChild(link)}link.href=THEME_CSS_ENDPOINT+`?t=`+Date.now()}",
    "function onVisibilityChange(){document.hidden||refresh()}",
    "function install(){if(installed)return;installed=true;refresh();interval=setInterval(refresh,5000);window.addEventListener(`focus`,refresh);document.addEventListener(`visibilitychange`,onVisibilityChange)}",
    "function cleanup(){document.removeEventListener(`DOMContentLoaded`,install);document.removeEventListener(`visibilitychange`,onVisibilityChange);window.removeEventListener(`focus`,refresh);interval!=null&&clearInterval(interval);interval=null;installed=false}",
    "globalThis.codexLinuxOmarchyThemeCleanup=cleanup;",
    "document.readyState===`loading`?document.addEventListener(`DOMContentLoaded`,install,{once:true}):install();",
    "})();",
  ].join("");
}

function applyOmarchyThemeLoader(source) {
  if (typeof source !== "string") {
    return source;
  }
  if (source.includes("codexLinuxOmarchyThemeVersion=")) {
    return source;
  }
  return `${source}\n${omarchyThemeRuntimeSource()}`;
}

module.exports = {
  RUNTIME_VERSION,
  STYLE_LINK_ID,
  THEME_CSS_ENDPOINT,
  descriptors: [
    {
      id: "omarchy-theme-css-loader",
      phase: "webview-asset",
      order: 20_780,
      ciPolicy: "optional",
      pattern: /^index-.*\.js$/,
      missingDescription: "webview index bundle",
      skipDescription: "Omarchy theme CSS loader",
      apply: applyOmarchyThemeLoader,
    },
  ],
  applyOmarchyThemeLoader,
  omarchyThemeRuntimeSource,
};
