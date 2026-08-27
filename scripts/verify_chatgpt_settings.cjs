#!/usr/bin/env node

const { app, BrowserWindow } = require("electron");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "chat-reference", "settings");
const themes = ["light", "dark"];
const failures = [];
const renderedColors = new Map();

function rgbChannels(color) {
  const match = color.match(/rgba?\(\s*([\d.]+)[, ]+\s*([\d.]+)[, ]+\s*([\d.]+)/i);
  return match ? match.slice(1, 4).map(Number) : null;
}

function luminance(color) {
  const channels = rgbChannels(color);
  return channels ? channels.reduce((sum, value) => sum + value, 0) / channels.length : NaN;
}

app.commandLine.appendSwitch("disable-gpu");

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    width: 1200,
    height: 800,
    show: false,
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });

  for (const theme of themes) {
    const files = fs.readdirSync(path.join(root, theme)).filter((name) => name.endsWith(".html")).sort();
    if (files.length !== 21) failures.push(`${theme}: expected 21 files, found ${files.length}`);
    for (const file of files) {
      const slug = path.basename(file, ".html");
      const fullPath = path.join(root, theme, file);
      const source = fs.readFileSync(fullPath, "utf8");
      if (source.includes("app://-/assets/")) failures.push(`${theme}/${file}: unresolved app asset URL`);
      for (const match of source.matchAll(/\.\.\/assets\/([A-Za-z0-9._-]+)/g)) {
        if (!fs.existsSync(path.join(root, "assets", match[1]))) {
          failures.push(`${theme}/${file}: missing local asset ${match[1]}`);
        }
      }
      await window.loadFile(fullPath);
      const result = await window.webContents.executeJavaScript(`(() => ({
        active: document.querySelector('[data-settings-panel-slug][aria-current="page"]')?.dataset.settingsPanelSlug || null,
        themeClass: document.documentElement.classList.contains(${JSON.stringify(theme)}),
        colorScheme: getComputedStyle(document.documentElement).colorScheme,
        colors: (() => {
          const probe = document.createElement('span');
          probe.style.position = 'fixed';
          probe.style.visibility = 'hidden';
          document.body.appendChild(probe);
          const resolve = (token) => {
            probe.style.color = 'var(' + token + ')';
            return getComputedStyle(probe).color;
          };
          const value = {
            surface: resolve('--color-background-surface'),
            text: resolve('--color-text-foreground')
          };
          probe.remove();
          return value;
        })(),
        styleSheets: document.styleSheets.length,
        bodyWidth: document.body.getBoundingClientRect().width,
        bodyHeight: document.body.getBoundingClientRect().height,
        textLength: document.body.innerText.length,
        looseBodyText: [...document.body.childNodes]
          .filter(node => node.nodeType === Node.TEXT_NODE)
          .map(node => node.textContent.trim()).filter(Boolean).join(' ')
      }))()`);
      if (process.env.SETTINGS_DEBUG_TOP_LEFT === "1" && theme === "light" && slug === "general-settings") {
        const topLeft = await window.webContents.executeJavaScript(`JSON.stringify(
          [[2,2],[7,7],[12,7],[18,7],[24,7],[32,7],[40,7]].map(([x,y]) => ({
            x, y,
            elements: document.elementsFromPoint(x,y).slice(0,6).map(element => ({
              tag: element.tagName,
              id: element.id,
              class: element.className,
              text: element.textContent?.trim().slice(0,40),
              before: getComputedStyle(element, '::before').content,
              after: getComputedStyle(element, '::after').content
            }))
          }))
        )`);
        console.log(topLeft);
      }
      if (result.active !== slug) failures.push(`${theme}/${file}: active=${result.active}`);
      if (!result.themeClass) failures.push(`${theme}/${file}: theme class missing`);
      if (!result.colorScheme.includes(theme)) failures.push(`${theme}/${file}: color-scheme=${result.colorScheme}`);
      renderedColors.set(`${theme}/${slug}`, result.colors);
      if (result.styleSheets < 2) failures.push(`${theme}/${file}: CSS did not load`);
      if (result.looseBodyText) failures.push(`${theme}/${file}: loose body text=${JSON.stringify(result.looseBodyText)}`);
      if (result.bodyWidth <= 0 || result.bodyHeight <= 0 || result.textLength < 100) {
        failures.push(`${theme}/${file}: empty render ${JSON.stringify(result)}`);
      }
    }
  }

  for (const file of fs.readdirSync(path.join(root, "light")).filter((name) => name.endsWith(".html"))) {
    const slug = path.basename(file, ".html");
    const light = renderedColors.get(`light/${slug}`);
    const dark = renderedColors.get(`dark/${slug}`);
    if (!light || !dark) continue;
    if (!(luminance(light.surface) > luminance(dark.surface) + 100)) {
      failures.push(`${slug}: light/dark surfaces are not visually distinct (${light.surface} vs ${dark.surface})`);
    }
    if (!(luminance(light.text) < luminance(dark.text) - 100)) {
      failures.push(`${slug}: light/dark text colors are not visually distinct (${light.text} vs ${dark.text})`);
    }
  }

  window.destroy();
  if (failures.length) {
    console.error(failures.join("\n"));
    app.exit(1);
  } else {
    const sampleLight = renderedColors.get("light/general-settings");
    const sampleDark = renderedColors.get("dark/general-settings");
    console.log("Verified 42/42 settings snapshots in Electron (21 light, 21 dark).");
    console.log(`Computed colors — light: surface ${sampleLight.surface}, text ${sampleLight.text}; dark: surface ${sampleDark.surface}, text ${sampleDark.text}.`);
    app.exit(0);
  }
}).catch((error) => {
  console.error(error.stack || error);
  app.exit(1);
});
