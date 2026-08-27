#!/usr/bin/env node

const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const crypto = require("node:crypto");

const CDP_HTTP = process.env.CHATGPT_CDP_HTTP || "http://127.0.0.1:9222";
const ROOT = path.resolve(__dirname, "..");
const OUTPUT = path.join(ROOT, "chat-reference", "settings");
const CSS_DIR = path.join(OUTPUT, "css");

const EXPECTED_PANELS = [
  ["general-settings", "常规"],
  ["import", "导入"],
  ["profile", "个人资料"],
  ["appearance", "外观"],
  ["voice", "语音"],
  ["agent", "配置"],
  ["personalization", "个性化"],
  ["pets", "智能伙伴"],
  ["keyboard-shortcuts", "键盘快捷键"],
  ["usage", "使用情况和计费"],
  ["computer-use", "电脑操控"],
  ["chronicle", "计算机历史记录"],
  ["appshots", "应用快照"],
  ["plugins-settings", "插件"],
  ["browser-use", "浏览器"],
  ["hooks-settings", "钩子"],
  ["connections", "连接"],
  ["git-settings", "Git"],
  ["local-environments", "环境"],
  ["worktrees", "Worktrees"],
  ["data-controls", "已归档的聊天"],
];

function getJson(url) {
  return new Promise((resolve, reject) => {
    http.get(url, (response) => {
      let body = "";
      response.setEncoding("utf8");
      response.on("data", (chunk) => (body += chunk));
      response.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch (error) {
          reject(new Error(`Invalid JSON from ${url}: ${error.message}`));
        }
      });
    }).on("error", reject);
  });
}

