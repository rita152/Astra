use super::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec};

const GENERAL_PERMISSION_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "默认权限",
        "默认情况下，ChatGPT 可以读取和编辑其工作空间中的文件。需要时，它可以请求额外访问权限",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "完整访问权限",
        "当 ChatGPT 以完整访问权限运行时，它无需你的批准即可编辑你电脑上的任何文件，并运行可访问网络的命令。这会显著增加数据丢失、泄露或意外行为的风险。了解更多关于风险升高的信息。",
        ControlSpec::Switch(true),
    ),
];

const GENERAL_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "Projectless task folder",
        "The location where tasks started outside of projects store their data by default.",
        ControlSpec::Value("/Users/zp/Documents/Codex  ·  更改"),
    ),
    RowSpec::new(
        "默认文件打开位置",
        "默认打开文件和文件夹的位置",
        ControlSpec::Select("Cursor"),
    ),
    RowSpec::new("语言", "应用 UI 语言", ControlSpec::Select("自动检测")),
    RowSpec::new(
        "在菜单栏中显示",
        "关闭主窗口后，仍在 macOS 菜单栏中保留 ChatGPT",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "底部面板",
        "在应用标题栏中显示底部面板控件",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "默认终端位置",
        "选择终端快捷键和环境操作在何处打开终端标签页",
        ControlSpec::Segmented(&["底部", "右侧"], 0),
    ),
    RowSpec::new(
        "运行时防止系统休眠",
        "在 ChatGPT 运行任务时，让电脑保持唤醒状态",
        ControlSpec::Switch(false),
    ),
    RowSpec::new(
        "速度",
        "选择 ChatGPT 在聊天、子智能体和压缩中的运行速度",
        ControlSpec::Select("快速"),
    ),
    RowSpec::new(
        "提示词建议",
        "通过搜索项目文件和已连接的应用，建议下一步操作",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "打开源许可证",
        "捆绑依赖项的第三方声明",
        ControlSpec::Button("查看"),
    ),
    RowSpec::new(
        "插件",
        "允许 ChatGPT 使用已安装插件",
        ControlSpec::Switch(true),
    ),
];

const GENERAL_EDITOR_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "纯文本编辑器",
        "编写消息时，将代码、Markdown 和链接保留为纯文本",
        ControlSpec::Switch(false),
    ),
    RowSpec::new("显示上下文窗口使用情况", "", ControlSpec::Switch(true)),
    RowSpec::new(
        "发送快捷键",
        "选择按 Enter 时是发送提示还是插入新行",
        ControlSpec::Select("按 Enter 键"),
    ),
    RowSpec::new(
        "跟进处理方式",
        "在 ChatGPT 运行时将后续消息加入队列，或调整当前运行的方向。按 ⌘⏎ 可对单条消息执行相反操作",
        ControlSpec::Segmented(&["加入队列", "调整方向"], 0),
    ),
];

const GENERAL_POPOUT_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "弹出窗口快捷键",
        "为弹出窗口设置全局快捷键。不设置则保持关闭。",
        ControlSpec::Shortcut("关闭"),
    ),
    RowSpec::new(
        "默认使用独立聊天",
        "在任何项目外开始新聊天",
        ControlSpec::Switch(false),
    ),
];

const GENERAL_NOTIFICATION_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "轮次完成通知",
        "设置 ChatGPT 完成后何时提醒你",
        ControlSpec::Select("仅在未聚焦时"),
    ),
    RowSpec::new(
        "启用权限通知",
        "在需要通知权限时显示提醒",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "启用问题通知",
        "需要输入才能继续时显示提醒",
        ControlSpec::Switch(true),
    ),
];

const GENERAL_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("权限", "", GENERAL_PERMISSION_ROWS),
    SectionSpec::new("常规", "", GENERAL_ROWS),
    SectionSpec::new("编辑器", "", GENERAL_EDITOR_ROWS),
    SectionSpec::new("弹出窗口", "", GENERAL_POPOUT_ROWS),
    SectionSpec::new("通知", "", GENERAL_NOTIFICATION_ROWS),
];

const IMPORT_SYNC_ROWS: &[RowSpec] = &[RowSpec::new(
    "保持导入同步",
    "自动同步已连接来源中的新增和更新内容",
    ControlSpec::Switch(false),
)];

const IMPORT_SOURCE_ROWS: &[RowSpec] = &[
    RowSpec::new("Claude Code", "", ControlSpec::Button("导入")),
    RowSpec::new("Claude Cowork", "", ControlSpec::Button("导入")),
    RowSpec::new("Cursor", "", ControlSpec::Button("导入")),
];

