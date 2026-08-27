use super::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec};

macro_rules! key {
    ($title:literal, $subtitle:literal, $shortcut:literal) => {
        RowSpec::new($title, $subtitle, ControlSpec::Shortcut($shortcut))
    };
}

const PETS: &[SectionSpec] = &[
    SectionSpec::new(
        "选择宠物",
        "宠物会管理对话串，并突出显示需要关注的事项",
        &[
            RowSpec::new("创建", "收起宠物", ControlSpec::Button("创建")),
            RowSpec::new(
                "Codex",
                "The original Codex companion.",
                ControlSpec::Value("已选"),
            ),
            RowSpec::new(
                "Dewey",
                "A calm companion for focused workspace days",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Fireball",
                "Hot path energy for fast iteration.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Hoots",
                "A sharp-eyed owl for polished work in a blink.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Rocky",
                "A steady rock when the diff gets large.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Seedy",
                "Small green shoots for new ideas.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Stacky",
                "A balanced stack for deep work.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "BSOD",
                "A tiny blue-screen gremlin.",
                ControlSpec::Button("选择"),
            ),
            RowSpec::new(
                "Null Signal",
                "Quiet signal from the void.",
                ControlSpec::Button("选择"),
            ),
        ],
    ),
    SectionSpec::new(
        "自定义宠物",
        "",
        &[RowSpec::new(
            "/Users/zp/.codex/pets",
            "",
            ControlSpec::Button("打开文件夹"),
        )],
    ),
    SectionSpec::new(
        "外观",
        "",
        &[RowSpec::new(
            "宠物大小",
            "调整宠物大小",
            ControlSpec::Value("112"),
        )],
    ),
];

const SHORTCUT_CHAT: &[RowSpec] = &[
    key!("新聊天", "开始新聊天", "⌘N · ⇧⌘O"),
    key!(
        "新建临时聊天",
        "开始聊天，此聊天不会显示在历史记录中",
        "⇧⌘N"
    ),
    key!("快速聊天", "在快速编辑器中开始轻量聊天", "⌥⌘N"),
    key!("归档聊天", "归档当前聊天", "⇧⌘A"),
    key!("新建独立聊天", "在项目外开始新聊天", "⌥⌘O"),
    key!("打开侧边聊天", "在侧边聊天中打开当前聊天", "⌥⌘S"),
    key!("标记为未读", "将当前聊天标记为未读", "⇧⌘U"),
    key!("在新窗口中打开", "在新窗口中打开当前聊天", "未分配"),
    key!("切换置顶状态", "置顶或取消置顶当前聊天", "⌥⌘P"),
    key!("重命名聊天", "重命名当前聊天", "⌥⌘R"),
    key!("分叉聊天", "为当前聊天创建分支", "未分配"),
];

const SHORTCUT_NAVIGATION: &[RowSpec] = &[
    key!("聚焦浏览器地址栏", "聚焦应用内浏览器地址栏", "⌘L"),
    key!("聚焦主聊天输入框", "将键盘焦点移至主聊天输入框", "未分配"),
    key!(
        "聚焦侧边聊天",
        "将键盘焦点移至已打开的侧边聊天输入框",
        "未分配"
    ),
    key!("转到行", "转到当前文件中的某一行", "⌘L"),
    key!("返回", "在导航历史记录中返回", "⌘[ · Mouse Back"),
    key!("前进", "在导航历史记录中前进", "⌘] · Mouse Forward"),
    key!(
        "下一个最近查看的聊天",
        "切换到下一个最近已查看的聊天",
        "⌃Tab"
    ),
    key!("下一个标签页", "切换到下一个标签页", "⌃Tab · ⇧⌘] · ⌥⌘Right"),
    key!("下一个聊天", "切换到下一个聊天", "⇧⌘] · ⌥⌘Right"),
    key!(
        "下一个需关注的聊天",
        "切换到下一个等待输入或有未读活动的聊天",
        "⌥⌘A"
    ),
    key!(
        "上一个最近查看的聊天",
        "切换到上一个最近已查看的聊天",
        "⌃⇧Tab"
    ),
    key!("上一个标签页", "切换到上一个标签页", "⌃⇧Tab · ⇧⌘[ · ⌥⌘Left"),
    key!("上一个聊天", "切换到上一个聊天", "⇧⌘[ · ⌥⌘Left"),
    key!("转到最近聊天 1", "打开此快捷槽位中最近更新的聊天", "⌥⌘1"),
    key!("转到最近聊天 2", "打开此快捷槽位中最近更新的聊天", "⌥⌘2"),
    key!("转到最近聊天 3", "打开此快捷槽位中最近更新的聊天", "⌥⌘3"),
    key!("转到最近聊天 4", "打开此快捷槽位中最近更新的聊天", "⌥⌘4"),
    key!("转到最近聊天 5", "打开此快捷槽位中最近更新的聊天", "⌥⌘5"),
    key!("转到最近聊天 6", "打开此快捷槽位中最近更新的聊天", "⌥⌘6"),
    key!("切换聊天…", "搜索并切换到聊天", "未分配"),
    key!("切换到聊天", "切换到聊天", "⌃1"),
    key!("切换到工作", "切换到工作", "⌃2"),
    key!("切换到 Codex", "切换到 Codex", "⌃3"),
];

