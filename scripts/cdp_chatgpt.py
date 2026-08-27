#!/usr/bin/env python3
"""Small CDP probe for the locally running ChatGPT desktop app."""

from __future__ import annotations

import argparse
import base64
import json
import pathlib
import time
import urllib.request

import websocket


class Cdp:
    def __init__(self, url: str) -> None:
        self.socket = websocket.create_connection(url, timeout=15, suppress_origin=True)
        self.sequence = 0

    def call(self, method: str, params: dict | None = None) -> dict:
        self.sequence += 1
        request_id = self.sequence
        self.socket.send(json.dumps({"id": request_id, "method": method, "params": params or {}}))
        while True:
            message = json.loads(self.socket.recv())
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(message["error"])
                return message.get("result", {})

    def close(self) -> None:
        self.socket.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--endpoint", default="http://127.0.0.1:9222")
    parser.add_argument("--expression")
    parser.add_argument("--click", nargs=2, type=float, metavar=("X", "Y"))
    parser.add_argument("--click-selector")
    parser.add_argument("--press", nargs=2, type=float, metavar=("X", "Y"))
    parser.add_argument("--release", nargs=2, type=float, metavar=("X", "Y"))
    parser.add_argument("--move", nargs=2, type=float, metavar=("X", "Y"))
    parser.add_argument("--scroll", type=float)
    parser.add_argument("--key")
    parser.add_argument("--drag", nargs=4, type=float, metavar=("X1", "Y1", "X2", "Y2"))
    parser.add_argument("--clip", nargs=4, type=float, metavar=("X", "Y", "WIDTH", "HEIGHT"))
    parser.add_argument("--screenshot", type=pathlib.Path)
    args = parser.parse_args()

    with urllib.request.urlopen(f"{args.endpoint}/json/list", timeout=5) as response:
        targets = json.load(response)
    target = next(
        item
        for item in targets
        if item.get("type") == "page" and item.get("url") == "app://-/index.html"
    )
    cdp = Cdp(target["webSocketDebuggerUrl"])
    try:
        cdp.call("Runtime.enable")
        cdp.call("Page.enable")
        if args.click_selector:
            selector = json.dumps(args.click_selector, ensure_ascii=False)
            rect = None
            evaluation = None
            for _ in range(20):
                evaluation = cdp.call(
                    "Runtime.evaluate",
                    {
                        "expression": (
                            f"(() => {{ const element = document.querySelector({selector}); "
                            "if (!element) return null; const rect = element.getBoundingClientRect(); "
                            "return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }; })()"
                        ),
                        "returnByValue": True,
                    },
                )
                rect = evaluation.get("result", {}).get("value")
                if rect is not None:
                    break
                time.sleep(0.05)
            if rect is None:
                raise RuntimeError(
                    f"selector did not match: {args.click_selector}; evaluation={evaluation}"
                )
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseMoved", **rect})
            cdp.call("Input.dispatchMouseEvent", {"type": "mousePressed", **rect, "button": "left", "clickCount": 1})
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseReleased", **rect, "button": "left", "clickCount": 1})
        if args.move:
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": args.move[0], "y": args.move[1]})
        if args.click:
            x, y = args.click
            cdp.call("Input.dispatchMouseEvent", {"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1})
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1})
        if args.press:
            cdp.call("Input.dispatchMouseEvent", {"type": "mousePressed", "x": args.press[0], "y": args.press[1], "button": "left", "clickCount": 1})
        if args.release:
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": args.release[0], "y": args.release[1], "button": "left", "clickCount": 1})
        if args.scroll:
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseWheel", "x": 200, "y": 400, "deltaX": 0, "deltaY": args.scroll})
        if args.key:
            key_map = {
                "Escape": ("Escape", "Escape", 27),
                "ArrowDown": ("ArrowDown", "ArrowDown", 40),
                "ArrowUp": ("ArrowUp", "ArrowUp", 38),
                "Home": ("Home", "Home", 36),
                "End": ("End", "End", 35),
                "Enter": ("Enter", "Enter", 13),
                "Space": (" ", "Space", 32),
            }
            key, code, virtual_key = key_map.get(args.key, (args.key, args.key, 0))
            params = {
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": virtual_key,
                "nativeVirtualKeyCode": virtual_key,
            }
            cdp.call("Input.dispatchKeyEvent", {"type": "keyDown", **params})
            cdp.call("Input.dispatchKeyEvent", {"type": "keyUp", **params})
        if args.drag:
            x1, y1, x2, y2 = args.drag
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x1, "y": y1})
            cdp.call("Input.dispatchMouseEvent", {"type": "mousePressed", "x": x1, "y": y1, "button": "left", "buttons": 1, "clickCount": 1})
            for step in range(1, 9):
                progress = step / 8
                cdp.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x1 + (x2 - x1) * progress, "y": y1 + (y2 - y1) * progress, "button": "left", "buttons": 1})
            cdp.call("Input.dispatchMouseEvent", {"type": "mouseReleased", "x": x2, "y": y2, "button": "left", "buttons": 0, "clickCount": 1})
        time.sleep(0.12)
        output: dict = {}
        if args.expression:
            output["value"] = cdp.call(
                "Runtime.evaluate",
                {"expression": args.expression, "returnByValue": True, "awaitPromise": True},
            ).get("result", {}).get("value")
        if args.screenshot:
            args.screenshot.parent.mkdir(parents=True, exist_ok=True)
            screenshot_options = {"format": "png", "fromSurface": True}
            if args.clip:
                screenshot_options["clip"] = {
                    "x": args.clip[0], "y": args.clip[1], "width": args.clip[2],
                    "height": args.clip[3], "scale": 1,
                }
            image = cdp.call("Page.captureScreenshot", screenshot_options)["data"]
            args.screenshot.write_bytes(base64.b64decode(image))
            output["screenshot"] = str(args.screenshot)
        print(json.dumps(output, ensure_ascii=False, indent=2))
    finally:
        cdp.close()


if __name__ == "__main__":
    main()
