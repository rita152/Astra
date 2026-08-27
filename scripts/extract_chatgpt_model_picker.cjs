const CDP_HTTP = process.env.CHATGPT_CDP_HTTP || "http://127.0.0.1:9222";
const fs = require("node:fs");

class CDP {
  constructor(url) {
    this.nextId = 1;
    this.pending = new Map();
    this.ws = new WebSocket(url);
  }

  async open() {
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", ({ data }) => {
      const message = JSON.parse(data);
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(JSON.stringify(message.error)));
      else pending.resolve(message.result);
    });
  }

  send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  }

  close() { this.ws.close(); }
}

async function main() {
const targets = await fetch(`${CDP_HTTP}/json/list`).then((response) => response.json());
const target = targets.find((item) => item.type === "page" && item.url === "app://-/index.html");
if (!target) throw new Error("ChatGPT main page target not found");

const cdp = new CDP(target.webSocketDebuggerUrl);
await cdp.open();

const prepare = `(() => {
  const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
  const textNode = (label) => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const parent = walker.currentNode.parentElement;
      if (walker.currentNode.textContent.trim() === label && visible(parent)
          && !parent.closest('[aria-hidden="true"],[inert]')) return walker.currentNode;
    }
    return null;
  };
  const clickable = (label) => {
    const node = textNode(label);
    return node?.parentElement?.closest('button,[role="button"],[role="menuitem"],[tabindex]') || node?.parentElement;
  };
  const visible = (element) => {
    if (!element) return false;
    const r = element.getBoundingClientRect();
    const s = getComputedStyle(element);
    return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && s.display !== 'none';
  };
  const measure = (element) => {
    const r = element.getBoundingClientRect();
    const s = getComputedStyle(element);
    return {
      tag: element.tagName,
      role: element.getAttribute('role'),
      ariaLabel: element.getAttribute('aria-label'),
      ariaChecked: element.getAttribute('aria-checked'),
      ariaExpanded: element.getAttribute('aria-expanded'),
      text: element.innerText,
      className: String(element.className),
      rect: { x: r.x, y: r.y, width: r.width, height: r.height },
      style: {
        display: s.display, position: s.position, gap: s.gap,
        padding: s.padding, margin: s.margin, border: s.border,
        borderRadius: s.borderRadius, background: s.backgroundColor,
        color: s.color, fontFamily: s.fontFamily, fontSize: s.fontSize,
        fontWeight: s.fontWeight, lineHeight: s.lineHeight,
        boxShadow: s.boxShadow, opacity: s.opacity, transform: s.transform,
        alignItems: s.alignItems, justifyContent: s.justifyContent,
        overflow: s.overflow, width: s.width, height: s.height,
      },
      directText: [...element.childNodes].filter(node => node.nodeType === Node.TEXT_NODE)
        .map(node => node.textContent.trim()).filter(Boolean).join(' '),
      html: element.outerHTML.slice(0, 10000),
    };
  };
  const visibleDialogs = () => [...document.querySelectorAll('[role="menu"],[role="dialog"],[data-radix-popper-content-wrapper]')]
    .filter(visible).map(measure);
  const visibleItems = () => [...document.querySelectorAll('button,[role="menuitem"],[role="menuitemradio"],[role="option"]')]
    .filter(visible)
    .map(measure)
    .filter(item => item.rect.y > innerHeight * .35 || item.style.position === 'fixed');

  window.__modelPickerAudit = { textNode, clickable, visible, measure, visibleDialogs, visibleItems };
  return true;
})()`;
await cdp.evaluate(prepare);

async function pointFor(label) {
  return cdp.evaluate(`(() => {
    const e = window.__modelPickerAudit.clickable(${JSON.stringify(label)});
    if (!e) return null;
    const r = e.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  })()`);
}

async function clickLabel(label) {
  const point = await pointFor(label);
  if (!point) return false;
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...point });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...point });
  await new Promise((resolve) => setTimeout(resolve, 220));
  return true;
}

async function clickSelector(selector) {
  const point = await cdp.evaluate(`(() => {
    const e = document.querySelector(${JSON.stringify(selector)});
    if (!e || !window.__modelPickerAudit.visible(e)) return null;
    const r = e.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  })()`);
  if (!point) return false;
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...point });
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...point });
  await new Promise((resolve) => setTimeout(resolve, 320));
  return true;
}

