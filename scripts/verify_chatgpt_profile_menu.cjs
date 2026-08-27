#!/usr/bin/env node

const { app, BrowserWindow } = require("electron");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "chat-reference", "profile-menu");
const failures = [];
const colors = {};
const menuRects = {};

function luminance(color) {
  const values = color.match(/[\d.]+/g)?.slice(0, 3).map(Number);
  return values ? values.reduce((sum, value) => sum + value, 0) / 3 : NaN;
}

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    // Deliberately smaller than the 2560x1410 capture viewport so stale
    // popper coordinates cannot pass verification.
    width: 1024, height: 600, show: false,
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  for (const theme of ["light", "dark"]) {
    const file = path.join(root, `${theme}.html`);
    const source = fs.readFileSync(file, "utf8");
    if (/<script\b|Content-Security-Policy|app:\/\/-\/assets\//i.test(source)) {
      failures.push(`${theme}: unsafe or unresolved source reference`);
    }
    await window.loadFile(file);
    const result = await window.webContents.executeJavaScript(`(() => {
      const menu = [...document.querySelectorAll('[role="menu"]')].find(element => {
        const rect = element.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      });
      const trigger = document.querySelector('button[aria-label="打开个人资料菜单"]');
      const probe = document.createElement('span');
      probe.style.cssText = 'position:fixed;visibility:hidden;color:var(--color-background-surface)';
      document.body.appendChild(probe);
      const surface = getComputedStyle(probe).color;
      probe.remove();
      const rect = menu?.getBoundingClientRect();
      return {
        menuText: menu?.innerText || '', menuRect: rect ? {x:rect.x,y:rect.y,w:rect.width,h:rect.height} : null,
        viewport: {width: innerWidth, height: innerHeight},
        triggerOpen: trigger?.dataset.state === 'open' && trigger?.getAttribute('aria-expanded') === 'true',
        themeClass: document.documentElement.classList.contains(${JSON.stringify(theme)}),
        colorScheme: getComputedStyle(document.documentElement).colorScheme,
        surface, styleSheets: document.styleSheets.length,
        looseText: [...document.body.childNodes].filter(node => node.nodeType === Node.TEXT_NODE)
          .map(node => node.textContent.trim()).filter(Boolean).join(' ')
      };
    })()`);
    colors[theme] = result.surface;
    menuRects[theme] = { ...result.menuRect, viewport: result.viewport };
    if (!result.menuText.includes("设置") || !result.menuText.includes("退出登录")) failures.push(`${theme}: open menu content missing`);
    if (!result.menuRect || result.menuRect.x < 0 || result.menuRect.x > 400 ||
        result.menuRect.y < 0 || result.menuRect.x + result.menuRect.w > result.viewport.width ||
        result.menuRect.y + result.menuRect.h > result.viewport.height) {
      failures.push(`${theme}: menu is outside viewport (${JSON.stringify(result.menuRect)} in ${JSON.stringify(result.viewport)})`);
    }
    if (!result.triggerOpen) failures.push(`${theme}: profile trigger is not open`);
    if (!result.themeClass || !result.colorScheme.includes(theme)) failures.push(`${theme}: theme class/scheme mismatch`);
    if (result.styleSheets < 2) failures.push(`${theme}: CSS did not load`);
    if (result.looseText) failures.push(`${theme}: loose text ${JSON.stringify(result.looseText)}`);
  }
  window.destroy();
  if (!(luminance(colors.light) > luminance(colors.dark) + 100)) failures.push(`themes are not visually distinct: ${JSON.stringify(colors)}`);
  if (failures.length) {
    console.error(failures.join("\n"));
    app.exit(1);
  } else {
    console.log(`Verified profile menu snapshots. light=${colors.light}; dark=${colors.dark}`);
    console.log(`Menu bounds at 1024x600: ${JSON.stringify(menuRects)}`);
    app.exit(0);
  }
}).catch((error) => { console.error(error.stack || error); app.exit(1); });
