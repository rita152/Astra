const { app, BrowserWindow, nativeTheme } = require("electron");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

function argument(name, fallback) {
  const prefix = `--${name}=`;
  const value = process.argv.find((item) => item.startsWith(prefix));
  return value ? value.slice(prefix.length) : fallback;
}

const theme = argument("theme", "dark") === "light" ? "light" : "dark";
const sidebarScrollTop = Number(argument("sidebar-scroll", "0"));
const output = path.resolve(argument("output", `artifacts/reference-${theme}.png`));
nativeTheme.themeSource = theme;

// Request a 2× source frame. Chromium can still follow the active monitor on
// macOS, so the asset pipeline detects the actual size and handles 1× or 2×.
app.commandLine.appendSwitch("force-device-scale-factor", "2");
app.commandLine.appendSwitch("disable-features", "CalculateNativeWinOcclusion");

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    useContentSize: true,
    show: false,
    frame: true,
    title: "Codex",
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 18, y: 18 },
    backgroundColor: theme === "dark" ? "#181818" : "#ffffff",
    webPreferences: {
      backgroundThrottling: false,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  window.setMenuBarVisibility(false);
  await window.loadFile(path.resolve("chat-reference/chat.html"));
  await window.webContents.executeJavaScript(`
    (() => {
      const style = document.createElement('style');
      style.id = 'pixel-capture-motion-reset';
      style.textContent = '*,*::before,*::after{animation:none!important;transition:none!important}';
      document.head.appendChild(style);
    })();
    window.applyTheme(${JSON.stringify(theme)});
    (() => {
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      let node;
      while (walker.nextNode()) {
        if (walker.currentNode.textContent.trim() === '获得 250 额度') {
          node = walker.currentNode.parentElement;
          break;
        }
      }
      node?.closest('aside')?.remove();
    })();
    document.fonts.ready.then(() => {
      const scroller = [...document.querySelectorAll('aside *')].find((element) =>
        ['auto', 'scroll'].includes(getComputedStyle(element).overflowY));
      if (scroller) scroller.scrollTop = ${sidebarScrollTop};
      return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
  `);
  // A hidden macOS Chromium surface can retain the frame produced before the
  // class switch. Showing it without activation commits the themed frame to
  // the compositor while keeping keyboard focus in the caller.
  window.showInactive();
  await new Promise((resolve) => setTimeout(resolve, 120));
  // Keep :hover deterministic. A hidden/inactive Chromium window otherwise
  // inherits the host cursor's last screen position, which can accidentally
  // hover the theme switch or another control between capture runs.
  window.webContents.sendInputEvent({ type: "mouseMove", x: 720, y: 450 });
  await new Promise((resolve) => setTimeout(resolve, 32));

  const metrics = await window.webContents.executeJavaScript(`({
    width: innerWidth,
    height: innerHeight,
    dpr: devicePixelRatio,
    theme: document.documentElement.className,
    colorScheme: getComputedStyle(document.documentElement).colorScheme,
    surface: getComputedStyle(document.documentElement).getPropertyValue('--color-background-surface').trim(),
    bodyBackground: getComputedStyle(document.body).backgroundColor,
    layout: (() => {
      const measure = (selector) => {
        const element = document.querySelector(selector);
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return {
          x: rect.x, y: rect.y, width: rect.width, height: rect.height,
          fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight,
          background: style.backgroundColor
        };
      };
      const measureText = (label, occurrence = 0) => {
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        const matches = [];
        while (walker.nextNode()) {
          if (walker.currentNode.textContent.trim() === label) matches.push(walker.currentNode);
        }
        const node = matches[occurrence];
        if (!node) return null;
        const range = document.createRange();
        range.selectNodeContents(node);
        const rect = range.getBoundingClientRect();
        const style = getComputedStyle(node.parentElement);
        return {
          x: rect.x, y: rect.y, width: rect.width, height: rect.height,
          fontFamily: style.fontFamily, fontSize: style.fontSize,
          lineHeight: style.lineHeight, fontWeight: style.fontWeight,
          color: style.color, opacity: style.opacity
        };
      };
      const scrollAncestor = (label) => {
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        let node;
        while (walker.nextNode()) {
          if (walker.currentNode.textContent.trim() === label) { node = walker.currentNode.parentElement; break; }
        }
        while (node && !['auto', 'scroll'].includes(getComputedStyle(node).overflowY)) node = node.parentElement;
        if (!node) return null;
        const rect = node.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height,
          scrollHeight: node.scrollHeight, scrollTop: node.scrollTop,
          overflowY: getComputedStyle(node).overflowY };
      };
      const ancestors = (label, limit = 6) => {
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        let node;
        while (walker.nextNode()) {
          if (walker.currentNode.textContent.trim() === label) { node = walker.currentNode.parentElement; break; }
        }
        const result = [];
        while (node && result.length < limit) {
          const rect = node.getBoundingClientRect();
          const style = getComputedStyle(node);
          result.push({ tag: node.tagName, className: String(node.className).slice(0, 180),
            x: rect.x, y: rect.y, width: rect.width, height: rect.height,
            display: style.display, gap: style.gap, padding: style.padding,
            background: style.backgroundColor, borderRadius: style.borderRadius });
          node = node.parentElement;
        }
        return result;
      };
      return {
        sidebar: measure('aside.app-shell-left-panel'),
        heading: measure('.heading-xl'),
        composer: measure('[data-codex-composer-root] ._ComposerLayoutBody_f4zzl_2'),
        themeSwitch: measure('#codex-theme-switch'),
        sidebarScroll: scrollAncestor('oh-my-pi'),
        ancestors: {
          suggestion: ancestors('Prove plugin upgrades never mutate an active run'),
          credits: ancestors('获得 250 额度', 8),
          utility: ancestors('本地', 8),
          permission: ancestors('完全访问', 8),
          model: ancestors('5.6 Sol', 8)
        },
        text: {
          codex: measureText('Codex'),
          newChat: measureText('新对话'),
          projects: measureText('项目'),
          projectName: measureText('oh-my-pi'),
          projectChild: measureText('分析项目中的 Web 工具'),
          selectedProject: measureText('coda', 0),
          selectedChild: measureText('统一外挂能力包使用体验'),
          suggestion: measureText('Prove plugin upgrades never mutate an active run'),
          credits: measureText('获得 250 额度'),
          creditsDetail: measureText('邀请好友使用 ChatGPT 桌面版。对方发送第一条消息后，你们各获得 250 额度。'),
          increaseCredits: measureText('增加额度'),
          recommend: measureText('推荐'),
          utilityProject: measureText('coda', 1),
          utilityLocal: measureText('本地'),
          utilityBranch: measureText('main'),
          permission: measureText('完全访问'),
          model: measureText('5.6 Sol'),
          effort: measureText('极高'),
          placeholder: measureText('随心输入'),
          darkTheme: measureText('深色')
        }
      };
    })()
  })`);
  if (metrics.width !== 1440 || metrics.height !== 900) {
    throw new Error(`unexpected render metrics: ${JSON.stringify(metrics)}`);
  }

  fs.mkdirSync(path.dirname(output), { recursive: true });
  let image = await window.webContents.capturePage({ x: 0, y: 0, width: 1440, height: 900 });
  const capturedSize = image.getSize();
  const rawOutput = output.replace(/\.png$/i, "-raw.png");
  const png = image.toPNG();
  fs.writeFileSync(output, png);
  fs.writeFileSync(rawOutput, png);
  if (capturedSize.width !== 1440 || capturedSize.height !== 900) {
    execFileSync("/usr/bin/sips", ["--resampleHeightWidth", "900", "1440", output], {
      stdio: "ignore",
    });
  }
  process.stdout.write(`${JSON.stringify({ output, rawOutput, capturedSize, ...metrics })}\n`);
  window.destroy();
  app.quit();
}).catch((error) => {
  console.error(error);
  app.exit(1);
});
