#!/usr/bin/env node

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const crypto = require("node:crypto");

const ROOT = path.resolve(__dirname, "..");
const OUTPUT = path.join(ROOT, "chat-reference", "profile-menu");
const CDP_HTTP = process.env.CHATGPT_CDP_HTTP || "http://127.0.0.1:9222";
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const hash = (text) => crypto.createHash("sha256").update(text).digest("hex");

function getJson(url) {
  return new Promise((resolve, reject) => http.get(url, (response) => {
    let body = "";
    response.on("data", (chunk) => (body += chunk));
    response.on("end", () => resolve(JSON.parse(body)));
  }).on("error", reject));
}

class CDP {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.id = 0;
    this.pending = new Map();
    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      const handler = this.pending.get(message.id);
      if (handler) handler(message);
      this.pending.delete(message.id);
    };
  }
  open() {
    return new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });
  }
  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.id;
      this.pending.set(id, (message) => message.error ? reject(new Error(message.error.message)) : resolve(message.result));
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }
  async eval(expression, awaitPromise = false) {
    const result = await this.send("Runtime.evaluate", { expression, awaitPromise, returnByValue: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    return result.result.value;
  }
}

async function openProfileMenu(cdp) {
  const state = await cdp.eval(`(() => {
    const button = document.querySelector('button[aria-label="打开个人资料菜单"]');
    if (!button) return null;
    const rect = button.getBoundingClientRect();
    return { open: button.dataset.state === 'open', x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  })()`);
  if (!state) throw new Error("Profile menu trigger was not found on the main conversation page");
  if (!state.open) {
    await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: state.x, y: state.y, button: "left", clickCount: 1 });
    await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: state.x, y: state.y, button: "left", clickCount: 1 });
  }
}

async function waitForStableOpenMenu(cdp) {
  let prior = null;
  let stable = 0;
  const started = Date.now();
  while (Date.now() - started < 30_000) {
    const state = JSON.parse(await cdp.eval(`JSON.stringify((() => {
      const trigger = document.querySelector('button[aria-label="打开个人资料菜单"]');
      const menus = [...document.querySelectorAll('[role="menu"]')].filter(element => {
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return rect.width > 0 && rect.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
      });
      return {
        open: trigger?.dataset.state === 'open' && trigger?.getAttribute('aria-expanded') === 'true',
        menuCount: menus.length,
        menuText: menus.map(menu => menu.innerText).join('\\n'),
        fonts: document.fonts.status,
        incompleteImages: [...document.images].filter(image => !image.complete).length,
        signature: menus.map(menu => menu.outerHTML).join('') + '\\n' + document.body.innerText
      };
    })())`));
    const current = hash(state.signature);
    if (state.open && state.menuCount === 1 && state.menuText.includes("设置") &&
        state.fonts === "loaded" && state.incompleteImages === 0 && current === prior) stable += 1;
    else stable = 0;
    prior = current;
    if (stable >= 3) {
      await cdp.eval(`new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`, true);
      return { waitMs: Date.now() - started, menuText: state.menuText, stableSamples: stable + 1 };
    }
    await delay(350);
  }
  throw new Error("The profile/settings menu did not reach a stable open state");
}

async function snapshot(cdp) {
  return JSON.parse(await cdp.eval(`(async () => {
    await document.fonts.ready;
    const clone = document.documentElement.cloneNode(true);
    const originals = [...document.images];
    const images = [...clone.querySelectorAll('img')];
    for (let i = 0; i < originals.length; i += 1) {
      const source = originals[i].currentSrc || originals[i].src;
      if (!source || source.startsWith('data:')) continue;
      try {
        const blob = await (await fetch(source)).blob();
        images[i].src = await new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result);
          reader.onerror = reject;
          reader.readAsDataURL(blob);
        });
        images[i].removeAttribute('srcset');
      } catch (_) {}
    }
    return JSON.stringify({
      html: clone.outerHTML,
      url: location.href,
      viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio }
    });
  })()`, true));
}

function removeTag(html, tag) {
  return html.replace(new RegExp(`<${tag}\\b[^>]*>[\\s\\S]*?<\\/${tag}>`, "gi"), "");
}