const IMPORT_HISTORY_ROWS: &[RowSpec] = &[RowSpec::new(
    "2026年7月12日 14:08",
    "已导入 1 项",
    ControlSpec::Value("MCP 服务器  ·  已导入 1 项"),
)];

const IMPORT_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("开启自动同步", "", IMPORT_SYNC_ROWS),
    SectionSpec::new(
        "从其他 AI 应用导入",
        "检测到可添加到 ChatGPT 的配置",
        IMPORT_SOURCE_ROWS,
    ),
    SectionSpec::new("导入历史", "", IMPORT_HISTORY_ROWS),
];

const PROFILE_IDENTITY_ROWS: &[RowSpec] = &[
    RowSpec::new("rita", "@zb3242957365  ·  Pro", ControlSpec::Button("编辑")),
    RowSpec::new("可见性", "私有", ControlSpec::Button("分享")),
    RowSpec::new("邀请好友", "", ControlSpec::Button("邀请好友")),
];

const PROFILE_STATS_ROWS: &[RowSpec] = &[
    RowSpec::new("累计 Token 数", "", ControlSpec::Value("157亿")),
    RowSpec::new("峰值 Token 数", "", ControlSpec::Value("13.6亿")),
    RowSpec::new("最长聊天时长", "", ControlSpec::Value("10 小时 28 分")),
    RowSpec::new("当前连续天数", "", ControlSpec::Value("28 天")),
    RowSpec::new("最长连续天数", "", ControlSpec::Value("28 天")),
];

const PROFILE_ACTIVITY_ROWS: &[RowSpec] = &[RowSpec::new(
    "视图",
    "9月 · 10月 · 11月 · 12月 · 1月 · 2月 · 3月 · 4月 · 5月 · 6月 · 7月 · 8月",
    ControlSpec::Segmented(&["每日", "每周", "累计"], 0),
)];

const PROFILE_INSIGHT_ROWS: &[RowSpec] = &[
    RowSpec::new("快速模式", "", ControlSpec::Value("17%")),
    RowSpec::new("最常用的推理强度", "", ControlSpec::Value("最高 · 91%")),
    RowSpec::new("已探索的技能", "", ControlSpec::Value("53")),
    RowSpec::new("使用的技能总数", "", ControlSpec::Value("962")),
    RowSpec::new("聊天总数", "", ControlSpec::Value("1,806")),
];

const PROFILE_PLUGIN_ROWS: &[RowSpec] = &[
    RowSpec::new("$git-commit-message", "", ControlSpec::Value("156 次运行")),
    RowSpec::new("$codebase-design", "", ControlSpec::Value("127 次运行")),
    RowSpec::new("$openai-docs", "", ControlSpec::Value("123 次运行")),
    RowSpec::new("$tdd", "", ControlSpec::Value("68 次运行")),
    RowSpec::new("$ui-ux-pro-max", "", ControlSpec::Value("64 次运行")),
];

const PROFILE_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("", "", PROFILE_IDENTITY_ROWS),
    SectionSpec::new("", "", PROFILE_STATS_ROWS),
    SectionSpec::new("Token 活动", "", PROFILE_ACTIVITY_ROWS),
    SectionSpec::new("活动洞察", "", PROFILE_INSIGHT_ROWS),
    SectionSpec::new("最常用的插件", "", PROFILE_PLUGIN_ROWS),
];

const APPEARANCE_THEME_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "主题",
        "",
        ControlSpec::Segmented(&["系统", "浅色", "深色"], 2),
    ),
    RowSpec::new(
        "深色主题",
        "Aa  Codex",
        ControlSpec::Segmented(&["导入", "复制主题"], 0),
    ),
    RowSpec::new("强调色", "深色 强调色", ControlSpec::Value("#339CFF")),
    RowSpec::new("背景", "深色 背景颜色", ControlSpec::Value("#181818")),
    RowSpec::new("前景", "深色 墨迹颜色", ControlSpec::Value("#FFFFFF")),
    RowSpec::new("UI 字体", "系统默认", ControlSpec::Select("常规")),
    RowSpec::new("代码字体", "系统默认", ControlSpec::Select("常规")),
    RowSpec::new("半透明侧边栏", "", ControlSpec::Switch(true)),
    RowSpec::new("对比度", "", ControlSpec::Value("60")),
];