const SHORTCUT_VIEW: &[RowSpec] = &[
    key!("切换活动视图", "开启或关闭侧边栏活动视图", "⌥⌘U"),
    key!("打开浏览器标签页", "打开新浏览器标签页", "⌘T"),
    key!("打开审查选项卡", "打开“审阅”选项卡", "⌃⇧G"),
    key!("重新打开已关闭的标签页", "重新打开最近关闭的标签页", "⇧⌘T"),
    key!("切换底部面板", "显示或隐藏底部面板", "⌘J"),
    key!("显示/隐藏浏览器面板", "显示或隐藏浏览器面板", "⇧⌘B"),
    key!("切换置顶摘要", "显示或隐藏已固定的摘要", "未分配"),
    key!(
        "切换审阅",
        "显示或隐藏当前 Git 支持的聊天中的“审阅”",
        "未分配"
    ),
    key!("切换侧边栏", "显示或隐藏侧边栏", "⌘B"),
    key!("切换审阅面板", "显示或隐藏当前聊天的审阅", "⌥⌘B"),
    key!("打开终端", "打开终端面板", "⌃`"),
];

const SHORTCUT_ENVIRONMENT: &[RowSpec] = &[
    key!("环境操作 1", "执行此快捷槽位中的环境操作", "⇧⌘D"),
    key!("环境操作 2", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 3", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 4", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 5", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 6", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 7", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 8", "执行此快捷槽位中的环境操作", "未分配"),
    key!("环境操作 9", "执行此快捷槽位中的环境操作", "未分配"),
    key!("提交或推送", "打开提交或推送选项", "未分配"),
    key!("创建分支", "打开分支创建选项", "未分配"),
    key!("创建草稿 PR", "打开草稿 Pull Request 创建选项", "未分配"),
    key!("创建 PR", "打开 Pull Request 创建选项", "未分配"),
    key!("合并 PR", "打开 Pull Request 合并选项", "未分配"),
    key!(
        "在 GitHub 上打开 PR",
        "打开与当前聊天关联的 Pull Request",
        "未分配"
    ),
];

const SHORTCUT_COMMANDS: &[RowSpec] = &[
    key!("打开文件夹", "将本地项目添加到 ChatGPT", "⌘O"),
    key!("强制重新加载技能", "刷新当前上下文的技能目录", "未分配"),
    key!("前往技能", "浏览已安装和推荐的技能", "未分配"),
    key!("从其他 AI 应用导入", "从其他 AI 应用导入", "未分配"),
    key!("键盘快捷方式", "自定义键盘快捷键", "未分配"),
    key!("MCP", "配置 MCP 服务器", "未分配"),
    key!("个性", "调整语气和回复风格", "未分配"),
    key!(
        "全部标为已读",
        "将所有聊天和已安排任务更新标记为已读",
        "⇧Esc"
    ),
    key!("反馈", "向 ChatGPT 团队发送产品反馈", "未分配"),
    key!("注销", "退出登录 ChatGPT", "未分配"),
    key!("管理已安排任务", "从当前页面创建或管理已安排任务", "未分配"),
    key!("显示宠物", "打开虚拟宠物浮层", "未分配"),
    key!("打开控制窗口", "打开语音聊天控制窗口", "未分配"),
    key!("重做上一步操作", "重做最近撤销的应用操作", "⇧⌘Z"),
    key!("设置", "打开 ChatGPT 设置", "⌘,"),
    key!("撤销上一步操作", "撤销最近的应用操作", "⌘Z"),
    key!("批准请求", "批准已开启的请求", "⏎"),
    key!("拒绝请求", "拒绝当前请求", "Esc"),
    key!("关闭标签页", "关闭当前标签页", "⌘W"),
    key!("关闭", "关闭活动窗口", "⌘W"),
];

