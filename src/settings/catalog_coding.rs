use super::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec};

const BROWSER_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new(
        "",
        "",
        &[RowSpec::new(
            "浏览器",
            "让 ChatGPT 控制内置浏览器",
            ControlSpec::Switch(true),
        )],
    ),
    SectionSpec::new(
        "常规",
        "",
        &[
            RowSpec::new("导入…", "", ControlSpec::Button("导入…")),
            RowSpec::new(
                "网页 URL 和链接打开位置",
                "链接默认打开位置",
                ControlSpec::Select("默认浏览器"),
            ),
            RowSpec::new(
                "本地 URL 打开位置",
                "本地开发站点默认打开位置",
                ControlSpec::Select("ChatGPT"),
            ),
            RowSpec::new(
                "浏览数据",
                "清除应用内浏览器中的浏览历史记录、网站数据、缓存和下载历史记录",
                ControlSpec::Button("清除浏览数据"),
            ),
            RowSpec::new(
                "浏览历史",
                "查看和管理在内置浏览器中访问过的页面",
                ControlSpec::Button("管理"),
            ),
            RowSpec::new(
                "批注截图",
                "截图可帮助 ChatGPT 更好地理解和处理评论，但会增加套餐用量",
                ControlSpec::Select("始终包含"),
            ),
        ],
    ),
    SectionSpec::new(
        "自动填充和密码",
        "",
        &[
            RowSpec::new(
                "密码管理器",
                "添加、删除和编辑已保存的密码",
                ControlSpec::Button("管理"),
            ),
            RowSpec::new(
                "联系信息",
                "添加、删除和编辑已保存的地址、电话号码和电子邮箱地址",
                ControlSpec::Button("管理"),
            ),
        ],
    ),
    SectionSpec::new(
        "下载",
        "",
        &[
            RowSpec::new("位置", "系统下载文件夹", ControlSpec::Button("更改")),
            RowSpec::new(
                "下载前询问保存位置",
                "对在内置浏览器中发起的下载显示保存对话框",
                ControlSpec::Switch(false),
            ),
            RowSpec::new(
                "下载历史记录",
                "查看和管理从内置浏览器下载的文件",
                ControlSpec::Button("管理"),
            ),
        ],
    ),
    SectionSpec::new(
        "权限",
        "",
        &[
            RowSpec::new(
                "网站设置",
                "管理内置浏览器中的摄像头和麦克风权限",
                ControlSpec::Button("管理"),
            ),
            RowSpec::new(
                "审批",
                "选择 ChatGPT 在打开网站前是否请求批准。了解更多",
                ControlSpec::Select("始终询问"),
            ),
            RowSpec::new(
                "历史记录",
                "选择 ChatGPT 是否可访问你的内置浏览器历史记录",
                ControlSpec::Select("始终询问"),
            ),
            RowSpec::new(
                "下载",
                "选择 ChatGPT 从网站下载文件前是否先询问",
                ControlSpec::Select("始终询问"),
            ),
            RowSpec::new(
                "上传",
                "选择 ChatGPT 在将文件上传到网站前是否先询问",
                ControlSpec::Select("始终询问"),
            ),
            RowSpec::new(
                "启用站点工具",
                "允许 ChatGPT 发现并调用网站公开的站点工具，包括 WebMCP",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "网站权限",
                "为特定网站覆盖上述默认设置",
                ControlSpec::Button("添加"),
            ),
            RowSpec::new("尚无网站专属权限", "", ControlSpec::None),
        ],
    ),
    SectionSpec::new(
        "开发者模式",
        "风险升高",
        &[RowSpec::new(
            "启用完整 CDP 访问权限",
            "允许 ChatGPT 在已连接的 Browser Use 会话中使用完整的 Chrome DevTools Protocol (CDP) 访问权限。完整 CDP 访问权限可让 ChatGPT 检查并控制敏感的浏览器内部功能，可能使你的数据面临风险。",
            ControlSpec::Switch(true),
        )],
    ),
];

const HOOKS_SECTIONS: &[SectionSpec] = &[SectionSpec::new(
    "未找到钩子",
    "已配置的钩子将显示在此处",
    &[],
)];