const APPEARANCE_PREFERENCE_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "使用指针光标",
        "悬停交互元素时切换为指针光标",
        ControlSpec::Switch(false),
    ),
    RowSpec::new(
        "Dock 图标",
        "选择应用在 Dock 中使用的图标",
        ControlSpec::Segmented(&["ChatGPT", "Codex"], 0),
    ),
    RowSpec::new(
        "减少动态效果",
        "减少动画效果或匹配系统设置",
        ControlSpec::Segmented(&["系统", "开启", "关闭"], 0),
    ),
    RowSpec::new(
        "UI 字号",
        "调整 ChatGPT 界面使用的基准字号",
        ControlSpec::Value("14 px"),
    ),
    RowSpec::new(
        "代码字体大小",
        "调整聊天和差异视图中代码使用的基础字号",
        ControlSpec::Value("12 px"),
    ),
    RowSpec::new(
        "差异标记",
        "使用颜色或 +/− 标记显示更改",
        ControlSpec::Segmented(&["颜色", "+/-"], 0),
    ),
    RowSpec::new(
        "字体平滑",
        "使用 macOS 原生字体抗锯齿",
        ControlSpec::Switch(true),
    ),
];

const APPEARANCE_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("", "", APPEARANCE_THEME_ROWS),
    SectionSpec::new("偏好设置", "", APPEARANCE_PREFERENCE_ROWS),
];

const VOICE_GENERAL_ROWS: &[RowSpec] = &[RowSpec::new(
    "麦克风",
    "用于语音聊天和听写",
    ControlSpec::Select("系统默认"),
)];

const VOICE_CHAT_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "语音",
        "选择 Codex 在新语音聊天中使用的语音",
        ControlSpec::Select("Maple"),
    ),
    RowSpec::new(
        "语音聊天热键",
        "在桌面端任意位置启动语音聊天",
        ControlSpec::Shortcut("关闭"),
    ),
    RowSpec::new(
        "屏幕上下文",
        "当你提到屏幕上的内容时，允许 Codex 查看前台应用。Codex 首次需要访问时，macOS 会请求权限。",
        ControlSpec::Switch(true),
    ),
];

const VOICE_DICTATION_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "按住听写快捷键",
        "在桌面任意位置按住，即可在光标处听写",
        ControlSpec::Shortcut("⌃V"),
    ),
    RowSpec::new(
        "切换听写快捷键",
        "在桌面任意位置按一次开始听写，再按一次停止",
        ControlSpec::Shortcut("关闭"),
    ),
    RowSpec::new(
        "保持听写栏可见",
        "听写未录制时显示小型快捷键提醒",
        ControlSpec::Switch(false),
    ),
    RowSpec::new(
        "播放听写音效",
        "听写开始和停止时播放提示音",
        ControlSpec::Switch(true),
    ),
];

const VOICE_DICTIONARY_ROWS: &[RowSpec] = &[RowSpec::new(
    "听写词典",
    "听写应能识别的单词或短语",
    ControlSpec::Button("添加条目"),
)];

const VOICE_RECORDING_ROWS: &[RowSpec] = &[
    RowSpec::new("录音已取消", "8月26日 18:54", ControlSpec::Button("重试")),
    RowSpec::new("录音已取消", "8月26日 18:54", ControlSpec::Button("重试")),
    RowSpec::new("Yeah.", "8月6日 18:28", ControlSpec::None),
    RowSpec::new(
        "现在bug walkthrough都已经完成了,请你进行review,如果没有问题的话,就直接合并到main上。",
        "8月3日 23:22",
        ControlSpec::None,
    ),
    RowSpec::new("Yeah.", "7月31日 21:25", ControlSpec::None),
    RowSpec::new(
        "那么既然现在greedy presets的性能和差异性已经被验证了,那么接下来可以在这个greedy presets的基础上来构建square thread和experts global这两组专家策略了。",
        "7月29日 18:36",
        ControlSpec::None,
    ),
    RowSpec::new("Yeah.", "7月29日 16:28", ControlSpec::None),
];

const VOICE_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("常规", "", VOICE_GENERAL_ROWS),
    SectionSpec::new("语音聊天", "", VOICE_CHAT_ROWS),
    SectionSpec::new("听写", "", VOICE_DICTATION_ROWS),
    SectionSpec::new("", "", VOICE_DICTIONARY_ROWS),
    SectionSpec::new(
        "最近录音",
        "你最近的 20 段录音会保存在此设备上",
        VOICE_RECORDING_ROWS,
    ),
];

