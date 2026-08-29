import fs from "node:fs";
import path from "node:path";

const CDP_HTTP = "http://127.0.0.1:9222";
const outputDir = path.resolve(
  process.argv.find((argument) => argument.startsWith("--artifact-dir="))
    ?.slice("--artifact-dir=".length) ||
    "artifacts/chatgpt-command-tool-ui-2026-08-30",
);
const sentinel =
  process.argv.find((argument) => argument.startsWith("--sentinel="))
    ?.slice("--sentinel=".length) || "SHELLPIXEL20260830";
const prompt = `请使用终端执行 printf '${sentinel}\\n'，等待命令执行完成后告诉我输出。`;
fs.mkdirSync(outputDir, { recursive: true });

const targets = await (await fetch(`${CDP_HTTP}/json/list`)).json();
const target = targets.find(
  (candidate) =>
    candidate.type === "page" &&
    candidate.title === "ChatGPT" &&
    candidate.url === "app://-/index.html",
);
if (!target) throw new Error("ChatGPT app target not found on CDP port 9222");

const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 0;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const callback = pending.get(message.id);
  if (!callback) return;
  pending.delete(message.id);
  if (message.error) callback.reject(new Error(JSON.stringify(message.error)));
  else callback.resolve(message.result);
};

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
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

async function clickPoint(x, y) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await send("Input.dispatchMouseEvent", {
    type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1,
  });
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1,
  });
}

async function screenshot(name) {
  const image = await send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(path.join(outputDir, `${name}.png`), Buffer.from(image.data, "base64"));
}

const snapshotExpression = `(() => {
  const sentinel = ${JSON.stringify(sentinel)};
  const rect = (element) => {
    const r = element.getBoundingClientRect();
    return {x:r.x,y:r.y,width:r.width,height:r.height,top:r.top,right:r.right,bottom:r.bottom,left:r.left};
  };
  const style = (element) => {
    const value = getComputedStyle(element);
    const keys = [
      'display','position','boxSizing','flexDirection','alignItems','justifyContent','gap',
      'width','height','minWidth','minHeight','maxWidth','maxHeight','paddingTop','paddingRight',
      'paddingBottom','paddingLeft','marginTop','marginRight','marginBottom','marginLeft',
      'backgroundColor','color','borderTopWidth','borderRightWidth','borderBottomWidth','borderLeftWidth',
      'borderTopColor','borderRightColor','borderBottomColor','borderLeftColor','borderRadius','boxShadow',
      'fontFamily','fontSize','fontWeight','lineHeight','letterSpacing','opacity','cursor',
      'overflowX','overflowY','whiteSpace','textOverflow','transition','transform'
    ];
    return Object.fromEntries(keys.map((key) => [key, value[key]]));
  };
  const node = (element, html = false) => {
    if (!element) return null;
    const data = Object.fromEntries([...element.attributes]
      .filter((attribute) => attribute.name.startsWith('data-') || attribute.name.startsWith('aria-') || attribute.name === 'role')
      .map((attribute) => [attribute.name, attribute.value]));
    const result = {
      tag: element.tagName,
      id: element.id || null,
      className: typeof element.className === 'string' ? element.className : null,
      text: (element.innerText || element.textContent || '').trim().slice(0, 4000),
      data,
      rect: rect(element),
      style: style(element),
      childCount: element.children.length,
    };
    if (html) result.outerHTML = element.outerHTML.slice(0, 30000);
    return result;
  };
  const input = document.querySelector('[role="textbox"][contenteditable="true"]');
  const inputTop = input?.getBoundingClientRect().top || innerHeight;
  const all = [...document.querySelectorAll('*')];
  const relevant = all.filter((element) => {
    const r = element.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0 || r.left < 260 || r.top < 40 || r.top >= inputTop) return false;
    const text = (element.innerText || element.textContent || '').trim();
    const aria = [element.getAttribute('aria-label'), element.getAttribute('title'), element.getAttribute('data-testid')]
      .filter(Boolean).join(' ');
    return text.includes(sentinel) || text.includes('printf') ||
      /终端|命令|command|terminal|tool|运行|读取|执行/.test(aria);
  }).map((element) => node(element, true))
    .sort((a,b) => (a.rect.width*a.rect.height) - (b.rect.width*b.rect.height))
    .slice(0, 80);
  const centralButtons = [...document.querySelectorAll('button')].filter((element) => {
    const r = element.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && r.left > 260 && r.top > 40 && r.top < inputTop;
  }).map((element) => node(element, true)).slice(-100);
  const assistantMessages = [...document.querySelectorAll('[data-markdown-text-style="assistant-message"]')]
    .map((element) => node(element, true));
  const userMessages = [...document.querySelectorAll('[data-user-message-bubble="true"]')]
    .map((element) => node(element, true));
  return {
    capturedAt: new Date().toISOString(),
    performanceNow: performance.now(),
    viewport: {innerWidth,innerHeight,devicePixelRatio,scrollX,scrollY},
    bodyTextTail: document.body.innerText.slice(-12000),
    input: input ? node(input, true) : null,
    relevant,
    centralButtons,
    assistantMessages,
    userMessages,
    mutationLog: (window.__commandToolCaptureMutations || []).slice(-500),
  };
})()`;

async function snapshot(name, withScreenshot = true) {
  const value = await evaluate(snapshotExpression);
  value.name = name;
  fs.writeFileSync(path.join(outputDir, `${name}.json`), JSON.stringify(value, null, 2));
  if (withScreenshot) await screenshot(name);
  return value;
}

await send("Page.enable");
await send("Runtime.enable");