function themedHtml(source, theme, waitInfo) {
  let html = removeTag(removeTag(source.html, "script"), "style");
  html = html.replace(/<meta\b[^>]*http-equiv=["']Content-Security-Policy["'][^>]*>/gi, "");
  html = html.replace(/<link\b[^>]*rel=["'](?:stylesheet|modulepreload|preload)["'][^>]*>/gi, "");
  html = html.replace(/<html\b[^>]*>/i, (tag) => tag.replace(/\sstyle="([^"]*)"/i, (_style, value) => {
    const kept = value.split(";").map((item) => item.trim()).filter(Boolean).filter((declaration) => {
      const name = declaration.slice(0, declaration.indexOf(":" )).trim();
      return !name.startsWith("--color-") && !name.startsWith("--shadow-") &&
        !["--codex-base-contrast", "--codex-base-ink", "--codex-base-surface"].includes(name);
    });
    return kept.length ? ` style="${kept.join("; ")};"` : "";
  }));
  html = html.replace(/<html\b([^>]*)class=["']([^"']*)["']([^>]*)>/i, (_all, before, classes, after) => {
    const next = classes.split(/\s+/).filter(Boolean)
      .filter((name) => !["dark", "light", "electron-dark", "electron-light"].includes(name));
    next.push(theme, `electron-${theme}`, "electron-opaque");
    return `<html${before}class="${[...new Set(next)].join(" ")}"${after}>`;
  });
  html = html.replace(/<title>[\s\S]*?<\/title>/i, `<title>主对话 · 个人资料设置菜单 · ${theme}</title>`);
  const head = `\n<!-- CDP stable snapshot: profile/settings menu open; theme=${theme}; wait_ms=${waitInfo.waitMs}; captured_at=${new Date().toISOString()} -->\n<link rel="stylesheet" href="./css/base.css">\n<link rel="stylesheet" href="./css/static.css">`;
  html = html.replace(/<head([^>]*)>/i, `<head$1>${head}`);
  return `<!DOCTYPE html>\n${html}\n`;
}

async function main() {
  const target = (await getJson(`${CDP_HTTP}/json/list`))
    .find((item) => item.type === "page" && item.url === "app://-/index.html");
  if (!target) throw new Error("ChatGPT main CDP target not found");
  const cdp = new CDP(target.webSocketDebuggerUrl);
  await cdp.open();
  try {
    await openProfileMenu(cdp);
    const waitInfo = await waitForStableOpenMenu(cdp);
    const source = await snapshot(cdp);
    const css = await cdp.eval(`(() => [...document.styleSheets].map((sheet, index) => {
      try { return '/* stylesheet ' + index + ': ' + (sheet.href || 'inline') + ' */\\n' + [...sheet.cssRules].map(rule => rule.cssText).join('\\n'); }
      catch (error) { return '/* unavailable stylesheet ' + index + ': ' + String(error) + ' */'; }
    }).join('\\n\\n'))()`);

    fs.mkdirSync(path.join(OUTPUT, "css"), { recursive: true });
    fs.writeFileSync(path.join(OUTPUT, "css", "base.css"), css);
    fs.writeFileSync(path.join(OUTPUT, "css", "static.css"), `
html,body{width:100%;height:100%;margin:0}
*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}
html.light,html.electron-light{color-scheme:light}
html.dark,html.electron-dark{color-scheme:dark}
button,a,input,select,textarea,[contenteditable]{pointer-events:none!important}
body>[data-radix-popper-content-wrapper]:has([role="menu"]){
  position:fixed!important;
  inset:auto auto 64px 8px!important;
  transform:none!important;
  max-height:calc(100vh - 80px)!important;
}
body>[data-radix-popper-content-wrapper]:has([role="menu"])>[role="menu"]{
  max-height:calc(100vh - 80px)!important;
  overflow:auto!important;
}
`);
    for (const theme of ["light", "dark"]) {
      fs.writeFileSync(path.join(OUTPUT, `${theme}.html`), themedHtml(source, theme, waitInfo));
    }
    fs.writeFileSync(path.join(OUTPUT, "index.html"), `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>主对话设置菜单快照</title></head><body><p><a href="light.html">Light</a></p><p><a href="dark.html">Dark</a></p></body></html>\n`);
    fs.writeFileSync(path.join(OUTPUT, "manifest.json"), `${JSON.stringify({
      generatedAt: new Date().toISOString(), targetId: target.id, source: source.url,
      viewport: source.viewport, menuText: waitInfo.menuText, waitMs: waitInfo.waitMs,
    }, null, 2)}\n`);
    console.log(`Created ${path.join(OUTPUT, "light.html")} and dark.html`);
  } finally {
    cdp.ws.close();
  }
}

main().catch((error) => { console.error(error.stack || error); process.exitCode = 1; });