async function hoverLabel(label) {
  const point = await pointFor(label);
  if (!point) return false;
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
  await new Promise((resolve) => setTimeout(resolve, 180));
  await cdp.evaluate(`(() => {
    const e = window.__modelPickerAudit.clickable(${JSON.stringify(label)});
    e?.focus();
    e?.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerType: 'mouse' }));
  })()`);
  await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "ArrowRight", code: "ArrowRight", windowsVirtualKeyCode: 39 });
  await new Promise((resolve) => setTimeout(resolve, 320));
  return true;
}

async function escape() {
  await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
  await new Promise((resolve) => setTimeout(resolve, 140));
}

async function snapshot() {
  return cdp.evaluate(`(() => {
    const a = window.__modelPickerAudit;
    const portals = [...document.body.children].filter(e => a.visible(e) && (e.innerText || '').trim()).map(a.measure);
    const relevant = [...document.querySelectorAll('*')]
      .filter(e => a.visible(e) && ['模型','推理强度','速度','高级'].includes((e.innerText || '').trim()))
      .map(e => a.measure(e));
    const menuItems = [...document.querySelectorAll('[role="menuitem"],[role="menuitemradio"],[data-radix-menu-content],button')]
      .filter(e => a.visible(e) && e.getBoundingClientRect().y > innerHeight - 700 && (e.innerText || '').trim())
      .map(e => a.measure(e));
    const sliderSelectors = [
      '[data-model-picker-power-slider]', '[data-model-picker-power-slider] [data-orientation="horizontal"]',
      '[data-model-picker-power-slider] [class*="_Track_"]',
      '[data-model-picker-power-slider] [class*="_Range_"]',
      '[data-model-picker-power-slider] [class*="_TickRail_"]',
      '[data-model-picker-power-slider] [class*="_Tick_"]',
      '[data-model-picker-power-slider] [class*="_Thumb_"]',
      '[data-model-picker-view-toggle]', '[data-fast-mode-enabled]'
    ];
    const slider = sliderSelectors.flatMap(selector => [...document.querySelectorAll(selector)])
      .filter((e, i, all) => a.visible(e) && all.indexOf(e) === i)
      .map(e => a.measure(e));
    const rootMenu = [...document.querySelectorAll('[role="menu"]')].find(a.visible);
    const tree = rootMenu ? [rootMenu, ...rootMenu.querySelectorAll('*')]
      .filter(a.visible)
      .map((e, index) => ({ index, parentIndex: e === rootMenu ? null : [rootMenu, ...rootMenu.querySelectorAll('*')].indexOf(e.parentElement), ...a.measure(e) })) : [];
    return { portals, relevant, menuItems, slider, tree };
  })()`);
}