class CDP {
  constructor(url) {
    this.socket = new WebSocket(url);
    this.nextId = 0;
    this.pending = new Map();
    this.socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const callback = this.pending.get(message.id);
      if (callback) callback(message);
      this.pending.delete(message.id);
    };
  }

  async open() {
    await new Promise((resolve, reject) => {
      this.socket.onopen = resolve;
      this.socket.onerror = reject;
    });
  }

  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.nextId;
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP command timed out: ${method}`));
      }, 60_000);
      this.pending.set(id, (message) => {
        clearTimeout(timeout);
        if (message.error) reject(new Error(`${method}: ${message.error.message}`));
        else resolve(message.result);
      });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression, { awaitPromise = false } = {}) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    }
    return result.result.value;
  }

  close() {
    this.socket.close();
  }
}

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
const digest = (value) => crypto.createHash("sha256").update(value).digest("hex");

async function pageState(cdp) {
  const json = await cdp.evaluate(`JSON.stringify((() => {
    const active = document.querySelector('[data-settings-panel-slug][aria-current="page"]');
    const busySelectors = [
      '[aria-busy="true"]',
      '[data-loading="true"]',
      '[data-state="loading"]',
      '[role="progressbar"]'
    ];
    const visible = (element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return style.display !== 'none' && style.visibility !== 'hidden' && rect.width > 0 && rect.height > 0;
    };
    const busy = busySelectors.flatMap(selector => [...document.querySelectorAll(selector)])
      .filter(visible)
      // Quota/usage meters are determinate progress bars and remain visible
      // after loading. Only indeterminate progress bars count as busy.
      .filter(element => element.getAttribute('role') !== 'progressbar' ||
        (!element.hasAttribute('aria-valuenow') && !element.hasAttribute('aria-valuetext')))
      .length;
    const text = document.body?.innerText || '';
    return {
      active: active?.dataset.settingsPanelSlug || null,
      readyState: document.readyState,
      fonts: document.fonts?.status || 'loaded',
      incompleteImages: [...document.images].filter(image => !image.complete).length,
      busy,
      text,
      html: document.body?.innerHTML || ''
    };
  })())`);
  const state = JSON.parse(json);
  state.hash = digest(`${state.text}\n${state.html}`);
  delete state.html;
  return state;
}

async function waitForStablePanel(cdp, slug) {
  const started = Date.now();
  let priorHash = null;
  let stableSamples = 0;
  let lastState = null;

  while (Date.now() - started < 45_000) {
    const state = await pageState(cdp);
    lastState = state;
    const ready = state.active === slug && state.readyState === "complete" &&
      state.fonts === "loaded" && state.incompleteImages === 0 && state.busy === 0;

    if (ready && state.hash === priorHash) stableSamples += 1;
    else stableSamples = 0;
    priorHash = state.hash;

    // Four equal samples 350 ms apart, after all explicit loading signals clear.
    if (stableSamples >= 3) {
      await cdp.evaluate(`new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`, {
        awaitPromise: true,
      });
      return { ...state, waitMs: Date.now() - started, stableSamples: stableSamples + 1 };
    }
    await delay(350);
  }

  throw new Error(`Panel ${slug} did not reach a stable loaded state: ${JSON.stringify(lastState)}`);
}

async function snapshotDocument(cdp) {
  const value = await cdp.evaluate(`(async () => {
    await document.fonts.ready;
    const originalImages = [...document.images];
    const clone = document.documentElement.cloneNode(true);
    const cloneImages = [...clone.querySelectorAll('img')];
    for (let index = 0; index < originalImages.length; index += 1) {
      const source = originalImages[index].currentSrc || originalImages[index].src;
      if (!source || source.startsWith('data:')) continue;
      try {
        const response = await fetch(source);
        const blob = await response.blob();
        const dataUrl = await new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(reader.result);
          reader.onerror = reject;
          reader.readAsDataURL(blob);
        });
        cloneImages[index].setAttribute('src', dataUrl);
        cloneImages[index].removeAttribute('srcset');
      } catch (_) {
        cloneImages[index].setAttribute('src', source);
      }
    }
    return JSON.stringify({
      html: clone.outerHTML,
      title: document.title,
      url: location.href,
      viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
      active: document.querySelector('[data-settings-panel-slug][aria-current="page"]')?.dataset.settingsPanelSlug || null
    });
  })()`, { awaitPromise: true });
  return JSON.parse(value);
}

function removeTag(html, tagName) {
  return html.replace(new RegExp(`<${tagName}\\b[^>]*>[\\s\\S]*?<\\/${tagName}>`, "gi"), "");
}

function buildStaticHtml(snapshot, panel, theme, waitInfo) {
  let html = snapshot.html;
  html = removeTag(html, "script");
  html = removeTag(html, "style");
  html = html.replace(/<meta\b[^>]*http-equiv=["']Content-Security-Policy["'][^>]*>/gi, "");
  html = html.replace(/<link\b[^>]*rel=["'](?:stylesheet|modulepreload|preload)["'][^>]*>/gi, "");
  html = html.replaceAll("app://-/assets/", "../assets/");
  // The running app writes its resolved dark palette directly on <html>.
  // Inline custom properties outrank the electron-light stylesheet, so simply
  // changing the class would leave a visually dark page. Preserve runtime
  // layout/type values, but remove palette overrides and let the requested
  // theme class supply the real color tokens.
  html = html.replace(/<html\b[^>]*>/i, (tag) => tag.replace(/\sstyle="([^"]*)"/i, (_style, declarations) => {
    const themeProperty = (name) =>
      name.startsWith("--color-") ||
      name.startsWith("--shadow-") ||
      ["--codex-base-contrast", "--codex-base-ink", "--codex-base-surface"].includes(name);
    const kept = declarations.split(";").map((value) => value.trim()).filter(Boolean).filter((declaration) => {
      const separator = declaration.indexOf(":");
      const name = separator < 0 ? declaration : declaration.slice(0, separator).trim();
      return !themeProperty(name);
    });
    return kept.length ? ` style="${kept.join("; ")};"` : "";
  }));
  html = html.replace(/<html\b([^>]*)class=["']([^"']*)["']([^>]*)>/i, (_match, before, classes, after) => {
    const next = classes.split(/\s+/).filter(Boolean)
      .filter((name) => !["dark", "light", "electron-dark", "electron-light"].includes(name));
    next.push(theme, `electron-${theme}`, "electron-opaque");
    return `<html${before}class="${[...new Set(next)].join(" ")}"${after}>`;
  });
  html = html.replace(/<title>[\s\S]*?<\/title>/i, `<title>${panel.label} · ChatGPT 设置 · ${theme}</title>`);

  const metadata = `<!--\nCDP stable snapshot\npanel: ${panel.slug} (${panel.label})\ntheme: ${theme}\nsource: ${snapshot.url}\nviewport: ${snapshot.viewport.width}x${snapshot.viewport.height} @ ${snapshot.viewport.dpr}x\nwait_ms: ${waitInfo.waitMs}\nstable_samples: ${waitInfo.stableSamples}\ncaptured_at: ${new Date().toISOString()}\n-->`;
  const links = `\n${metadata}\n<link rel="stylesheet" href="../css/settings-base.css">\n<link rel="stylesheet" href="../css/settings-static.css">`;
  html = html.replace(/<head([^>]*)>/i, `<head$1>${links}`);
  return `<!DOCTYPE html>\n${html}\n`;
}

async function collectCss(cdp) {
  return cdp.evaluate(`(() => {
    const chunks = [];
    for (const [index, sheet] of [...document.styleSheets].entries()) {
      try {
        chunks.push('/* stylesheet ' + index + ': ' + (sheet.href || 'inline') + ' */\\n' +
          [...sheet.cssRules].map(rule => rule.cssText).join('\\n'));
      } catch (error) {
        chunks.push('/* stylesheet ' + index + ' unavailable: ' + String(error) + ' */');
      }
    }
    return chunks.join('\\n\\n');
  })()`);
}

async function downloadAppAsset(cdp, url) {
  const dataUrl = await cdp.evaluate(`(async () => {
    const response = await fetch(${JSON.stringify(url)});
    if (!response.ok) throw new Error('HTTP ' + response.status + ' for ' + response.url);
    const blob = await response.blob();
    return await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = reject;
      reader.readAsDataURL(blob);
    });
  })()`, { awaitPromise: true });
  return Buffer.from(dataUrl.slice(dataUrl.indexOf(",") + 1), "base64");
}

function writeIndex(panels) {
  const rows = panels.map((panel) => `
      <tr><td>${panel.label}</td><td><code>${panel.slug}</code></td>
      <td><a href="light/${panel.slug}.html">light</a></td>
      <td><a href="dark/${panel.slug}.html">dark</a></td></tr>`).join("");
  const html = `<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>ChatGPT 设置页 CDP 快照</title><style>
