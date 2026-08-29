import fs from "node:fs";
import path from "node:path";

const CDP_HTTP = "http://127.0.0.1:9222";
const artifactDirectoryArgument = process.argv.find((argument) =>
  argument.startsWith("--artifact-dir="),
);
const ARTIFACT_DIR = path.resolve(
  artifactDirectoryArgument?.slice("--artifact-dir=".length) ||
    "artifacts/chatgpt-hello-flow",
);
fs.mkdirSync(ARTIFACT_DIR, { recursive: true });

const targets = await (await fetch(`${CDP_HTTP}/json/list`)).json();
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.title === "ChatGPT" &&
    candidate.url === "app://-/index.html",
);
if (!target) throw new Error("ChatGPT app://-/index.html target not found");

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 0;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (!message.id) return;
  const callback = pending.get(message.id);
  pending.delete(message.id);
  callback?.(message);
};

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, (message) => {
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
    });
    socket.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text);
  }
  return result.result.value;
}

const snapshotExpression = `(() => {
  const input = document.querySelector('[role="textbox"][contenteditable="true"]');
  const rect = (element) => {
    if (!element) return null;
    const value = element.getBoundingClientRect();
    return { x: value.x, y: value.y, width: value.width, height: value.height,
      top: value.top, right: value.right, bottom: value.bottom, left: value.left };
  };
  const style = (element) => {
    if (!element) return null;
    const value = getComputedStyle(element);
    const keys = [
      'display','position','boxSizing','flexDirection','alignItems','justifyContent','flexGrow','flexShrink',
      'width','height','minWidth','minHeight','maxWidth','maxHeight','paddingTop','paddingRight',
      'paddingBottom','paddingLeft','marginTop','marginRight','marginBottom','marginLeft','gap',
      'backgroundColor','color','borderTopWidth','borderRightWidth','borderBottomWidth','borderLeftWidth',
      'borderTopColor','borderRightColor','borderBottomColor','borderLeftColor','borderRadius','boxShadow',
      'fontFamily','fontSize','fontWeight','lineHeight','letterSpacing','textAlign','opacity','cursor',
      'outline','outlineColor','outlineWidth','transition','transform','overflowX','overflowY','zIndex'
    ];
    return Object.fromEntries(keys.map((key) => [key, value[key]]));
  };
  const node = (element, includeHtml = false) => {
    if (!element) return null;
    const result = {
      tag: element.tagName,
      id: element.id || null,
      className: typeof element.className === 'string' ? element.className : null,
      role: element.getAttribute('role'),
      ariaLabel: element.getAttribute('aria-label'),
      ariaPressed: element.getAttribute('aria-pressed'),
      ariaExpanded: element.getAttribute('aria-expanded'),
      dataState: element.getAttribute('data-state'),
      disabled: 'disabled' in element ? element.disabled : null,
      text: (element.innerText || element.value || '').slice(0, 1000),
      rect: rect(element),
      style: style(element),
    };
    if (includeHtml) result.outerHTML = element.outerHTML.slice(0, 12000);
    return result;
  };
  let composer = input;
  while (composer?.parentElement) {
    const candidate = composer.parentElement;
    const bounds = candidate.getBoundingClientRect();
    composer = candidate;
    if (bounds.width >= 700 && bounds.height >= 95 && bounds.height <= 150) break;
  }
  const buttons = composer ? [...composer.querySelectorAll('button')].map((button) => ({
    ...node(button, true),
    svg: button.querySelector('svg')?.outerHTML || null,
  })) : [];
  const ancestors = [];
  for (let current = input, depth = 0; current && depth < 8; current = current.parentElement, depth++) {
    ancestors.push(node(current, depth < 5));
  }
  const centralText = [...document.querySelectorAll('main *, [role="main"] *, body > div *')]
    .filter((element) => {
      const bounds = element.getBoundingClientRect();
      const text = (element.innerText || '').trim();
      return text && bounds.width > 0 && bounds.height > 0 && bounds.left > 270 &&
        bounds.top > 40 && bounds.bottom < (input?.getBoundingClientRect().top || innerHeight - 100) &&
        element.children.length <= 4;
    })
    .map((element) => node(element))
    .sort((a, b) => (a.rect.width * a.rect.height) - (b.rect.width * b.rect.height))
    .slice(0, 120);
  const exactHello = [...document.querySelectorAll('*')]
    .filter((element) => (element.innerText || '').trim() === 'hello' && element.getBoundingClientRect().left > 270)
    .map((element) => node(element, true))
    .sort((a, b) => (a.rect.width * a.rect.height) - (b.rect.width * b.rect.height))
    .slice(0, 12);
  const userBubbles = [...document.querySelectorAll('[data-user-message-bubble="true"]')]
    .map((element) => node(element, true));
  const assistantMessages = [...document.querySelectorAll('[data-markdown-text-style="assistant-message"]')]
    .map((element) => node(element, true));
  const finalAssistant = node(document.querySelector(
    '[data-local-conversation-final-assistant="true"] [data-markdown-text-style="assistant-message"]'
  ), true);
  return {
    capturedAt: performance.now(),
    viewport: { innerWidth, innerHeight, outerWidth, outerHeight, devicePixelRatio },
    activeElement: node(document.activeElement),
    body: node(document.body),
    input: node(input, true),
    composer: node(composer, true),
    ancestors,
    buttons,
    exactHello,
    userBubbles,
    assistantMessages,
    finalAssistant,
    centralText,
    mutationLog: (window.__gpuiCaptureMutationLog || []).slice(-300),
  };
})()`;

