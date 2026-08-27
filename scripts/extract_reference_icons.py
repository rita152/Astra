#!/usr/bin/env python3
"""Extract the original vector icons embedded in the Electron DOM snapshot."""

from pathlib import Path

from lxml import etree, html


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "chat-reference" / "chat.html"
OUTPUT = ROOT / "assets" / "icons"

TEXT_ICONS = {
    "new-chat": "新对话",
    "pull-request": "拉取请求",
    "sites": "站点",
    "scheduled": "已安排",
    "plugins": "插件",
    "folder": "oh-my-pi",
    "suggestion": "Prove plugin upgrades never mutate an active run",
    "credits": "获得 250 额度",
    "permission": "完全访问",
    "home-mark": "你想让我们在 coda 中构建什么？",
    "utility-folder": ("coda", 2),
    "local": "本地",
    "branch": "main",
}

ARIA_ICONS = {
    "add": "添加文件等内容",
    "dictation": "听写",
    "voice": "开始新的语音聊天",
    "search": "搜索",
    "activity": "查看活动，需要关注",
    "quick-chat": "快速聊天",
    "sidebar-toggle": ("隐藏边栏", 0),
    "back": ("返回", 0),
    "forward": ("前进", 0),
    "bottom-panel": ("切换底部面板显示", 0),
    "right-sidebar": ("显示/隐藏侧边栏", 0),
    "help": "打开帮助菜单",
}


def closest_svg(node):
    current = node
    for _ in range(10):
        svgs = current.xpath('.//*[local-name()="svg"]')
        if svgs:
            return svgs[0]
        current = current.getparent()
        if current is None:
            break
    raise RuntimeError(f"no SVG near {node.text_content().strip()!r}")


def serialize(svg) -> bytes:
    svg.attrib.pop("class", None)
    if "viewbox" in svg.attrib:
        svg.attrib["viewBox"] = svg.attrib.pop("viewbox")
    svg.attrib["xmlns"] = "http://www.w3.org/2000/svg"
    return etree.tostring(svg, encoding="utf-8", xml_declaration=False)


def main() -> None:
    document = html.fromstring(SOURCE.read_text())
    OUTPUT.mkdir(parents=True, exist_ok=True)

    for name, target in TEXT_ICONS.items():
        label, occurrence = target if isinstance(target, tuple) else (target, 0)
        nodes = document.xpath(f'//*[normalize-space(text())={label!r}]')
        if not nodes:
            text_nodes = document.xpath(
                f'//text()[contains(normalize-space(.), {label.split()[0]!r})]'
            )
            nodes = [text_nodes[0].getparent()] if text_nodes else []
        if not nodes:
            raise RuntimeError(f"missing text target: {label}")
        (OUTPUT / f"{name}.svg").write_bytes(
            serialize(closest_svg(nodes[occurrence]))
        )

    for name, target in ARIA_ICONS.items():
        label, occurrence = target if isinstance(target, tuple) else (target, 0)
        nodes = document.xpath(f'//*[@aria-label={label!r}]')
        if not nodes:
            raise RuntimeError(f"missing aria target: {label}")
        (OUTPUT / f"{name}.svg").write_bytes(
            serialize(closest_svg(nodes[occurrence]))
        )


if __name__ == "__main__":
    main()