const newChatRect = await evaluate(`(() => {
  const button = [...document.querySelectorAll('button')].find((candidate) =>
    (candidate.innerText || '').trim() === '新对话' && candidate.getBoundingClientRect().top < 180
  );
  if (!button) return null;
  const r = button.getBoundingClientRect();
  return {x:r.x,y:r.y,width:r.width,height:r.height};
})()`);
if (!newChatRect) throw new Error("New chat button not found");
await clickPoint(newChatRect.x + newChatRect.width / 2, newChatRect.y + newChatRect.height / 2);

const deadlineForInput = Date.now() + 15_000;
while (Date.now() < deadlineForInput) {
  if (await evaluate(`Boolean(document.querySelector('[role="textbox"][contenteditable="true"]'))`)) break;
  await new Promise((resolve) => setTimeout(resolve, 100));
}

await evaluate(`(() => {
  window.__commandToolCaptureMutations = [];
  window.__commandToolCaptureObserver?.disconnect();
  window.__commandToolCaptureObserver = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      const target = mutation.target.nodeType === Node.ELEMENT_NODE ? mutation.target : mutation.target.parentElement;
      const r = target?.getBoundingClientRect?.();
      window.__commandToolCaptureMutations.push({
        time: performance.now(), type: mutation.type, tag: target?.tagName || null,
        className: typeof target?.className === 'string' ? target.className.slice(0,500) : null,
        text: (target?.innerText || target?.textContent || '').trim().slice(0,2000),
        rect: r ? {x:r.x,y:r.y,width:r.width,height:r.height} : null,
        attributes: mutation.attributeName || null,
        added: [...mutation.addedNodes].map((node) => (node.innerText || node.textContent || '').trim().slice(0,1000)),
        removed: [...mutation.removedNodes].map((node) => (node.innerText || node.textContent || '').trim().slice(0,1000)),
      });
    }
    if (window.__commandToolCaptureMutations.length > 5000) window.__commandToolCaptureMutations.splice(0,2500);
  });
  window.__commandToolCaptureObserver.observe(document.body, {subtree:true,childList:true,characterData:true,attributes:true});
  return true;
})()`);

await snapshot("01-empty");
const inputRect = await evaluate(`(() => {
  const r = document.querySelector('[role="textbox"][contenteditable="true"]').getBoundingClientRect();
  return {x:r.x,y:r.y,width:r.width,height:r.height};
})()`);
await clickPoint(inputRect.x + 40, inputRect.y + 20);
await send("Input.insertText", { text: prompt });
await snapshot("02-prompt-typed");
await send("Input.dispatchKeyEvent", {
  type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 36,
});
await send("Input.dispatchKeyEvent", {
  type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 36,
});

await new Promise((resolve) => setTimeout(resolve, 80));
await snapshot("03-submitted");

let lastFingerprint = "";
let changeIndex = 0;
let finalSnapshot = null;
const completionDeadline = Date.now() + 180_000;
while (Date.now() < completionDeadline) {
  await new Promise((resolve) => setTimeout(resolve, 150));
  const current = await evaluate(snapshotExpression);
  const fingerprint = JSON.stringify(current.relevant.map((entry) => [entry.tag,entry.text,entry.data,entry.rect]));
  if (fingerprint !== lastFingerprint && current.relevant.length > 0) {
    lastFingerprint = fingerprint;
    changeIndex += 1;
    const name = `04-tool-change-${String(changeIndex).padStart(2, "0")}`;
    current.name = name;
    fs.writeFileSync(path.join(outputDir, `${name}.json`), JSON.stringify(current, null, 2));
    if (changeIndex <= 12) await screenshot(name);
  }
  const hasFinalAssistant = current.assistantMessages.some((entry) =>
    entry.text.replace(/\\/g, '').includes(sentinel)
  );
  const stopButtonVisible = [...current.centralButtons].some((entry) => /停止|stop/i.test(entry.data?.['aria-label'] || ''));
  if (hasFinalAssistant && !stopButtonVisible) {
    finalSnapshot = current;
    break;
  }
}
if (!finalSnapshot) throw new Error("Timed out waiting for completed command-tool response");
await snapshot("05-completed-collapsed");

const summaryToggle = await evaluate(`(() => {
  const button = [...document.querySelectorAll('button')].find((element) => {
    const r = element.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && r.left > 260 && /用时|已处理/.test((element.innerText || '').trim());
  });
  if (!button) return null;
  const r = button.getBoundingClientRect();
  return {x:r.x+r.width/2,y:r.y+r.height/2,text:(button.innerText||'').trim()};
})()`);
if (summaryToggle) {
  await clickPoint(summaryToggle.x, summaryToggle.y);
  await new Promise((resolve) => setTimeout(resolve, 300));
  await snapshot("06-summary-expanded");
}

const expandable = await evaluate(`(() => {
  const body = document.querySelector('[data-testid="exec-shell-body"]');
  const element = body?.previousElementSibling?.querySelector('button');
  if (!element) return null;
  const r = element.getBoundingClientRect();
  return {x:r.x+r.width/2,y:r.y+r.height/2,text:(element.innerText||'').trim(),html:element.outerHTML.slice(0,10000)};
})()`);
if (expandable) {
  await clickPoint(expandable.x, expandable.y);
  await new Promise((resolve) => setTimeout(resolve, 300));
  await snapshot("07-tool-expanded");
}

fs.writeFileSync(
  path.join(outputDir, "flow-summary.json"),
  JSON.stringify({
    target: {id:target.id,title:target.title,url:target.url}, prompt, sentinel,
    toolStateChanges: changeIndex, summaryToggle, expandable,
    files: fs.readdirSync(outputDir).sort(),
  }, null, 2),
);
socket.close();
console.log(JSON.stringify({outputDir,prompt,toolStateChanges:changeIndex,expanded:Boolean(expandable)}));