const AGENT_DEFAULT_ROWS: &[RowSpec] = &[
    RowSpec::new("用户配置", "", ControlSpec::Button("打开 config.toml")),
    RowSpec::new(
        "批准策略",
        "选择 ChatGPT 何时请求批准",
        ControlSpec::Select("按请求"),
    ),
    RowSpec::new(
        "沙盒设置",
        "选择 ChatGPT 运行命令时的权限范围",
        ControlSpec::Select("完整访问权限"),
    ),
    RowSpec::new(
        "网页搜索",
        "选择 ChatGPT 访问网络的方式",
        ControlSpec::Select("实时"),
    ),
    RowSpec::new(
        "输出详细程度",
        "选择 ChatGPT 回复包含细节的详细程度",
        ControlSpec::Select("模型默认"),
    ),
    RowSpec::new(
        "推理摘要",
        "选择 ChatGPT 总结其推理的方式",
        ControlSpec::Select("自动"),
    ),
];

const AGENT_MODEL_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "可用推理强度",
        "选择在模型控件中显示哪些推理强度级别。可用性因模型而异",
        ControlSpec::Button("已选择 6 个"),
    ),
    RowSpec::new(
        "模型选择器滑块中的 Ultra",
        "将 Ultra 显示为滑块最高档选项",
        ControlSpec::Switch(true),
    ),
];

const AGENT_DEPENDENCY_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "Codex 依赖项",
        "允许 ChatGPT 安装并提供随附的 Node.js 和 Python 工具",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "诊断 Codex 工作空间中的问题",
        "检查当前捆绑包并记录诊断日志",
        ControlSpec::Button("诊断"),
    ),
    RowSpec::new(
        "重置并安装工作空间",
        "下载新的软件包并安装，然后重新加载工具",
        ControlSpec::Button("重新安装"),
    ),
    RowSpec::new("当前版本：", "", ControlSpec::Value("26.819.11345")),
];

const AGENT_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("智能体默认设置", "", AGENT_DEFAULT_ROWS),
    SectionSpec::new("模型功能", "", AGENT_MODEL_ROWS),
    SectionSpec::new("工作空间依赖项", "", AGENT_DEPENDENCY_ROWS),
];

const PERSONALIZATION_INSTRUCTION_ROWS: &[RowSpec] = &[RowSpec::new(
    "自定义指令",
    "向 ChatGPT 提供适用于此主机上所有聊天的额外说明和上下文。了解更多",
    ControlSpec::Button("保存"),
)];

const PERSONALIZATION_MEMORY_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "启用本地记忆",
        "根据此电脑上的聊天创建记忆，并用于个性化该电脑上的后续聊天",
        ControlSpec::Switch(false),
    ),
    RowSpec::new(
        "允许基于工具辅助聊天生成本地记忆",
        "从使用过 MCP 工具或网页搜索的聊天生成记忆",
        ControlSpec::Switch(true),
    ),
    RowSpec::new(
        "删除本地记忆",
        "删除存储在此电脑本地的所有记忆",
        ControlSpec::Danger("删除"),
    ),
];

const PERSONALIZATION_PERSONALITY_ROWS: &[RowSpec] = &[RowSpec::new(
    "个性",
    "选择 ChatGPT 回复的默认语气",
    ControlSpec::Select("亲和"),
)];

const PERSONALIZATION_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new("", "", PERSONALIZATION_INSTRUCTION_ROWS),
    SectionSpec::new(
        "记忆",
        "设置在此电脑上如何收集、保留和整合本地记忆。了解更多",
        PERSONALIZATION_MEMORY_ROWS,
    ),
    SectionSpec::new(
        "",
        "并非所有模型都支持个性设置。可在自定义指令中调整 Codex 的语气。",
        PERSONALIZATION_PERSONALITY_ROWS,
    ),
];

pub const PAGES: &[PageSpec] = &[
    PageSpec::new(
        "general-settings",
        "常规",
        "",
        PageKind::Standard,
        GENERAL_SECTIONS,
    ),
    PageSpec::new(
        "import",
        "导入",
        "将其他 AI 应用的设置、项目和聊天导入 ChatGPT",
        PageKind::Standard,
        IMPORT_SECTIONS,
    ),
    PageSpec::new(
        "profile",
        "个人资料",
        "",
        PageKind::Profile,
        PROFILE_SECTIONS,
    ),
    PageSpec::new(
        "appearance",
        "外观",
        "",
        PageKind::Standard,
        APPEARANCE_SECTIONS,
    ),
    PageSpec::new("voice", "语音", "", PageKind::Standard, VOICE_SECTIONS),
    PageSpec::new(
        "agent",
        "配置",
        "配置新聊天的权限、网页访问和智能体回复  了解更多",
        PageKind::Standard,
        AGENT_SECTIONS,
    ),
    PageSpec::new(
        "personalization",
        "个性化",
        "",
        PageKind::Standard,
        PERSONALIZATION_SECTIONS,
    ),
];