const SHORTCUT_COMPOSER: &[RowSpec] = &[
    key!(
        "附加文件和文件夹",
        "将文件和文件夹附加到当前编辑器",
        "未分配"
    ),
    key!("添加照片", "将照片添加到当前编辑器", "未分配"),
    key!("清除提示", "清除当前编辑器中的提示", "未分配"),
    key!("循环切换推理强度", "循环切换编辑器推理强度选项", "未分配"),
    key!("降低推理强度", "降低当前编辑器推理强度", "未分配"),
    key!("提高推理强度", "提高当前编辑器推理强度", "未分配"),
    key!("打开模型选择器", "打开编辑器模型选择器", "⌃⇧M"),
    key!("打开项目选择器", "打开编辑器项目选择器", "⌥⇧⌘O"),
    key!(
        "将提示加入队列",
        "将当前编辑器提示作为排队消息提交",
        "未分配"
    ),
    key!("开始听写", "在当前编辑器中开始听写", "⌃⇧D"),
    key!("切换语音聊天", "开始或停止语音聊天", "⌃⇧V"),
    key!("调整提示方向", "将当前编辑器提示作为引导消息提交", "未分配"),
    key!("发送消息", "发送当前编辑器中的消息", "未分配"),
    key!(
        "在后台发送消息",
        "无需打开聊天即可发送当前编辑器中的消息",
        "⌘⏎"
    ),
    key!("切换快速模式", "在当前编辑器中开启或关闭快速模式", "未分配"),
    key!("切换规划模式", "在当前编辑器中开启或关闭方案模式", "未分配"),
    key!(
        "切换云端/本地",
        "切换 ChatGPT Work 的云端和本地执行",
        "未分配"
    ),
    key!(
        "切换本地/工作树",
        "在本地与新工作树之间切换当前编辑器",
        "未分配"
    ),
];

const SHORTCUT_COPY: &[RowSpec] = &[
    key!("复制为 Markdown", "将当前聊天复制为 Markdown", "未分配"),
    key!("复制对话路径", "复制当前聊天路径", "⌥⇧⌘C"),
    key!("复制深层链接", "复制当前聊天的深度链接", "⌥⌘L"),
    key!("复制会话 ID", "复制当前聊天会话 ID", "⌥⌘C"),
    key!("复制工作目录", "复制当前聊天的工作目录", "⇧⌘C"),
];

const SHORTCUT_GLOBAL: &[RowSpec] = &[
    key!(
        "按住听写快捷键",
        "在桌面任意位置按住，即可在光标位置听写",
        "⌃V"
    ),
    key!(
        "切换听写快捷键",
        "在桌面任意位置按一次开始听写，再次按下停止",
        "未分配"
    ),
    key!(
        "强制重新加载浏览器页面",
        "强制重新加载当前浏览器页面",
        "⇧⌘R"
    ),
    key!(
        "弹出窗口快捷键",
        "在桌面任意位置显示或隐藏弹出窗口",
        "未分配"
    ),
    key!("浏览器返回", "在浏览器历史记录中返回上一页", "⌘Left"),
    key!("浏览器前进", "在浏览器历史记录中前进", "⌘Right"),
    key!("新窗口", "打开新窗口", "未分配"),
    key!("打开命令菜单", "打开命令菜单", "⌘K · ⇧⌘P"),
    key!("语音聊天快捷键", "在桌面端任意位置发起语音聊天", "未分配"),
    key!("结束语音聊天", "结束当前语音聊天", "未分配"),
    key!(
        "切换语音聊天麦克风",
        "在语音聊天中将麦克风静音或取消静音",
        "未分配"
    ),
    key!("切换语音聊天音频", "将语音聊天音频静音或取消静音", "未分配"),
    key!("重新加载浏览器页面", "重新加载当前浏览器页面", "⌘R"),
    key!("搜索文件…", "搜索文件", "⌘P"),
    key!("显示键盘快捷键", "显示当前可用的快捷键", "⌘/"),
    key!("转到聊天 1", "打开此快捷槽位中可见的聊天", "⌘1"),
    key!("转到聊天 2", "打开此快捷槽位中可见的聊天", "⌘2"),
    key!("转到聊天 3", "打开此快捷槽位中可见的聊天", "⌘3"),
    key!("转到聊天 4", "打开此快捷槽位中可见的聊天", "⌘4"),
    key!("转到聊天 5", "打开此快捷槽位中可见的聊天", "⌘5"),
    key!("转到聊天 6", "打开此快捷槽位中可见的聊天", "⌘6"),
    key!("转到聊天 7", "打开此快捷槽位中可见的聊天", "⌘7"),
    key!("转到聊天 8", "打开此快捷槽位中可见的聊天", "⌘8"),
    key!("转到聊天 9", "打开此快捷槽位中可见的聊天", "⌘9"),
    key!("显示/隐藏文件树", "显示/隐藏文件树面板", "⇧⌘E"),
    key!("最大化/还原侧边面板", "展开或还原侧边面板", "未分配"),
    key!("开始跟踪记录", "开始或停止轨迹录制", "⇧⌘S"),
];

