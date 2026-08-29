const targets = await (await fetch("http://127.0.0.1:9222/json/list")).json();
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.title === "ChatGPT" &&
    candidate.url === "app://-/index.html",
);
if (!target) throw new Error("ChatGPT app://-/index.html target not found");

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});

let nextId = 1;
const pending = new Map();
socket.addEventListener("message", ({ data }) => {
  const message = JSON.parse(data);
  if (!message.id) return;
  const callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  if (message.error) callback.reject(new Error(JSON.stringify(message.error)));
  else callback.resolve(message.result);
});

function send(method, params = {}) {
  const id = nextId++;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result.value;
}

await Promise.all([send("Runtime.enable"), send("DOM.enable"), send("CSS.enable")]);

await evaluate(`(() => {
  const newChat = [...document.querySelectorAll('button')].find(
    (button) => (button.innerText || '').trim() === '新对话' &&
      button.getBoundingClientRect().top < 160
  );
  newChat?.click();
  return Boolean(newChat);
})()`);

const inputDeadline = Date.now() + 10_000;
while (Date.now() < inputDeadline) {
  if (await evaluate(`Boolean(document.querySelector('[role="textbox"][contenteditable="true"]'))`)) {
    break;
  }
  await new Promise((resolve) => setTimeout(resolve, 25));
}

await evaluate(`document.querySelector('[role="textbox"][contenteditable="true"]').focus()`);
await send("Input.insertText", { text: "hello" });
await send("Input.dispatchKeyEvent", {
  type: "keyDown",
  key: "Enter",
  code: "Enter",
  windowsVirtualKeyCode: 13,
  nativeVirtualKeyCode: 36,
});
await send("Input.dispatchKeyEvent", {
  type: "keyUp",
  key: "Enter",
  code: "Enter",
  windowsVirtualKeyCode: 13,
  nativeVirtualKeyCode: 36,
});

const thinkingDeadline = Date.now() + 15_000;
while (Date.now() < thinkingDeadline) {
  if (await evaluate(`[
    ...document.querySelectorAll('.loading-shimmer-pure-text')
  ].some((element) => (element.textContent || '').includes('正在思考'))`)) break;
  await new Promise((resolve) => setTimeout(resolve, 10));
}

const computed = await evaluate(`(() => {
  const element = [...document.querySelectorAll('.loading-shimmer-pure-text')]
    .find((candidate) => (candidate.textContent || '').includes('正在思考'));
  if (!element) return null;
  const style = getComputedStyle(element);
  const rect = element.getBoundingClientRect();
  const serializeStyle = (value) => ({
    color: value.color,
    background: value.background,
    backgroundColor: value.backgroundColor,
    backgroundImage: value.backgroundImage,
    backgroundPosition: value.backgroundPosition,
    backgroundSize: value.backgroundSize,
    backgroundClip: value.backgroundClip,
    webkitBackgroundClip: value.webkitBackgroundClip,
    webkitTextFillColor: value.webkitTextFillColor,
    opacity: value.opacity,
    animation: value.animation,
    animationName: value.animationName,
    animationDuration: value.animationDuration,
    animationTimingFunction: value.animationTimingFunction,
    animationIterationCount: value.animationIterationCount,
    animationDirection: value.animationDirection,
    animationDelay: value.animationDelay,
    transform: value.transform,
    position: value.position,
    inset: value.inset,
    overflow: value.overflow,
    width: value.width,
    height: value.height,
    clipPath: value.clipPath,
    maskImage: value.maskImage,
  });
  const descendants = [...element.querySelectorAll('*')].map((child) => {
    const childStyle = getComputedStyle(child);
    const childRect = child.getBoundingClientRect();
    return {
      className: child.className,
      text: child.textContent,
      rect: {x:childRect.x,y:childRect.y,width:childRect.width,height:childRect.height},
      style: serializeStyle(childStyle),
    };
  });
  const rules = [];
  for (const sheet of document.styleSheets) {
    try {
      for (const rule of sheet.cssRules || []) {
        const cssText = rule.cssText || '';
        if (cssText.includes('cadencedShimmer')) rules.push(cssText);
      }
    } catch {}
  }
  return {
    text: element.textContent,
    outerHTML: element.outerHTML,
    parentHTML: element.parentElement?.outerHTML.slice(0, 12000),
    rect: {x:rect.x,y:rect.y,width:rect.width,height:rect.height},
    fontFamily: style.fontFamily,
    fontSize: style.fontSize,
    fontWeight: style.fontWeight,
    fontStyle: style.fontStyle,
    fontStretch: style.fontStretch,
    lineHeight: style.lineHeight,
    letterSpacing: style.letterSpacing,
    fontFeatureSettings: style.fontFeatureSettings,
    fontVariationSettings: style.fontVariationSettings,
    textRendering: style.textRendering,
    webkitFontSmoothing: style.webkitFontSmoothing,
    color: style.color,
    style: serializeStyle(style),
    before: serializeStyle(getComputedStyle(element, '::before')),
    after: serializeStyle(getComputedStyle(element, '::after')),
    descendants,
    rules,
    animations: element.getAnimations({subtree:true}).map((animation) => ({
      targetClassName: animation.effect?.target?.className,
      playState: animation.playState,
      currentTime: animation.currentTime,
      startTime: animation.startTime,
      playbackRate: animation.playbackRate,
      timing: animation.effect?.getComputedTiming(),
      keyframes: animation.effect?.getKeyframes(),
    })),
  };
})()`);

const cadenceSamples = await evaluate(`(async () => {
  const element = [...document.querySelectorAll('.loading-shimmer-pure-text')]
    .find((candidate) => (candidate.textContent || '').includes('正在思考'));
  if (!element) return [];
  const started = performance.now();
  const samples = [];
  let previous = '';
  while (performance.now() - started < 4200) {
    const sweep = element.querySelector('[class*="cadencedShimmerSweep"]');
    const highlight = element.querySelector('[class*="cadencedShimmerHighlight"]');
    const value = [element.className, sweep && getComputedStyle(sweep).transform,
      highlight && getComputedStyle(highlight).transform].join('|');
    if (value !== previous) {
      samples.push({t: Math.round(performance.now() - started), value});
      previous = value;
    }
    await new Promise(requestAnimationFrame);
  }
  return samples;
})()`);

let platformFonts = [];
if (computed) {
  const documentNode = await send("DOM.getDocument", { depth: -1, pierce: true });
  const selected = await send("DOM.querySelector", {
    nodeId: documentNode.root.nodeId,
    selector: ".loading-shimmer-pure-text",
  });
  if (selected.nodeId) {
    const fonts = await send("CSS.getPlatformFontsForNode", { nodeId: selected.nodeId });
    platformFonts = fonts.fonts;
  }
}

console.log(JSON.stringify({ target: { title: target.title, url: target.url }, computed, cadenceSamples, platformFonts }, null, 2));
socket.close();
