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

await send("Runtime.enable");
const bubbleRectResult = await send("Runtime.evaluate", {
  expression: `(() => {
    const rect = document.querySelector('[data-user-message-bubble="true"]')?.getBoundingClientRect();
    return rect ? {x:rect.x,y:rect.y,width:rect.width,height:rect.height} : null;
  })()`,
  returnByValue: true,
});
const hoveredBubbleRect = bubbleRectResult.result.value;
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 500, y: 500 });
await new Promise((resolve) => setTimeout(resolve, 150));
const beforeHoverResult = await send("Runtime.evaluate", {
  expression: `(() => {
    const bubble = document.querySelector('[data-user-message-bubble="true"]');
    const container = bubble?.parentElement;
    const copy = container?.querySelector('button[aria-label="复制消息"]');
    const time = [...(container?.querySelectorAll('span') || [])]
      .find((element) => /^\\d{2}:\\d{2}$/.test((element.textContent || '').trim()));
    const inspect = (element) => element ? {
      visibility:getComputedStyle(element).visibility,
      opacity:getComputedStyle(element).opacity,
      display:getComputedStyle(element).display,
    } : null;
    return {copy:inspect(copy), time:inspect(time)};
  })()`,
  returnByValue: true,
});
if (hoveredBubbleRect) {
  await send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: hoveredBubbleRect.x + hoveredBubbleRect.width / 2,
    y: hoveredBubbleRect.y + hoveredBubbleRect.height / 2,
  });
  await new Promise((resolve) => setTimeout(resolve, 150));
}

const result = await send("Runtime.evaluate", {
  expression: `(() => {
    const composer = document.querySelector('[role="textbox"][contenteditable="true"]');
    const composerTop = composer?.getBoundingClientRect().top ?? innerHeight;
    const actions = [...document.querySelectorAll('button')]
      .filter((button) => {
        const rect = button.getBoundingClientRect();
        return button.querySelector('svg') && rect.width > 0 && rect.height > 0 &&
          rect.left > 250 && rect.top < composerTop;
      })
      .map((button) => {
        const rect = button.getBoundingClientRect();
        const style = getComputedStyle(button);
        const svg = button.querySelector('svg');
        const svgRect = svg?.getBoundingClientRect();
        const svgStyle = svg ? getComputedStyle(svg) : null;
        return {
          ariaLabel: button.getAttribute('aria-label'),
          title: button.getAttribute('title'),
          text: (button.innerText || '').trim(),
          rect: {x:rect.x,y:rect.y,width:rect.width,height:rect.height},
          color: style.color,
          backgroundColor: style.backgroundColor,
          borderRadius: style.borderRadius,
          svgRect: svgRect ? {
            x:svgRect.x,y:svgRect.y,width:svgRect.width,height:svgRect.height,
          } : null,
          svgStyle: svgStyle ? {
            width:svgStyle.width,height:svgStyle.height,
            display:svgStyle.display,transform:svgStyle.transform,
          } : null,
          svg: svg?.outerHTML,
        };
      });
    const bubble = document.querySelector('[data-user-message-bubble="true"]');
    const bubbleRect = bubble?.getBoundingClientRect();
    const bubbleNodes = bubble ? [bubble, ...bubble.querySelectorAll('*')]
      .filter((element) => (element.textContent || '').trim() === 'hello')
      .map((element) => {
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return {
          tag: element.tagName,
          className: element.className,
          rect: {x:rect.x,y:rect.y,width:rect.width,height:rect.height},
          padding: [style.paddingTop, style.paddingRight, style.paddingBottom, style.paddingLeft],
          borderRadius: style.borderRadius,
          cornerShape: style.cornerShape,
          clipPath: style.clipPath,
          mask: style.mask,
          maskImage: style.maskImage,
          webkitMask: style.webkitMask,
          webkitMaskImage: style.webkitMaskImage,
          backgroundColor: style.backgroundColor,
          fontFamily: style.fontFamily,
          fontSize: style.fontSize,
          fontWeight: style.fontWeight,
          lineHeight: style.lineHeight,
        };
      }) : [];
    const bubbleFooterNodes = bubbleRect ? [...document.querySelectorAll('*')]
      .filter((element) => {
        const rect = element.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0 &&
          rect.top >= bubbleRect.bottom && rect.top < bubbleRect.bottom + 40 &&
          rect.left >= bubbleRect.left - 100 && rect.right <= bubbleRect.right + 4 &&
          (element.children.length === 0 || element.tagName === 'BUTTON');
      })
      .map((element) => {
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return {
          tag: element.tagName,
          text: (element.innerText || '').trim(),
          ariaLabel: element.getAttribute('aria-label'),
          className: element.className,
          rect: {x:rect.x,y:rect.y,width:rect.width,height:rect.height},
          color: style.color,
          opacity: style.opacity,
          backgroundColor: style.backgroundColor,
          borderRadius: style.borderRadius,
          fontSize: style.fontSize,
          lineHeight: style.lineHeight,
          transition: style.transition,
          transitionProperty: style.transitionProperty,
          transitionDuration: style.transitionDuration,
          transitionTimingFunction: style.transitionTimingFunction,
          transitionDelay: style.transitionDelay,
          svg: element.querySelector('svg')?.outerHTML || null,
        };
      }) : [];
    return {actions, bubbleNodes, bubbleFooterNodes};
  })()`,
  returnByValue: true,
});

const value = result.result.value;
value.beforeHover = beforeHoverResult.result.value;
console.log(JSON.stringify(value, null, 2));
socket.close();