const SHORTCUTS: &[SectionSpec] = &[
    SectionSpec::new("聊天", "", SHORTCUT_CHAT),
    SectionSpec::new("导航", "", SHORTCUT_NAVIGATION),
    SectionSpec::new("视图", "", SHORTCUT_VIEW),
    SectionSpec::new("环境与 Git", "", SHORTCUT_ENVIRONMENT),
    SectionSpec::new("应用命令", "", SHORTCUT_COMMANDS),
    SectionSpec::new("编辑器", "", SHORTCUT_COMPOSER),
    SectionSpec::new("复制", "", SHORTCUT_COPY),
    SectionSpec::new("全局、浏览器与语音", "", SHORTCUT_GLOBAL),
];

const USAGE: &[SectionSpec] = &[
    SectionSpec::new(
        "当前套餐",
        "",
        &[RowSpec::new(
            "Pro 套餐",
            "₱9,990/月",
            ControlSpec::Button("查看套餐"),
        )],
    ),
    SectionSpec::new(
        "额度余额",
        "购买额度或启用自动充值，达到限额后仍可继续使用 Codex。了解更多",
        &[
            RowSpec::new("PHP 0", "当前余额", ControlSpec::Button("购买额度")),
            RowSpec::new(
                "自动充值",
                "达到上限后仍可继续工作 · 最高可享 40% 折扣",
                ControlSpec::Switch(false),
            ),
            RowSpec::new("为他人购买额度", "", ControlSpec::Button("赠送额度")),
        ],
    ),
    SectionSpec::new(
        "通用使用限额",
        "",
        &[RowSpec::new(
            "每周使用限额",
            "重置时间：2026年9月1日 22:15",
            ControlSpec::Value("剩余 81%"),
        )],
    ),
    SectionSpec::new(
        "GPT-5.3-Codex-Spark 使用限额",
        "",
        &[
            RowSpec::new(
                "5 小时使用限额",
                "重置时间：03:29",
                ControlSpec::Value("剩余 100%"),
            ),
            RowSpec::new(
                "每周使用限额",
                "重置时间：2026年9月2日 22:29",
                ControlSpec::Value("剩余 100%"),
            ),
        ],
    ),
    SectionSpec::new(
        "使用限额重置",
        "",
        &[RowSpec::new(
            "完全重置",
            "将于 9/21 GMT+8 08:24 到期",
            ControlSpec::Button("使用重置额度"),
        )],
    ),
    SectionSpec::new(
        "取消套餐",
        "",
        &[RowSpec::new(
            "您的订阅由 ChatGPT 管理。",
            "如需取消套餐，请前往账单操作。",
            ControlSpec::None,
        )],
    ),
];

const COMPUTER_USE: &[SectionSpec] = &[
    SectionSpec::new(
        "控制",
        "",
        &[
            RowSpec::new(
                "任意应用",
                "允许 ChatGPT 控制您电脑上的应用",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Google Chrome",
                "已安装浏览器扩展程序",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "更多浏览器",
                "为更多浏览器设置扩展程序",
                ControlSpec::Button("管理"),
            ),
            RowSpec::new(
                "Microsoft Excel",
                "允许 ChatGPT 使用 Microsoft Excel 加载项以获得更多控制权限",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "锁屏操作",
                "允许 ChatGPT 在 Mac 锁定时使用此 Mac。了解更多",
                ControlSpec::Switch(false),
            ),
        ],
    ),
    SectionSpec::new(
        "始终允许的应用",
        "",
        &[RowSpec::new("暂无", "", ControlSpec::None)],
    ),
];