const CONNECTION_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new(
        "",
        "",
        &[RowSpec::new(
            "连接模式",
            "",
            ControlSpec::Segmented(&["控制这台 Mac", "控制其他设备"], 0),
        )],
    ),
    SectionSpec::new(
        "可控制这台 Mac 的设备",
        "SSH",
        &[
            RowSpec::new("允许连接", "", ControlSpec::Switch(true)),
            RowSpec::new(
                "Android 16 24129PN74C",
                "上次连接时间 1 周",
                ControlSpec::Danger("撤销访问权限"),
            ),
            RowSpec::new("添加设备", "", ControlSpec::Button("添加")),
        ],
    ),
    SectionSpec::new(
        "其他设置",
        "",
        &[RowSpec::new(
            "让这台 Mac 保持唤醒状态",
            "当电脑接通电源且启用远程访问时，防止其进入睡眠状态",
            ControlSpec::Switch(false),
        )],
    ),
];

const GIT_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new(
        "",
        "",
        &[
            RowSpec::new(
                "分支前缀",
                "ChatGPT 创建新分支时使用的前缀",
                ControlSpec::Value("codex/"),
            ),
            RowSpec::new(
                "拉取请求合并方法",
                "选择 ChatGPT 合并拉取请求的方式",
                ControlSpec::Segmented(&["合并", "压缩合并"], 0),
            ),
            RowSpec::new(
                "始终强制推送",
                "从 ChatGPT 推送时使用 --force-with-lease",
                ControlSpec::Switch(false),
            ),
            RowSpec::new(
                "创建草稿拉取请求",
                "从 ChatGPT 创建 PR 时默认使用草稿拉取请求",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "审查结果呈现方式",
                "尽可能在当前聊天中启动 /review，或启动单独的审查聊天",
                ControlSpec::Segmented(&["内联", "单独"], 0),
            ),
        ],
    ),
    SectionSpec::new(
        "监控并修复 Pull Request",
        "",
        &[
            RowSpec::new("准备就绪时自动合并", "", ControlSpec::Switch(false)),
            RowSpec::new(
                "继续监控，直到 Pull Request 合并",
                "例如：检查通过后评论 /merge，并批准不相关的 Chromatic 变更…",
                ControlSpec::None,
            ),
        ],
    ),
    SectionSpec::new(
        "提交说明",
        "将添加到提交信息生成提示中",
        &[RowSpec::new("添加提交信息指引…", "", ControlSpec::None)],
    ),
    SectionSpec::new(
        "拉取请求说明",
        "将添加到 PR 标题/描述生成提示中",
        &[RowSpec::new("添加拉取请求指引…", "", ControlSpec::None)],
    ),
];

const ENVIRONMENT_ROWS: &[RowSpec] = &[
    RowSpec::new("GPUI", "", ControlSpec::Button("+")),
    RowSpec::new("oh-my-pi", "can1357", ControlSpec::Button("+")),
    RowSpec::new(
        "飞行器设计大赛",
        "deepseek-harness",
        ControlSpec::Button("+"),
    ),
    RowSpec::new("deepseek-harness", "deepseek-ai", ControlSpec::Button("+")),
    RowSpec::new("coda", "rita152", ControlSpec::Button("+")),
    RowSpec::new("pi", "rita152", ControlSpec::Button("+")),
    RowSpec::new(
        "codex",
        "openai · environment.toml",
        ControlSpec::Button("+"),
    ),
    RowSpec::new("LAG", "rita152", ControlSpec::Button("+")),
    RowSpec::new("语音输入法", "", ControlSpec::Button("+")),
    RowSpec::new("LAG_创新", "liuqh16", ControlSpec::Button("+")),
];

const ENVIRONMENT_SECTIONS: &[SectionSpec] = &[SectionSpec::new("选择项目", "", ENVIRONMENT_ROWS)];

