const { app, BrowserWindow, nativeTheme } = require("electron");
const path = require("node:path");

function argument(name, fallback) {
  const prefix = `--${name}=`;
  const value = process.argv.find((item) => item.startsWith(prefix));
  return value ? value.slice(prefix.length) : fallback;
}

const theme = argument("theme", "dark") === "light" ? "light" : "dark";
const slug = argument("slug", "general-settings");
const scope = argument("scope", "content");

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
    webPreferences: {
      backgroundThrottling: false,
      contextIsolation: true,
      sandbox: true,
    },
  });

  await window.loadFile(path.resolve(`chat-reference/settings/${theme}/${slug}.html`));
  await window.webContents.executeJavaScript(`document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))`);

  const audit = await window.webContents.executeJavaScript(`(() => {
    const round = (value) => Math.round(value * 64) / 64;
    const compactText = (element) => [...element.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent.trim())
      .filter(Boolean)
      .join(" ")
      .replace(/\\s+/g, " ")
      .slice(0, 180);
    const aggregateText = (element) => element.textContent
      .trim()
      .replace(/\\s+/g, " ")
      .slice(0, 180);
    const sidebar = document.querySelector('.app-shell-left-panel');
    const root = ${JSON.stringify(scope)} === 'sidebar'
      ? (sidebar ?? document.body)
      : ${JSON.stringify(scope)} === 'all'
        ? document.body
        : (sidebar?.nextElementSibling ?? document.body);
    const rootDepth = (() => {
      let depth = 0;
      for (let node = root; node; node = node.parentElement) depth += 1;
      return depth;
    })();
    const rows = [];
    for (const element of root.querySelectorAll('*')) {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      if (rect.width <= 0 || rect.height <= 0 || style.visibility === 'hidden' || style.display === 'none') continue;
      let depth = 0;
      for (let node = element; node; node = node.parentElement) depth += 1;
      const directText = compactText(element);
      const hasVisual = style.backgroundColor !== 'rgba(0, 0, 0, 0)'
        || parseFloat(style.borderTopWidth) > 0
        || parseFloat(style.borderRightWidth) > 0
        || parseFloat(style.borderBottomWidth) > 0
        || parseFloat(style.borderLeftWidth) > 0;
      const isLeaf = element.children.length === 0;
      if (!directText && !hasVisual && !isLeaf) continue;
      rows.push({
        tag: element.tagName.toLowerCase(),
        depth: depth - rootDepth,
        text: directText || (isLeaf ? aggregateText(element) : ''),
        role: element.getAttribute('role'),
        aria: element.getAttribute('aria-label'),
        class: element.className?.toString().replace(/\\s+/g, ' ').slice(0, 240) || '',
        rect: [round(rect.x), round(rect.y), round(rect.width), round(rect.height)],
        style: {
          display: style.display,
          position: style.position,
          background: style.backgroundColor,
          color: style.color,
          border: [style.borderTopWidth, style.borderTopColor, style.borderRadius],
          padding: style.padding,
          gap: style.gap,
          font: [style.fontFamily, style.fontSize, style.fontWeight, style.lineHeight],
        },
      });
    }
    return { theme: ${JSON.stringify(theme)}, slug: ${JSON.stringify(slug)}, scope: ${JSON.stringify(scope)}, rows };
  })()`);

  process.stdout.write(`${JSON.stringify(audit, null, 2)}\n`);
  window.destroy();
  app.quit();
}).catch((error) => {
  console.error(error);
  app.exit(1);
});