const samples = [];
async function snapshot(label) {
  const data = await evaluate(snapshotExpression);
  data.label = label;
  samples.push(data);
  fs.writeFileSync(
    path.join(ARTIFACT_DIR, `${label}.json`),
    JSON.stringify(data, null, 2),
  );
  const screenshot = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(
    path.join(ARTIFACT_DIR, `${label}.png`),
    Buffer.from(screenshot.data, "base64"),
  );
  return data;
}

async function inputRect() {
  return evaluate(`(() => {
    const r = document.querySelector('[role="textbox"][contenteditable="true"]').getBoundingClientRect();
    return { x:r.x, y:r.y, width:r.width, height:r.height };
  })()`);
}

async function dispatchMouse(type, x, y, button = "none", buttons = 0) {
  await send("Input.dispatchMouseEvent", {
    type,
    x,
    y,
    button,
    buttons,
    clickCount: type === "mousePressed" || type === "mouseReleased" ? 1 : 0,
  });
}

async function key(type, keyValue, code, modifiers = 0) {
  await send("Input.dispatchKeyEvent", {
    type,
    key: keyValue,
    code,
    windowsVirtualKeyCode: keyValue === "Enter" ? 13 : keyValue === "Escape" ? 27 : 9,
    nativeVirtualKeyCode: keyValue === "Enter" ? 36 : keyValue === "Escape" ? 53 : 48,
    modifiers,
  });
}

await send("Page.enable");
await send("Runtime.enable");

