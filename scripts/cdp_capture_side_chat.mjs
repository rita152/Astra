// Read-only styles and screenshots from a dedicated ChatGPT debug instance.
// The endpoint must be explicit: an existing debug port may belong to another task.
import fs from 'node:fs';

const endpoint = process.env.CHATGPT_CDP_HTTP;
if (!endpoint) throw Error('Set CHATGPT_CDP_HTTP to your dedicated debug instance');
const output = process.argv[2] || 'artifacts/side-chat';
const name = process.argv[3] || 'reference';
fs.mkdirSync(output, { recursive: true });
const targets = await (await fetch(`${endpoint}/json/list`)).json();
const page = targets.find(t => t.url === 'app://-/index.html');
if (!page) throw Error('ChatGPT main window not found');
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let sequence = 0;
const pending = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data), request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id); clearTimeout(request.timer);
  message.error ? request.reject(Error(message.error.message)) : request.resolve(message.result);
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = ++sequence;
  const timer = setTimeout(() => { pending.delete(id); reject(Error(`${method} timed out`)); }, 10000);
  pending.set(id, { resolve, reject, timer });
  socket.send(JSON.stringify({ id, method, params }));
});
try {
  const result = await send('Runtime.evaluate', { returnByValue: true, expression: `(() => {
    const panel = document.querySelector('[data-app-shell-tab-panel-controller="right"]');
    const left = panel?.getBoundingClientRect().left ?? innerWidth / 2;
    const roots = [document];
    const walk = root => { for (const e of root.querySelectorAll('*')) if (e.shadowRoot) { roots.push(e.shadowRoot); walk(e.shadowRoot); } };
    walk(document);
    const properties = ['fontFamily','fontSize','fontWeight','lineHeight','color','backgroundColor','border','borderRadius','boxShadow','padding','margin','gap','overflow','width','height'];
    const visible = e => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.bottom > 0 && r.y < innerHeight && (r.x >= left || e.closest('[role=dialog],[role=menu]')); };
    const nodes = roots.flatMap(root => [...root.querySelectorAll('button,input,textarea,div,p,span,svg,[role=menuitem],[role=checkbox]')]).filter(visible);
    return { viewport:[innerWidth,innerHeight], dpr:devicePixelRatio, theme:document.documentElement.className, panel:panel?.getBoundingClientRect().toJSON(),
      nodes:nodes.map(e => ({ tag:e.tagName, role:e.getAttribute('role'), label:e.getAttribute('aria-label'),
        text:e.children.length ? (e.matches('button,[role=menuitem]') ? e.textContent.slice(0,250) : null) : e.textContent.slice(0,400),
        class:e.getAttribute('class'), placeholder:e.getAttribute('placeholder'), disabled:e.getAttribute('aria-disabled') || e.disabled,
        rect:e.getBoundingClientRect().toJSON(), style:Object.fromEntries(properties.map(p => [p,getComputedStyle(e)[p]])) })) };
  })()` });
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  fs.writeFileSync(`${output}/${name}.json`, JSON.stringify(result.result.value, null, 2));
  const shot = await send('Page.captureScreenshot', { format:'png', captureBeyondViewport:false });
  fs.writeFileSync(`${output}/${name}.png`, Buffer.from(shot.data, 'base64'));
  console.log(JSON.stringify({ name, panel:result.result.value.panel, nodes:result.result.value.nodes.length }));
} finally { for (const request of pending.values()) clearTimeout(request.timer); socket.close(); }
