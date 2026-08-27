#!/usr/bin/env python3
"""Capture the real ChatGPT project-creation dialog through its CDP target."""

from __future__ import annotations

import base64
import json
import pathlib
import sys
import time
import urllib.request

from cdp_chatgpt import Cdp


def main() -> None:
    output = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "artifacts/reference-project-create-dark.png")
    remote = "--remote" in sys.argv[2:]
    with urllib.request.urlopen("http://127.0.0.1:9222/json/list", timeout=5) as response:
        targets = json.load(response)
    target = next(
        item
        for item in targets
        if item.get("type") == "page" and item.get("url") == "app://-/index.html"
    )
    cdp = Cdp(target["webSocketDebuggerUrl"])
    cdp.socket.settimeout(90)
    try:
        cdp.call("Runtime.enable")
        cdp.call("Page.enable")
        cdp.call(
            "Emulation.setDeviceMetricsOverride",
            {"width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": False},
        )
        state = cdp.call(
            "Runtime.evaluate",
            {
                "expression": """(() => {
                  const trigger = document.querySelector('button[aria-label="添加新项目"]');
                  const rect = trigger?.getBoundingClientRect();
                  return trigger && rect ? {
                    open: trigger.dataset.state === 'open',
                    x: rect.x + rect.width / 2,
                    y: rect.y + rect.height / 2
                  } : null;
                })()""",
                "returnByValue": True,
            },
        )["result"]["value"]
        if state is None:
            raise RuntimeError("The ChatGPT project-create trigger was not found")
        if not state["open"]:
            cdp.call(
                "Input.dispatchMouseEvent",
                {"type": "mouseMoved", "x": state["x"], "y": state["y"]},
            )
            time.sleep(0.08)
            for event_type in ("mousePressed", "mouseReleased"):
                cdp.call(
                    "Input.dispatchMouseEvent",
                    {
                        "type": event_type,
                        "x": state["x"],
                        "y": state["y"],
                        "button": "left",
                        "clickCount": 1,
                    },
                )
        time.sleep(0.25)
        visible = cdp.call(
            "Runtime.evaluate",
            {
                "expression": "[...document.querySelectorAll('[role=dialog]')].some(element => element.getBoundingClientRect().width > 0)",
                "returnByValue": True,
            },
        )["result"]["value"]
        if not visible:
            raise RuntimeError("The ChatGPT project-create dialog did not open")
        if remote:
            cdp.call(
                "Runtime.evaluate",
                {
                    "expression": "[...document.querySelectorAll('[role=radio]')][1]?.click()"
                },
            )
            time.sleep(0.08)
            cdp.call(
                "Runtime.evaluate",
                {
                    "expression": "[...document.querySelectorAll('button')].find(button => button.type === 'submit' && button.innerText.trim() === '下一步')?.click()"
                },
            )
            time.sleep(0.2)
            remote_visible = cdp.call(
                "Runtime.evaluate",
                {
                    "expression": "[...document.querySelectorAll('[role=dialog]')].some(element => element.innerText.includes('新建远程项目'))",
                    "returnByValue": True,
                },
            )["result"]["value"]
            if not remote_visible:
                raise RuntimeError("The ChatGPT remote-project dialog did not open")
        cdp.call(
            "Runtime.evaluate",
            {
                "expression": "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))",
                "awaitPromise": True,
            },
        )
        image = cdp.call(
            "Page.captureScreenshot",
            {
                "format": "png",
                "fromSurface": True,
                "captureBeyondViewport": False,
                "optimizeForSpeed": True,
                "clip": {"x": 0, "y": 0, "width": 1440, "height": 900, "scale": 1},
            },
        )["data"]
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(base64.b64decode(image))
        print(output)
    finally:
        try:
            cdp.call("Emulation.clearDeviceMetricsOverride")
        finally:
            cdp.close()


if __name__ == "__main__":
    main()