const hasExistingConversation = await evaluate(
  `Boolean(document.querySelector('[data-user-message-bubble="true"], [data-markdown-text-style="assistant-message"]'))`,
);
if (hasExistingConversation) {
  const newChatRect = await evaluate(`(() => {
    const button = [...document.querySelectorAll('button')].find((candidate) =>
      (candidate.innerText || '').trim() === '新对话' && candidate.getBoundingClientRect().top < 160
    );
    if (!button) return null;
    const rect = button.getBoundingClientRect();
    return {x:rect.x,y:rect.y,width:rect.width,height:rect.height};
  })()`);
  if (!newChatRect) throw new Error("Could not find the New chat button");
  await dispatchMouse(
    "mousePressed",
    newChatRect.x + newChatRect.width / 2,
    newChatRect.y + newChatRect.height / 2,
    "left",
    1,
  );
  await dispatchMouse(
    "mouseReleased",
    newChatRect.x + newChatRect.width / 2,
    newChatRect.y + newChatRect.height / 2,
    "left",
    0,
  );
  const resetDeadline = Date.now() + 10_000;
  while (Date.now() < resetDeadline) {
    if (await evaluate(`Boolean(document.querySelector('[role="textbox"][contenteditable="true"]'))`)) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

await evaluate(`(() => {
  window.__gpuiCaptureMutationLog = [];
  window.__gpuiCaptureObserver?.disconnect();
  window.__gpuiCaptureObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      const target = mutation.target.nodeType === Node.ELEMENT_NODE ? mutation.target : mutation.target.parentElement;
      const rect = target?.getBoundingClientRect?.();
      window.__gpuiCaptureMutationLog.push({
        time: performance.now(), type: mutation.type,
        target: target?.tagName || null,
        className: typeof target?.className === 'string' ? target.className.slice(0, 300) : null,
        text: (target?.innerText || target?.textContent || '').trim().slice(0, 500),
        rect: rect ? {x:rect.x,y:rect.y,width:rect.width,height:rect.height} : null,
        added: [...mutation.addedNodes].map((node) => (node.innerText || node.textContent || '').trim().slice(0, 300)),
        removed: [...mutation.removedNodes].map((node) => (node.innerText || node.textContent || '').trim().slice(0, 300)),
      });
    }
    if (window.__gpuiCaptureMutationLog.length > 2000) window.__gpuiCaptureMutationLog.splice(0, 1000);
  });
  window.__gpuiCaptureObserver.observe(document.body, {subtree:true,childList:true,characterData:true,attributes:true});
  return true;
})()`);

const before = await snapshot("01-before-focus");
const input = before.input.rect;
await dispatchMouse("mouseMoved", input.x + 30, input.y + 18);
await dispatchMouse("mousePressed", input.x + 30, input.y + 18, "left", 1);
await dispatchMouse("mouseReleased", input.x + 30, input.y + 18, "left", 0);
await new Promise((resolve) => setTimeout(resolve, 120));
await snapshot("02-focused");

await key("keyDown", "Escape", "Escape");
await key("keyUp", "Escape", "Escape");
await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("03-after-escape");

await dispatchMouse("mousePressed", input.x + 30, input.y + 18, "left", 1);
await dispatchMouse("mouseReleased", input.x + 30, input.y + 18, "left", 0);
await dispatchMouse("mousePressed", input.x + 30, input.y + 18, "left", 1);
await dispatchMouse("mouseReleased", input.x + 30, input.y + 18, "left", 0);
await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("04-second-click");

await send("Input.insertText", { text: "hello" });
await new Promise((resolve) => setTimeout(resolve, 120));
const typed = await snapshot("05-typed-active-send");
const sendButton = typed.buttons.find((button) =>
  /发送|send/i.test(button.ariaLabel || "") ||
  (button.rect.x > typed.input.rect.right - 80 && button.style.backgroundColor !== "rgba(0, 0, 0, 0)"),
);
if (!sendButton) throw new Error("Active send button not found after typing");

const sx = sendButton.rect.x + sendButton.rect.width / 2;
const sy = sendButton.rect.y + sendButton.rect.height / 2;
await dispatchMouse("mouseMoved", sx, sy);
await new Promise((resolve) => setTimeout(resolve, 120));
await snapshot("06-send-hover");
await dispatchMouse("mousePressed", sx, sy, "left", 1);
await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("07-send-pressed");
await dispatchMouse("mouseMoved", input.x - 20, input.y - 20, "left", 1);
await dispatchMouse("mouseReleased", input.x - 20, input.y - 20, "left", 0);
await new Promise((resolve) => setTimeout(resolve, 80));

await dispatchMouse("mousePressed", input.x + 50, input.y + 18, "left", 1);
await dispatchMouse("mouseReleased", input.x + 50, input.y + 18, "left", 0);
await key("keyDown", "Enter", "Enter");
await key("keyUp", "Enter", "Enter");
await new Promise((resolve) => setTimeout(resolve, 40));
await snapshot("08-immediately-after-enter");

let sawUserBubble = false;
let sawStarting = false;
let sawStreaming = false;
let completed = false;
let stableCompletedSamples = 0;
const deadline = Date.now() + 120_000;
while (Date.now() < deadline && !completed) {
  await new Promise((resolve) => setTimeout(resolve, 20));
  const state = await evaluate(snapshotExpression);
  const hasHello = state.userBubbles.some((bubble) => bubble.text.trim() === "hello");
  const hasAssistant = state.assistantMessages.some((message) => message.text.trim().length > 0);
  const isFinal = Boolean(state.finalAssistant?.text?.trim());
  if (hasHello && !sawUserBubble) {
    sawUserBubble = true;
    await snapshot("09-user-bubble");
  }
  if (!state.input && !hasAssistant && !sawStarting) {
    sawStarting = true;
    await snapshot("10-task-starting");
  }
  if (hasAssistant && !sawStreaming) {
    sawStreaming = true;
    await snapshot("11-first-stream-token");
  }
  if (sawStreaming && !isFinal) {
    if (!fs.existsSync(path.join(ARTIFACT_DIR, "12-streaming.png"))) {
      await new Promise((resolve) => setTimeout(resolve, 250));
      await snapshot("12-streaming");
    }
    stableCompletedSamples = 0;
  } else if (sawStreaming && isFinal && state.input) {
    stableCompletedSamples++;
    if (stableCompletedSamples >= 8) completed = true;
  }
}
if (!sawUserBubble) throw new Error("User hello bubble did not appear");
if (!sawStarting) throw new Error("Task starting state did not appear");
if (!sawStreaming) throw new Error("Model response did not stream before timeout");
if (!completed) throw new Error("Model response did not reach a completed state before timeout");
await snapshot("13-completed");

const finishedInput = await inputRect();
await dispatchMouse("mousePressed", finishedInput.x, finishedInput.y - 80, "left", 1);
await dispatchMouse("mouseReleased", finishedInput.x, finishedInput.y - 80, "left", 0);
await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("14-outside-click");

await dispatchMouse("mousePressed", finishedInput.x + 30, finishedInput.y + 18, "left", 1);
await dispatchMouse("mouseReleased", finishedInput.x + 30, finishedInput.y + 18, "left", 0);
await key("keyDown", "Tab", "Tab");
await key("keyUp", "Tab", "Tab");
await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("15-keyboard-tab");

fs.writeFileSync(
  path.join(ARTIFACT_DIR, "flow-summary.json"),
  JSON.stringify({ target, samples: samples.map(({ label, capturedAt }) => ({ label, capturedAt })) }, null, 2),
);
socket.close();
console.log(JSON.stringify({ artifactDir: ARTIFACT_DIR, target, labels: samples.map((sample) => sample.label) }, null, 2));