await escape();
await escape();
const trigger = await cdp.evaluate(`(() => {
  const e = document.querySelector('[data-codex-intelligence-trigger]');
  return e ? window.__modelPickerAudit.measure(e) : null;
})()`);
await clickSelector('[data-codex-intelligence-trigger]');
if (await cdp.evaluate(`window.__modelPickerAudit.clickable('高级')?.getAttribute('aria-expanded') === 'true'`)) {
  await clickLabel("高级");
}
const requestedSpeed = process.argv.find(value => value.startsWith('--speed='))?.slice(8);
if (requestedSpeed || process.argv.includes('--audit-slider')) {
  const advancedOpen = await cdp.evaluate(
    `window.__modelPickerAudit.clickable('高级')?.getAttribute('aria-expanded') === 'true'`
  );
  if (!advancedOpen) await clickLabel('高级');
}
if (requestedSpeed) {
  const shouldBeFast = requestedSpeed === 'fast';
  const isFast = await cdp.evaluate(`document.querySelector('[data-fast-mode-enabled]')?.getAttribute('data-fast-mode-enabled') === 'true'`);
  if (isFast !== shouldBeFast) await clickSelector('[data-fast-mode-enabled]');
}
const primary = await snapshot();
const states = {};
if (process.argv.includes('--audit-slider')) {
  states.sliderPositions = [];
  for (let index = 0; index < 6; index++) {
      const sliderRect = await cdp.evaluate(`(() => {
        const e = document.querySelector('[data-model-picker-power-slider] [data-orientation="horizontal"]');
        const r = e?.getBoundingClientRect();
        return r ? { x: r.x, y: r.y, width: r.width, height: r.height } : null;
      })()`);
      if (!sliderRect) break;
      const x = sliderRect.x + 13 + (sliderRect.width - 26) * index / 5;
      const y = sliderRect.y + sliderRect.height / 2;
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', button: 'left', clickCount: 1, x, y });
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', button: 'left', clickCount: 1, x, y });
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 1, y: 1 });
      await new Promise(resolve => setTimeout(resolve, 420));
      states.sliderPositions.push(await cdp.evaluate(`(() => ({
        trigger: document.querySelector('[data-codex-intelligence-trigger]')?.innerText,
        status: document.querySelector('[data-reasoning-slider]')?.getAttribute('aria-describedby')
          ?.split(' ').map(id => document.getElementById(id)?.innerText).filter(Boolean),
        value: document.querySelector('[data-model-picker-power-slider] [role="slider"]')?.getAttribute('aria-valuenow'),
        root: (() => { const e = document.querySelector('[data-model-picker-power-slider] [data-orientation="horizontal"]'); return e ? {
          fastMode: e.getAttribute('data-fast-mode'), max: e.getAttribute('data-max'),
          endpointLabels: e.getAttribute('data-endpoint-labels-visible'), rect: window.__modelPickerAudit.measure(e)
        } : null })(),
        range: (() => { const e = document.querySelector('[data-model-picker-power-slider] [class*="_Range_"]'); return e ? window.__modelPickerAudit.measure(e) : null })(),
        thumb: (() => { const e = document.querySelector('[data-model-picker-power-slider] [class*="_Thumb_"]:not([role="slider"])'); return e ? window.__modelPickerAudit.measure(e) : null })(),
        ticks: [...document.querySelectorAll('[data-model-picker-power-slider] [class*="_Tick_"]')].map(window.__modelPickerAudit.measure),
        effects: [...document.querySelectorAll('[data-model-picker-power-slider] canvas,[data-model-picker-power-slider] [class*="_Fill_"],[data-model-picker-power-slider] [class*="_Mask_"],[data-model-picker-power-slider] [class*="_Particle"],[data-model-picker-power-slider] [class*="_MaxEffects_"]')]
          .filter(window.__modelPickerAudit.visible).map(window.__modelPickerAudit.measure),
        fastToggle: (() => { const e = document.querySelector('[data-fast-mode-enabled]'); return e ? {
          enabled: e.getAttribute('data-fast-mode-enabled'), checked: e.getAttribute('aria-checked'), rect: window.__modelPickerAudit.measure(e)
        } : null })()
      }))()`));
      const auditShots = process.argv.find(value => value.startsWith('--audit-slider-shots='))?.slice(21);
      if (auditShots) {
        const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
        fs.writeFileSync(`${auditShots}-${index}.png`, Buffer.from(shot.data, 'base64'));
      }
  }
}
const requested = process.argv.find((value) => value.startsWith('--submenu='))?.slice(10);
if (requested) {
  const key = requested === '高级' ? '高级展开' : requested;
  if (requested !== '高级' && !(await pointFor(requested))) {
    await clickLabel('高级');
  }
  const activated = requested === '高级'
    ? await clickLabel(requested)
    : ((await clickLabel(requested)) && (await hoverLabel(requested)));
  if (activated) states[key] = await snapshot();
  else states[key] = { error: 'row not found' };
}

const result = {
  viewport: await cdp.evaluate(`({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio })`),
  trigger,
  primary,
  states,
  sliderCss: await cdp.evaluate(`(() => {
    const needles = ['_Root_', '_Track_', '_Range_', '_Tick_', '_Thumb_', '_MaxEffects_', '_Fill_', '_Particle', '_TrackParticle_'];
    const matches = [];
    const visit = (rules) => {
      for (const rule of rules || []) {
        if (rule.cssRules) visit(rule.cssRules);
        const text = rule.cssText || '';
        if (needles.some(needle => text.includes(needle))) matches.push(text);
      }
    };
    for (const sheet of document.styleSheets) {
      try { visit(sheet.cssRules); } catch (_) {}
    }
    return [...new Set(matches)];
  })()`),
  sliderTokens: await cdp.evaluate(`(() => {
    const style = getComputedStyle(document.documentElement);
    const names = ['--color-chart-blue','--color-chart-purple','--color-codex-description','--color-border','--color-border-strong','--color-text'];
    return Object.fromEntries(names.map(name => [name, style.getPropertyValue(name).trim()]));
  })()`),
};
const screenshotPath = process.argv.find((value) => value.startsWith('--screenshot='))?.slice(13);
if (screenshotPath) {
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  fs.writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
}
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
cdp.close();
setTimeout(() => process.exit(0), 25);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