const CHRONICLE: &[SectionSpec] = &[SectionSpec::new(
    "让 ChatGPT 关注你的工作",
    "ChatGPT 可以总结你在所用应用和网站中的活动，且绝不会录制你的屏幕或音频。",
    &[
        RowSpec::new(
            "开启",
            "询问你之前正在处理的事项，无需再次解释一切即可获得帮助，或发现自动化重复任务的机会。",
            ControlSpec::Button("开启"),
        ),
        RowSpec::new(
            "昨天会后，我答应给 Sarah 发送什么？",
            "你在 Slack 上告诉 Sarah，会在周五前发送 Q3 预算。最新版在 Google Sheets 中，你在 Google Docs 中的会议记录列出两个待确认的数字：招聘和差旅。",
            ControlSpec::None,
        ),
        RowSpec::new(
            "开启后，ChatGPT 会保存活动文本摘要",
            "可能包括通信内容。音频和私密模式网页浏览绝不会包含在内；你可以随时暂停或清除历史记录，并管理包含的内容。此功能会增加 Token 用量。了解更多",
            ControlSpec::None,
        ),
    ],
)];

const APPSHOTS: &[SectionSpec] = &[SectionSpec::new(
    "截取应用快照，向 ChatGPT 展示你最前端的窗口",
    "智能快照包含视觉和文本内容，包括滚动到屏幕外的文本。",
    &[
        RowSpec::new("快捷键", "同时按下两个 ⌘ 键", ControlSpec::Select("⌘ + ⌘")),
        RowSpec::new(
            "Appshot 发送目标",
            "选择使用快捷键时将 appshots 发送到哪里",
            ControlSpec::Select("自动"),
        ),
        RowSpec::new("播放音效", "", ControlSpec::Switch(true)),
    ],
)];

const PLUGINS: &[SectionSpec] = &[
    SectionSpec::new(
        "管理插件、技能和 MCP",
        "插件 14 · 应用 8 · MCP 4 · 技能 2",
        &[
            RowSpec::new("浏览目录", "", ControlSpec::Button("浏览目录")),
            RowSpec::new("添加", "", ControlSpec::Button("添加")),
            RowSpec::new("搜索插件", "", ControlSpec::None),
        ],
    ),
    SectionSpec::new(
        "插件",
        "",
        &[
            RowSpec::new("Gmail", "Read and manage Gmail", ControlSpec::Switch(true)),
            RowSpec::new(
                "GitHub",
                "Triage PRs, issues, CI, and publish flows",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Figma",
                "Figma design-to-code workflows",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Zotero",
                "Find papers and add citations from Zotero",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Default templates",
                "Default templates for documents, spreadsheets, and presentations",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Plugin Management",
                "Discover and manage plugins",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Documents",
                "Create and edit documents",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "PDF",
                "Read, create, and verify PDFs",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Spreadsheets",
                "Create and edit spreadsheets",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Presentations",
                "Create and edit presentations",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Template Creator",
                "Create or update reusable templates from reference content",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Sites",
                "Build and deploy websites",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Computer Use",
                "Control Mac apps from ChatGPT",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "Visualize",
                "Create interactive visuals",
                ControlSpec::Switch(true),
            ),
        ],
    ),
];

pub const PAGES: &[PageSpec] = &[
    PageSpec::new("pets", "智能伙伴", "", PageKind::Pets, PETS),
    PageSpec::new(
        "keyboard-shortcuts",
        "键盘快捷键",
        "",
        PageKind::KeyboardShortcuts,
        SHORTCUTS,
    ),
    PageSpec::new(
        "usage",
        "使用情况和计费",
        "如需查看发票、更改付款方式或进行其他操作，请前往网页版设置",
        PageKind::Usage,
        USAGE,
    ),
    PageSpec::new(
        "computer-use",
        "电脑操控",
        "管理 ChatGPT 如何使用你电脑上的其他应用程序",
        PageKind::Standard,
        COMPUTER_USE,
    ),
    PageSpec::new(
        "chronicle",
        "计算机历史记录",
        "",
        PageKind::Standard,
        CHRONICLE,
    ),
    PageSpec::new("appshots", "应用快照", "", PageKind::Standard, APPSHOTS),
    PageSpec::new("plugins-settings", "插件", "", PageKind::Standard, PLUGINS),
];