const WORKTREE_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new(
        "",
        "",
        &[
            RowSpec::new(
                "工作树根目录",
                "ChatGPT 创建托管工作树的目录。留空则使用默认位置",
                ControlSpec::Value("/Users/zp/.codex/worktrees"),
            ),
            RowSpec::new(
                "创建工作树前始终获取上游更新",
                "Codex 通常会在常规 Git 操作中获取分支更新。此设置还会在创建每个新工作树前获取上游更新。",
                ControlSpec::Switch(false),
            ),
            RowSpec::new(
                "自动删除旧工作树",
                "推荐大多数用户启用。仅当你需要手动管理旧工作树和磁盘使用空间时，再关闭此功能。",
                ControlSpec::Switch(true),
            ),
            RowSpec::new(
                "自动删除限制",
                "要保留的托管工作树数量；超过后，较旧的工作树会自动被清理。ChatGPT 会在删除工作树前创建快照，因此被清理的工作树应始终可以恢复。",
                ControlSpec::Value("15"),
            ),
        ],
    ),
    SectionSpec::new(
        "/Users/zp/.codex/worktrees/1ee3/openai-sdk-ts",
        "工作树",
        &[
            RowSpec::new(
                "使用相同文件和分支开始全新聊天",
                "",
                ControlSpec::Button("在此工作树中新建聊天"),
            ),
            RowSpec::new("删除", "", ControlSpec::Danger("删除")),
            RowSpec::new("对话", "移除权限系统：协议与能力层", ControlSpec::None),
        ],
    ),
    SectionSpec::new(
        "/Users/zp/.codex/worktrees/5592/openai-sdk-ts",
        "工作树",
        &[
            RowSpec::new(
                "使用相同文件和分支开始全新聊天",
                "",
                ControlSpec::Button("在此工作树中新建聊天"),
            ),
            RowSpec::new("删除", "", ControlSpec::Danger("删除")),
            RowSpec::new("对话", "移除权限系统：CLI、测试与文档", ControlSpec::None),
        ],
    ),
    SectionSpec::new(
        "/Users/zp/.codex/worktrees/fe6f/openai-sdk-ts",
        "工作树",
        &[
            RowSpec::new(
                "使用相同文件和分支开始全新聊天",
                "",
                ControlSpec::Button("在此工作树中新建聊天"),
            ),
            RowSpec::new("删除", "", ControlSpec::Danger("删除")),
            RowSpec::new(
                "对话",
                "移除权限系统：Runtime 与 Session",
                ControlSpec::None,
            ),
        ],
    ),
];

const ARCHIVED_ROWS: &[RowSpec] = &[
    RowSpec::new(
        "配置1v1 DAgger训练环境",
        "2026年8月26日，17:16",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "排查双服务器SSH断连原因",
        "2026年8月26日，15:08",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "配置并验证1v1 DAgger环境",
        "2026年8月26日，13:03",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "确定 AutoDL 训练环境",
        "2026年8月25日，21:26",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "排查SSH断连原因",
        "2026年8月25日，17:03",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "确认训练服务器要求",
        "2026年8月25日，14:07",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "说明1v1 DAgger训练流程",
        "2026年8月25日，2:41",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "评估1v1 DAgger训练准入",
        "2026年8月25日，2:20",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "重构 AGENTS 项目入口",
        "2026年8月24日，22:29",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "确认可见的非归档对话",
        "2026年8月24日，22:08",
        ControlSpec::Button("取消归档"),
    ),
    RowSpec::new(
        "实现1v1 DAgger训练闭环",
        "2026年8月24日，1:40",
        ControlSpec::Button("取消归档"),
    ),
];

const DATA_CONTROL_SECTIONS: &[SectionSpec] = &[
    SectionSpec::new(
        "",
        "",
        &[
            RowSpec::new("搜索已归档聊天", "", ControlSpec::Value("")),
            RowSpec::new("范围", "", ControlSpec::Select("全部聊天")),
            RowSpec::new("项目", "", ControlSpec::Select("所有项目")),
            RowSpec::new("全部删除", "", ControlSpec::Danger("全部删除")),
        ],
    ),
    SectionSpec::new("LAG", "49 个聊天", ARCHIVED_ROWS),
];

pub const PAGES: &[PageSpec] = &[
    PageSpec::new(
        "browser-use",
        "浏览器",
        "管理内置浏览器。可在计算机使用设置中设置浏览器扩展程序",
        PageKind::Standard,
        BROWSER_SECTIONS,
    ),
    PageSpec::new(
        "hooks-settings",
        "钩子",
        "通过配置和已启用的插件管理生命周期钩子。了解更多",
        PageKind::Standard,
        HOOKS_SECTIONS,
    ),
    PageSpec::new(
        "connections",
        "连接",
        "",
        PageKind::Standard,
        CONNECTION_SECTIONS,
    ),
    PageSpec::new("git-settings", "Git", "", PageKind::Standard, GIT_SECTIONS),
    PageSpec::new(
        "local-environments",
        "环境",
        "本地环境会告诉 ChatGPT 如何为项目设置工作树。了解更多。",
        PageKind::Standard,
        ENVIRONMENT_SECTIONS,
    ),
    PageSpec::new(
        "worktrees",
        "Worktrees",
        "",
        PageKind::Standard,
        WORKTREE_SECTIONS,
    ),
    PageSpec::new(
        "data-controls",
        "已归档的聊天",
        "",
        PageKind::Standard,
        DATA_CONTROL_SECTIONS,
    ),
];
