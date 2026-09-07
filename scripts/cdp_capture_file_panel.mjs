// Read the real file panel (including its editor/tree shadow roots) in a dedicated
// ChatGPT debug instance. Does not type, save, change themes, or send prompts.
import fs from 'node:fs';
const endpoint = process.env.CHATGPT_CDP_HTTP || 'http://127.0.0.1:9222';
const output = process.argv[2] || 'artifacts/file-panel';
fs.mkdirSync(output, { recursive: true });
const targets = await (await fetch(`${endpoint}/json/list`)).json();
const target = targets.find(t => t.url === 'app://-/index.html');
if (!target) throw Error('ChatGPT page not found');
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
let id = 0;
const pending = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data), entry = pending.get(message.id);
  if (!entry) return;
  pending.delete(message.id); clearTimeout(entry.timer);
  message.error ? entry.reject(message.error) : entry.resolve(message.result);
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const requestId = ++id;
  const timer = setTimeout(() => { pending.delete(requestId); reject(Error(`${method} timed out`)); }, 30000);
  pending.set(requestId, { resolve, reject, timer });
  socket.send(JSON.stringify({ id: requestId, method, params }));
});
try {
  await send('Page.bringToFront');
  const result = await send('Runtime.evaluate', {
    returnByValue: true, expression: `(() => {
      const tree = document.querySelector('file-tree-container');
      const editor = document.querySelector('diffs-container');
      if (!tree && !editor) throw Error('Open a file in the dedicated debug instance first');
      const props = ['fontFamily','fontSize','fontWeight','lineHeight','color','backgroundColor','borderColor','borderRadius','padding','gap','width','height'];
      const roots = [document, tree?.shadowRoot, editor?.shadowRoot].filter(Boolean);
      const selectors = ['[data-app-shell-tab-controller=right]', '[aria-label="切换文件树"]', '#workspace-directory-tree-search', 'file-tree-container', 'diffs-container', '[contenteditable][data-content]', '[data-line]', '[data-gutter]', '[data-item-path]', '[aria-label="撤销"]', '[aria-label="重做"]'];
      const nodes = roots.flatMap(root => [...root.querySelectorAll(selectors.join(','))]);
      return { viewport:[innerWidth,innerHeight], dpr:devicePixelRatio, theme:document.documentElement.className,
        nodes:nodes.filter(e=>e.getBoundingClientRect().width>0).slice(0,180).map(e=>({
          tag:e.tagName, label:e.getAttribute('aria-label'), path:e.getAttribute('data-item-path'),
          editable:e.getAttribute('contenteditable'), role:e.getAttribute('role'), rect:e.getBoundingClientRect().toJSON(),
          style:Object.fromEntries(props.map(p=>[p,getComputedStyle(e)[p]]))
        })) };
    })()`
  });
  if (result.exceptionDetails) throw Error(JSON.stringify(result.exceptionDetails));
  const data = result.result.value, theme = data.theme.includes('dark') ? 'dark' : 'light';
  fs.writeFileSync(`${output}/cdp-file-panel-${theme}.json`, JSON.stringify(data, null, 2));
  const shot = await send('Page.captureScreenshot', { format:'png', captureBeyondViewport:false });
  fs.writeFileSync(`${output}/cdp-file-panel-${theme}.png`, Buffer.from(shot.data,'base64'));
  console.log(JSON.stringify({output,theme,viewport:data.viewport,nodes:data.nodes.length}));
} finally { socket.close(); for (const entry of pending.values()) clearTimeout(entry.timer); }
