const { app, BrowserWindow, nativeTheme } = require("electron");
const fs = require("node:fs");
const path = require("node:path");

function argument(name, fallback) {
  const prefix = `--${name}=`;
  const value = process.argv.find((item) => item.startsWith(prefix));
  return value ? value.slice(prefix.length) : fallback;
}

const requestedTheme = argument("theme", "dark");
if (!["light", "dark"].includes(requestedTheme)) {
  throw new Error(`Unsupported settings theme: ${requestedTheme}`);
}
const theme = requestedTheme;
const slug = argument("slug", "general-settings");
const output = path.resolve(argument("output", `artifacts/settings-reference-${slug}-${theme}.png`));
const manifestPath = path.resolve("chat-reference/settings/manifest.json");
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const panel = manifest.find((item) => item && item.slug === slug);
if (!panel) throw new Error(`Unknown settings slug: ${slug}`);
nativeTheme.themeSource = theme;
app.commandLine.appendSwitch("force-device-scale-factor", "1");
app.commandLine.appendSwitch("disable-features", "CalculateNativeWinOcclusion");

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    useContentSize: true,
    show: false,
    frame: true,
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 18, y: 18 },
    backgroundColor: theme === "dark" ? "#181818" : "#ffffff",
    webPreferences: { backgroundThrottling: false, contextIsolation: true, sandbox: true },
  });
  await window.loadFile(path.resolve(`chat-reference/settings/${theme}/${slug}.html`));
  await window.webContents.executeJavaScript(`document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))`);
  window.showInactive();
  await new Promise((resolve) => setTimeout(resolve, 80));
  // Keep every reference capture in the same non-interactive state. Without an
  // explicit move Chromium inherits the host pointer position and may randomly
  // apply a row hover style to whichever element happens to be underneath it.
  window.webContents.sendInputEvent({ type: "mouseMove", x: 1400, y: 20, movementX: 0, movementY: 0 });
  await window.webContents.executeJavaScript(`new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))`);
  const metrics = await window.webContents.executeJavaScript(`(() => {
    const box = (element) => {
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height,
        background: style.backgroundColor, color: style.color, border: style.borderColor,
        radius: style.borderRadius, padding: style.padding, gap: style.gap,
        fontFamily: style.fontFamily, fontWeight: style.fontWeight,
        fontSize: style.fontSize, lineHeight: style.lineHeight };
    };
    const text = (value, root = document.body) => {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      while (walker.nextNode()) if (walker.currentNode.textContent.trim() === value) return box(walker.currentNode.parentElement);
      return null;
    };
    const active = document.querySelector('[data-settings-panel-slug][aria-current="page"]');
    const card = [...document.querySelectorAll('.rounded-2xl')].find(element => getComputedStyle(element).borderTopWidth !== '0px');
    const shell = document.querySelector('.zoom-adjusted-viewport');
    const sidebar = document.querySelector('.app-shell-left-panel');
    const content = sidebar?.nextElementSibling || document.body;
    const rect = (element) => {
      if (!element) return null;
      const value = element.getBoundingClientRect();
      return { x: value.x, y: value.y, width: value.width, height: value.height };
    };
    const sections = [...content.querySelectorAll('section')].map((section) => ({
      rect: rect(section),
      header: rect(section.firstElementChild),
      body: rect(section.lastElementChild),
      label: section.innerText.trim().slice(0, 80),
    }));
    return { viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio }, shell: box(shell),
      sidebar: box(sidebar), content: box(content), active: box(active), activeSlug: active?.dataset.settingsPanelSlug || null, card: box(card), sections,
      heading: text(${JSON.stringify(panel.label)}, content), headingLabel: ${JSON.stringify(panel.label)},
      sampleRow: text('默认权限'), colors: {
        surface: getComputedStyle(document.documentElement).getPropertyValue('--color-background-surface').trim(),
        panel: getComputedStyle(document.documentElement).getPropertyValue('--color-background-panel').trim(),
        text: getComputedStyle(document.documentElement).getPropertyValue('--color-text-foreground').trim(),
        secondary: getComputedStyle(document.documentElement).getPropertyValue('--color-text-foreground-secondary').trim(),
        border: getComputedStyle(document.documentElement).getPropertyValue('--color-border').trim()
      } };
  })()`);
  if (metrics.viewport.width !== 1440 || metrics.viewport.height !== 900 || metrics.viewport.dpr !== 1) {
    throw new Error(
      `Unexpected Electron viewport for ${theme}/${slug}: ` +
      `${metrics.viewport.width}x${metrics.viewport.height} @ ${metrics.viewport.dpr}x`,
    );
  }
  if (metrics.activeSlug !== slug) {
    throw new Error(`Active settings slug mismatch: expected ${slug}, found ${metrics.activeSlug}`);
  }
  if (!metrics.heading) {
    throw new Error(`Could not locate content heading ${JSON.stringify(panel.label)} for ${theme}/${slug}`);
  }
  const image = await window.webContents.capturePage({ x: 0, y: 0, width: 1440, height: 900 });
  const capturedSize = image.getSize();
  if (capturedSize.width !== 1440 || capturedSize.height !== 900) {
    throw new Error(
      `Unexpected Electron capture size for ${theme}/${slug}: ` +
      `${capturedSize.width}x${capturedSize.height}`,
    );
  }
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, image.toPNG());
  console.log(JSON.stringify({ output, capture: capturedSize, ...metrics }));
  window.destroy();
  app.quit();
}).catch((error) => { console.error(error); app.exit(1); });