:root{color-scheme:light dark;font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
body{max-width:900px;margin:48px auto;padding:0 24px}table{width:100%;border-collapse:collapse}
th,td{text-align:left;padding:10px 12px;border-bottom:1px solid color-mix(in srgb,currentColor 18%,transparent)}
a{color:#339cff}code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
</style></head><body><h1>ChatGPT 设置页 CDP 快照</h1>
<p>共 21 个独立设置子页；“账户”页已排除。每页分别提供 light / dark 静态 HTML。</p>
<table><thead><tr><th>页面</th><th>slug</th><th>浅色</th><th>深色</th></tr></thead><tbody>${rows}
</tbody></table></body></html>\n`;
  fs.writeFileSync(path.join(OUTPUT, "index.html"), html);
}

async function main() {
  const targets = await getJson(`${CDP_HTTP}/json/list`);
  const target = targets.find((item) => item.type === "page" && item.url === "app://-/index.html");
  if (!target) throw new Error("Could not find the main ChatGPT app://-/index.html CDP target");

  const cdp = new CDP(target.webSocketDebuggerUrl);
  await cdp.open();

  try {
    const discovered = JSON.parse(await cdp.evaluate(`JSON.stringify(
      [...document.querySelectorAll('[data-settings-panel-slug]')].map(button => [
        button.dataset.settingsPanelSlug,
        button.getAttribute('aria-label') || button.textContent.trim()
      ])
    )`));
    if (JSON.stringify(discovered) !== JSON.stringify(EXPECTED_PANELS)) {
      throw new Error(`Settings navigation changed. Discovered: ${JSON.stringify(discovered)}`);
    }

    fs.mkdirSync(path.join(OUTPUT, "light"), { recursive: true });
    fs.mkdirSync(path.join(OUTPUT, "dark"), { recursive: true });
    fs.mkdirSync(CSS_DIR, { recursive: true });

    const captures = [];
    const manifest = [];
    for (const [slug, label] of EXPECTED_PANELS) {
      process.stdout.write(`[${captures.length + 1}/21] ${label} (${slug}) ... `);
      const clicked = await cdp.evaluate(`(() => {
        const button = document.querySelector('[data-settings-panel-slug=${JSON.stringify(slug)}]');
        if (!button) return false;
        button.click();
        return true;
      })()`);
      if (!clicked) throw new Error(`Could not click settings panel ${slug}`);

      const waitInfo = await waitForStablePanel(cdp, slug);
      const snapshot = await snapshotDocument(cdp);
      if (snapshot.active !== slug) throw new Error(`Snapshot active panel mismatch for ${slug}`);
      captures.push({ panel: { slug, label }, snapshot, waitInfo });
      manifest.push({ slug, label, wait: waitInfo, viewport: snapshot.viewport });
      console.log(`stable after ${waitInfo.waitMs} ms (${waitInfo.text.length} text chars)`);
    }

    // Page-specific chunks are lazy-loaded. Collect CSS only after every panel
    // has been visited so the shared stylesheet is a superset for all snapshots.
    const css = await collectCss(cdp);
    fs.writeFileSync(path.join(CSS_DIR, "settings-base.css"), css);
    fs.writeFileSync(path.join(CSS_DIR, "settings-static.css"), `
html,body{width:100%;height:100%;margin:0}
*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}
html.light,html.electron-light{color-scheme:light}
html.dark,html.electron-dark{color-scheme:dark}
button,a,input,select,textarea,[contenteditable]{pointer-events:none!important}
`);

    const assetUrls = [...new Set(captures.flatMap(({ snapshot }) => {
      // Module preload and script references are removed from static pages;
      // only collect URLs that survive sanitization (media/inline styles).
      let staticSource = removeTag(removeTag(snapshot.html, "script"), "style");
      staticSource = staticSource.replace(/<link\b[^>]*rel=["'](?:stylesheet|modulepreload|preload)["'][^>]*>/gi, "");
      return staticSource.match(/app:\/\/-\/assets\/[A-Za-z0-9._-]+/g) || [];
    }))].sort();
    const assetDir = path.join(OUTPUT, "assets");
    fs.mkdirSync(assetDir, { recursive: true });
    const requiredAssets = new Set(assetUrls.map((url) => url.slice(url.lastIndexOf("/") + 1)));
    for (const filename of fs.readdirSync(assetDir)) {
      if (!requiredAssets.has(filename)) fs.unlinkSync(path.join(assetDir, filename));
    }
    for (const [index, url] of assetUrls.entries()) {
      const filename = url.slice(url.lastIndexOf("/") + 1);
      process.stdout.write(`[asset ${index + 1}/${assetUrls.length}] ${filename} ... `);
      const contents = await downloadAppAsset(cdp, url);
      fs.writeFileSync(path.join(assetDir, filename), contents);
      console.log(`${contents.length} bytes`);
    }

    for (const capture of captures) {
      for (const theme of ["light", "dark"]) {
        const html = buildStaticHtml(capture.snapshot, capture.panel, theme, capture.waitInfo);
        fs.writeFileSync(path.join(OUTPUT, theme, `${capture.panel.slug}.html`), html);
      }
    }
    manifest.push({
      generatedAt: new Date().toISOString(),
      targetId: target.id,
      source: target.url,
      panelCount: EXPECTED_PANELS.length,
      themes: ["light", "dark"],
      assetCount: assetUrls.length,
    });
    fs.writeFileSync(path.join(OUTPUT, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    writeIndex(EXPECTED_PANELS.map(([slug, label]) => ({ slug, label })));
    console.log(`Wrote 42 HTML snapshots plus index and CSS to ${OUTPUT}`);
  } finally {
    cdp.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exitCode = 1;
});
