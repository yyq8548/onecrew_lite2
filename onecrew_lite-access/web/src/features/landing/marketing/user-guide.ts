import { PUBLIC_CONTACT } from "@/lib/public-contact";
import { SITE_URL } from "./site";

const SITE_HOST = new URL(SITE_URL).host;

// Adapted from the user-supplied OneCrew 0.5.21 guide (updated 2026-09-09).
export const GUIDE_SOURCE = 'https://isgub9b5x9na.sg.larksuite.com/wiki/KHUowZ6tOi44GZkikpCldCWvg3f';
export type GuideLocale = 'zh' | 'en';
type Text = [string, string];
export type GuideSection = { id: string; title: Text; intro?: Text; steps?: Text[]; note?: Text; links?: { id: string; title: Text }[]; promptIndexes?: number[]; stepPrompts?: Record<number, number[]>; notePrompts?: number[]; table?: { headers: Text[]; rows: Text[][] } };
export const guideSections: GuideSection[] = [
  {
    "id": "quick-start",
    "title": [
      "先完成一次搜索与保存",
      "Your first search and saved leads"
    ],
    "intro": [
      "跟着本页找到一批符合条件的 Creator，检查后保存到 OneCrew 分组。第一次只搜索和保存，不需要连接所有平台，也不发送邮件。",
      "Find relevant creators, review them, and save them to a OneCrew group. Start with search and saving: you do not need every platform connected, and no emails are sent."
    ],
    "note": [
      "适用版本：OneCrew 0.5.21。准备已获批的体验权限、OneCrew 支持的 AI，以及搜索渠道的账号或 API 凭据。不同 AI 的接入方式不同，接入后可使用本页的通用任务指令。数据保存在本机。",
      "For OneCrew 0.5.21. Prepare approved access, an AI assistant supported by OneCrew, and credentials for your search channel. Setup differs by assistant; once connected, use the common task prompts below. Data stays on your computer."
    ],
    "links": [
      {
        "id": "installation",
        "title": [
          "第一次使用：从安装开始",
          "New here? Start with installation"
        ]
      },
      {
        "id": "choose-channel",
        "title": [
          "已安装并连接 你使用的 AI：选择渠道",
          "Already set up? Choose a channel"
        ]
      }
    ]
  },
  {
    "id": "installation",
    "title": [
      "1. 获取权限并安装 OneCrew",
      "1. Get access and install OneCrew"
    ],
    "steps": [
      [
        `第一步：打开 ${SITE_HOST} → 立即体验 → 申请体验。等待审核；审核通过后复制专属安装提示词。`,
        `Step 1: Open ${SITE_HOST} → Try now → Apply for access. After approval, copy your personal installation prompt.`
      ],
      [
        "打开你使用的 AI，新建对话（以 Codex 为例，点击“新建任务”）。复制下方指令，把括号中的占位内容替换为你自己的安装提示词，再发送。请使用具备本地执行能力、且符合安装提示词兼容要求的工具。",
        "Open a new conversation in the AI tool you use—for example, start a new task in Codex. Copy the prompt below and replace the placeholder with your personal installation prompt before sending. Use a tool that can run local commands and meets the installation prompt’s compatibility requirements."
      ],
      [
        "按照你使用的 AI 给出的安装步骤继续。如操作系统弹出权限、密码或登录窗口，请本人按照提示确认。",
        "Follow the installation steps provided by the AI tool you use. Personally handle any operating-system permission, password, or login prompts."
      ],
      [
        "安装完成后，发送下方指令检查结果。",
        "After installation, send this prompt to check the result."
      ]
    ],
    "intro": [
      "操作位置：官网 → 能执行本地安装操作的 AI 工具。以下以 Codex 为安装示例，不代表只能使用 Codex。其他工具请按当前安装提示词中的兼容说明操作；不具备本地执行能力的聊天工具不能代为安装。",
      "Where: official website → an AI tool capable of local installation. Codex is the installation example here, not the only assistant you can use. Follow the current installation prompt’s compatibility instructions for other tools; chat-only tools cannot install local software."
    ],
    "note": [
      "完成标志：已检查安装版本和服务状态。若安装工具找不到 onecrew 命令，查看常见问题，不要覆盖已有数据。",
      "Done when the installed version and service status have been checked. If your installation tool cannot find the onecrew command, see troubleshooting; do not overwrite existing data."
    ],
    "stepPrompts": {
      "1": [
        14
      ],
      "3": [
        15
      ]
    }
  },
  {
    "id": "login",
    "title": [
      "2. 登录并检查运行状态",
      "2. Sign in and check status"
    ],
    "steps": [
      [
        "复制下方指令发送给 你使用的 AI，打开登录页面。",
        "Send this prompt to your AI assistant to open sign-in."
      ],
      [
        "本人完成浏览器登录和授权后，再发送下方指令。",
        "After completing browser sign-in and authorization yourself, send this prompt."
      ]
    ],
    "note": [
      "本指南对应版本 0.5.21。正常状态应为：服务运行中，数据状态 ready，登录状态 active。",
      "This guide covers version 0.5.21. Expected status: service running, data ready, and login active."
    ],
    "intro": [
      "操作位置：用于安装的 AI 工具 → 浏览器登录页 → AI 工具。以下登录指令需要工具具有本地执行能力；密码、验证码和授权由你本人完成。",
      "Where: your installation AI tool → browser sign-in → the AI tool. These login prompts require local execution capability. Handle passwords, codes, and authorization yourself."
    ],
    "stepPrompts": {
      "0": [
        16
      ],
      "1": [
        17
      ]
    }
  },
  {
    "id": "ai-setup",
    "title": [
      "3. 选择你使用的 AI",
      "3. Choose your AI assistant"
    ],
    "intro": [
      "安装 OneCrew 与连接 AI 是两件事。OneCrew 负责连接、保存和执行已授权渠道；通用 AI 或 Codex 负责理解目标、规划步骤并调用已接入能力。只完成与你使用的 AI 对应的接入方式，不需要把下面两种都做一遍。",
      "Installing OneCrew and connecting an AI assistant are separate tasks. OneCrew handles connections, storage, and execution within your approved channels; a general AI assistant or Codex interprets goals, plans steps, and calls connected capabilities. Follow only the setup path for the assistant you use."
    ],
    "links": [
      {
        "id": "plugin",
        "title": [
          "使用 Codex：配置插件",
          "Codex: set up the plugin"
        ]
      },
      {
        "id": "other-ai",
        "title": [
          "使用其他 AI：查看接入说明",
          "Other AI: connection guidance"
        ]
      }
    ],
    "note": [
      "完成标志：在选定 AI 的新对话中能调用 OneCrew 并读取状态。仅打开聊天窗口或完成 OneCrew 登录，不代表 AI 已接入。",
      "Done when a new conversation in your chosen assistant can call OneCrew and read its status. Opening a chat or signing in to OneCrew alone does not establish the AI connection."
    ]
  },
  {
    "id": "plugin",
    "title": [
      "3A. 安装插件（仅限 Codex）",
      "3A. Install the plugin (Codex only)"
    ],
    "steps": [
      [
        "在 Codex 中发送下方指令。",
        "Send this prompt in Codex."
      ],
      [
        "安装成功后，使用 Codex 的“退出 / Quit / Exit”功能完全退出应用，再重新打开。不要只关闭当前任务或窗口。",
        "After installation, fully exit Codex using its Quit or Exit action, then reopen it. Do not just close the current task or window."
      ],
      [
        "重新打开 Codex，新建任务，再发送下方指令。",
        "Reopen Codex, start a new task, and send this prompt."
      ]
    ],
    "note": [
      "只关闭窗口不等于退出。插件变更需要完全退出并重启 Codex 后生效。",
      "Closing the window does not quit the app. Fully quit and restart Codex for plugin changes to take effect."
    ],
    "intro": [
      "仅适用于选择 Codex 的用户。使用其他 AI 请跳到“其他 AI 接入”，无需安装 Codex 插件。",
      "For Codex users only. If using another assistant, skip to Other AI setup; you do not need the Codex plugin."
    ],
    "links": [
      {
        "id": "other-ai",
        "title": [
          "我使用其他 AI",
          "I use another AI assistant"
        ]
      },
      {
        "id": "open-interface",
        "title": [
          "Codex 已接入：打开 OneCrew",
          "Codex connected? Open OneCrew"
        ]
      }
    ],
    "stepPrompts": {
      "0": [
        18
      ],
      "2": [
        19
      ]
    }
  },
  {
    "id": "other-ai",
    "title": [
      "3B. 使用其他 AI：完成对应接入",
      "3B. Other AI: connect your assistant"
    ],
    "intro": [
      "OneCrew 支持其他 AI，但配置入口和接入方式会因工具及版本而异。不要安装 Codex 专用插件，也不要照搬 Codex 的重启操作。",
      "OneCrew supports other AI assistants, but setup varies by tool and version. Do not install the Codex-specific plugin or assume Codex restart instructions apply."
    ],
    "steps": [
      [
        "查看当前 OneCrew 版本提供的官方接入说明，确认你的 AI 工具及版本受支持。",
        "Check the official connection instructions for your OneCrew version and confirm that your AI tool and version are supported."
      ],
      [
        "在所选 AI 工具的对应配置入口完成连接与授权，具体字段按其接入说明填写；不要复制其他工具的配置。",
        "Configure the connection and authorization in your selected AI tool, using its specific instructions and fields—not another tool’s configuration."
      ],
      [
        "按该工具要求刷新连接或新建对话，发送下方状态检查指令。能实际读取 OneCrew 状态后再继续。",
        "Refresh the connection or start a new conversation as directed by that tool. Send the status-check prompt below and continue only after OneCrew status can actually be read."
      ]
    ],
    "promptIndexes": [
      0
    ],
    "note": [
      `若当前版本未提供你所用 AI 的接入说明，请联系 ${PUBLIC_CONTACT.email} 确认。不要猜测连接地址、令牌或配置参数；通用指令并不代表任意聊天工具都能直接执行。`,
      `If your version has no instructions for your AI tool, contact ${PUBLIC_CONTACT.email}. Do not guess connection URLs, tokens, or configuration values. Common task prompts do not mean every chat tool can execute them directly.`
    ],
    "links": [
      {
        "id": "open-interface",
        "title": [
          "AI 已接入：打开 OneCrew",
          "AI connected? Open OneCrew"
        ]
      }
    ]
  },
  {
    "id": "open-interface",
    "title": [
      "4. 让 你使用的 AI 打开 OneCrew",
      "4. Ask your AI assistant to open OneCrew"
    ],
    "intro": [
      "如果所选 AI 工具能执行本地操作或打开页面，可以发送下方指令。若它不具备这些能力，使用安装时提供的管理界面入口或已创建的系统快捷方式。",
      "If your AI tool can run local actions or open pages, use this prompt. Otherwise, use the management interface entry provided during installation or an existing system shortcut."
    ],
    "steps": [
      [
        "点击上方“复制指令”，粘贴到 你使用的 AI 的输入框并发送。",
        "Select Copy prompt above, paste it into your AI assistant, and send it."
      ],
      [
        "等待 你使用的 AI 打开 OneCrew。如果它返回一个页面链接，点击该链接；如果要求登录，由你本人完成。",
        "Wait for your AI assistant to open OneCrew. If it returns a page link, open that link. Complete sign-in yourself if requested."
      ],
      [
        "打开后，点击左侧“连接”，对照下图检查“本地服务”为运行中、“账号登录”为已登录。截图展示的是连接页，不一定是首次打开的默认页面。",
        "Once open, select Connect in the left sidebar. Compare with the screenshot: the local service should be running and the account signed in. This shows the Connect page, not necessarily the default opening page."
      ]
    ],
    "promptIndexes": [
      9
    ],
    "note": [
      "桌面快捷入口为可选项，是否可用取决于当前系统和安装版本。创建后，从系统的应用列表或快捷方式打开 OneCrew；没有快捷入口时，仍可让 你使用的 AI 打开管理界面。OneCrew 在本机运行，数据仍保存在你的电脑上。",
      "A desktop shortcut is optional and depends on your operating system and installed version. If available, open OneCrew from your system’s app list or shortcut. Otherwise, ask your AI assistant to open the management interface. OneCrew runs locally and data stays on your computer."
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "界面已打开：连接一个搜索渠道",
          "Workspace open? Connect a search channel"
        ]
      },
      {
        "id": "faq",
        "title": [
          "打不开或状态异常？查看排错",
          "Cannot open it or see a status error? Troubleshoot"
        ]
      }
    ],
    "notePrompts": [
      20
    ]
  },
  {
    "id": "choose-channel",
    "title": [
      "5. 只连接本次要用的渠道",
      "5. Connect one search channel"
    ],
    "intro": [
      "本教程用 YouTube 搜索作为示例，需要先准备 YouTube Data API Key。若你已有其他平台账号，也可以选择相应渠道并调整搜索指令。",
      "This walkthrough uses YouTube and requires a YouTube Data API key. You can use another supported platform instead and adapt the search prompt."
    ],
    "note": [
      "操作位置：OneCrew 的账号/连接。只连接你要搜索的平台，显示“可用/就绪”并通过读取测试后，返回这里开始搜索。邮件连接留到需要外联时再配置。",
      "Where: Accounts / Connections in OneCrew. Connect only your search platform. Once it is ready and passes a read test, return here to search. Set up email later when you need outreach."
    ],
    "links": [
      {
        "id": "youtube",
        "title": [
          "配置 YouTube（本教程示例）",
          "Set up YouTube for this walkthrough"
        ]
      },
      {
        "id": "instagram-x",
        "title": [
          "改用 Instagram",
          "Use Instagram"
        ]
      },
      {
        "id": "x",
        "title": [
          "改用 X",
          "Use X"
        ]
      },
      {
        "id": "telegram",
        "title": [
          "改用 Telegram",
          "Use Telegram"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "渠道已就绪：开始搜索",
          "Channel ready? Start searching"
        ]
      }
    ]
  },
  {
    "id": "workflow",
    "title": [
      "6. 在 你使用的 AI 中完成第一次搜索",
      "6. Run your first search in your AI assistant"
    ],
    "intro": [
      "操作位置：你使用的 AI 的新对话。先检查 OneCrew 状态，再复制下面的搜索指令。此时只查看结果，不保存、不发送。",
      "Where: a new conversation with your connected AI assistant. Check OneCrew status, then copy the search prompt below. Review results only at this stage; do not save or send."
    ],
    "note": [
      "完成标志：你使用的 AI 返回候选频道及可核对的链接、筛选理由。结果不足时先调整关键词，不让 AI 编造账号或联系方式。图中展示完整任务示例；首次使用请以本节“只搜索”指令为准。",
      "Done when your AI assistant returns candidate channels with verifiable links and selection reasons. Refine keywords if needed; do not ask AI to invent accounts or contacts. The screenshot illustrates a broader task; use this section’s search-only prompt for your first run."
    ],
    "promptIndexes": [
      0,
      13
    ]
  },
  {
    "id": "save-results",
    "title": [
      "7. 检查、保存并查看线索",
      "7. Review, save, and find your leads"
    ],
    "intro": [
      "操作位置：你使用的 AI → OneCrew 的线索页面。先核对候选人，再保存；收到 AI 的文字回复不等于已经验证保存成功。",
      "Where: your AI assistant → Leads in OneCrew. Review candidates before saving. An AI response alone does not verify that the records were saved."
    ],
    "steps": [
      [
        "打开候选频道链接，检查内容方向、活跃度和公开联系方式来源，确认要保留哪些账号。",
        "Open candidate channel links. Check content, activity, and public contact sources, then choose which accounts to keep."
      ],
      [
        "使用下方指令，把确认的候选人去重后保存到指定分组。",
        "Use the prompt below to deduplicate approved candidates and save them to a named group."
      ],
      [
        "回到 OneCrew，进入“线索”，查找刚才指定的分组；核对保存数量、频道链接及联系方式。",
        "Return to OneCrew → Leads and find the group you specified. Check the saved count, channel links, and contact details."
      ],
      [
        "若看不到记录，发送下方指令回读分组；如果读取失败，按常见问题诊断，不要初始化空数据。",
        "If records are missing, use this prompt to read back the group. If reads fail, follow troubleshooting; do not initialize empty data."
      ]
    ],
    "promptIndexes": [
      12
    ],
    "note": [
      "完成标志：你能在 OneCrew 中找到刚保存的分组与线索。至此已完成首次上手；需要外联时，再阅读邮件配置和草稿流程。",
      "Done when you can find the saved group and leads in OneCrew. Your first run is complete. Continue to email setup and drafting only when you need outreach."
    ],
    "links": [
      {
        "id": "email",
        "title": [
          "下一步：连接邮箱并准备外联",
          "Next: connect email for outreach"
        ]
      },
      {
        "id": "faq",
        "title": [
          "没有找到保存结果？",
          "Cannot find your saved results?"
        ]
      }
    ],
    "stepPrompts": {
      "3": [
        24
      ]
    }
  },
  {
    "id": "connections",
    "title": [
      "连接账号前准备什么",
      "Prepare your account connections"
    ],
    "intro": [
      "先连接对应渠道，才能在 你使用的 AI 中搜索并同步结果到 OneCrew，也才能后续使用该账号沟通。只需先准备本次任务会用到的平台。",
      "Connect the channel before searching through your AI assistant, syncing results into OneCrew, or using that account for outreach. Start with the platforms your current task needs."
    ],
    "table": {
      "headers": [
        [
          "平台",
          "Platform"
        ],
        [
          "连接方式与准备项",
          "Connection and prerequisites"
        ]
      ],
      "rows": [
        [
          [
            "邮件",
            "Email"
          ],
          [
            "Resend API Key；已验证的发信域名、可以接收回复的真实邮箱。",
            "Resend API key, a verified sending domain, and a real mailbox that receives replies."
          ]
        ],
        [
          [
            "YouTube",
            "YouTube"
          ],
          [
            "YouTube Data API Key；Google 账号与 Google Cloud 项目。",
            "YouTube Data API key, a Google account, and a Google Cloud project."
          ]
        ],
        [
          [
            "Instagram",
            "Instagram"
          ],
          [
            "可见浏览器登录；本人账号、密码、短信或邮箱验证码、2FA 和稳定网络 / 代理。",
            "Visible-browser sign-in; your account, password, SMS or email verification, 2FA, and a stable network / proxy."
          ]
        ],
        [
          [
            "X",
            "X"
          ],
          [
            "可见浏览器登录；账号、密码、邮箱或手机验证码、2FA；使用 X Chat 时准备其独立 passcode。",
            "Visible-browser sign-in; account, password, email or phone verification, and 2FA. Prepare the separate passcode if using X Chat."
          ]
        ],
        [
          [
            "Telegram",
            "Telegram"
          ],
          [
            "api_id、api_hash；已登录的手机 App 或仍在使用的手机号、登录验证码、可选两步验证密码和稳定网络 / 代理。",
            "api_id and api_hash, a signed-in mobile app or active phone number, login code, optional two-step verification password, and a stable network / proxy."
          ]
        ]
      ]
    },
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "youtube",
    "title": [
      "开启 YouTube 搜索",
      "Enable YouTube search"
    ],
    "intro": [
      "OneCrew 搜索 YouTube 频道和视频需要一个 API 密钥。Google Cloud 项目承载 YouTube Data API 的启用和配额；OneCrew 只使用你提供的 Key，通用 AI 或 Codex 通过 OneCrew 发起搜索。这不是绑定你的 YouTube 账号。",
      "OneCrew needs an API key to search YouTube channels and videos. Your Google Cloud project owns YouTube Data API enablement and quota; OneCrew uses the key you provide, while a general AI assistant or Codex searches through OneCrew. Create the key in Google Cloud, then add it to OneCrew. This does not link your YouTube account."
    ],
    "steps": [
      [
        "打开 console.cloud.google.com，使用你自己的 Google 账号登录。通过顶部项目选择器新建或选择一个你有权限管理的项目，例如 OneCrew-YouTube。个人使用不需要公司账号；团队使用时，按团队要求选择账号。",
        "Open console.cloud.google.com and sign in with your own Google account. Use the top project selector to create a project or select one you have permission to manage, for example OneCrew-YouTube. You do not need a company account for personal use; for team use, follow your team’s account requirements."
      ],
      [
        "在“API 和服务 → 库”启用 YouTube Data API v3。",
        "Enable YouTube Data API v3 under APIs & Services → Library."
      ],
      [
        "在“API 和服务 → 凭据 → 创建凭据”创建并复制 API 密钥。编辑密钥，将 API 限制为 YouTube Data API v3；应用限制需兼容 OneCrew 的运行方式。不确定如何设置时，请联系 OneCrew 支持；使用团队项目时，也可以咨询项目管理员。",
        "Create and copy a key under APIs & Services → Credentials → Create credentials. Edit it to allow only YouTube Data API v3. Application restrictions must match OneCrew’s runtime. If you are unsure how to configure them, contact OneCrew support; for a team project, you can also ask the project administrator."
      ],
      [
        "在 OneCrew“账号/连接 → YouTube → 新增 API 凭据”填写标签和 Key，保存并确认状态。",
        "In OneCrew, open Accounts / Connections → YouTube → Add API credentials. Enter a label and key, save, and check the status."
      ],
      [
        "让 AI 搜索一个明确的频道名称，验证连接能正常读取。",
        "Ask AI to search for a specific channel name to verify that the connection can read data."
      ]
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "instagram-x",
    "title": [
      "连接 Instagram",
      "Connect Instagram"
    ],
    "intro": [
      "由账号本人在弹出的可见浏览器中登录。账号权限不会自动获得。",
      "The account owner signs in through a visible browser window. Account access is never granted automatically."
    ],
    "steps": [
      [
        "准备账号密码、验证码和双重验证方式，并保持网络稳定。",
        "Have your password, verification method, and two-factor authentication ready, and keep the network stable."
      ],
      [
        "先固定稳定代理节点；从开始登录到完成连接，不要切换节点或关闭代理。在“账号/连接 → Instagram”点击新增 / 连接，按向导配置并检测网络，系统会打开可见浏览器。",
        "Choose a stable proxy endpoint and do not switch or disable it during connection. In Accounts / Connections → Instagram, select Add / Connect, configure and test networking in the wizard, and continue to the visible browser."
      ],
      [
        "在弹出的浏览器中本人完成登录、验证码及平台安全确认。",
        "Complete sign-in, verification codes, and platform security checks yourself in the browser window."
      ],
      [
        "浏览器登录成功后回到 OneCrew，等待完成连接并显示“可用/就绪”，再让 AI 读取自己的 Instagram 账号资料，确认读取正常。",
        "After browser sign-in, return to OneCrew and wait for Ready / Available. Ask AI to read your own Instagram profile to verify access."
      ]
    ],
    "note": [
      "遇到反复验证时，停止高频重试，不要频繁切换国家节点。先在普通浏览器确认账号未被锁定并完成平台验证，再用同一稳定网络重新连接。",
      "If verification repeats, stop rapid retries and frequent country changes. Check in a regular browser that the account is not locked, complete platform verification, then reconnect using the same stable network."
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "x",
    "title": [
      "连接 X",
      "Connect X"
    ],
    "steps": [
      [
        "确认 X 账号能正常登录，准备密码、邮箱 / 手机验证码和 2FA。",
        "Confirm that the account can sign in; prepare the password, email / phone verification, and 2FA."
      ],
      [
        "选择稳定代理出口，并保持到登录完成；不要中途切换为直连。",
        "Choose a stable proxy endpoint and keep it throughout sign-in; do not switch to a direct connection midway."
      ],
      [
        "在“账号/连接 → X”点击新增 / 连接，完成代理配置和检测，按弹出浏览器的提示登录。",
        "Select Add / Connect under Accounts / Connections → X, configure and test the proxy, then sign in through the browser window."
      ],
      [
        "邮箱确认、验证码、异常登录提示等安全验证必须由账号本人完成。",
        "The account owner must complete email confirmation, codes, unusual-login prompts, and other security checks."
      ],
      [
        "返回 OneCrew，等待“就绪/可用”，让 AI 查询自己的 X 账号或一条明确的公开帖子，验证连接。",
        "Return to OneCrew, wait for Ready / Available, and ask AI to read your own X profile or a specific public post."
      ]
    ],
    "note": [
      "使用 X Chat 时，OneCrew 可能另行要求 X Chat passcode。它不同于登录密码、2FA 验证码和代理密码，只能填入专用的 X Chat passcode 字段。",
      "If using X Chat, OneCrew may separately request its passcode. This differs from your login password, 2FA code, and proxy password; enter it only in the dedicated X Chat passcode field."
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "telegram",
    "title": [
      "连接 Telegram",
      "Connect Telegram"
    ],
    "intro": [
      "先获取 Telegram 应用凭据 api_id 和 api_hash，再通过扫码或手机号授权。",
      "First obtain your Telegram application credentials, api_id and api_hash, then authorize with a QR code or phone number."
    ],
    "steps": [
      [
        "先确保手机 Telegram 已注册并登录，手机号仍可用。在 my.telegram.org 输入手机号，登录码通常发送到 Telegram App。",
        "Ensure your mobile Telegram account is registered and signed in and your phone number is active. Enter it at my.telegram.org; the login code usually arrives in Telegram."
      ],
      [
        "登录 my.telegram.org 后进入 API development tools；已有应用可查看现有凭据，无需重复创建。",
        "After signing in at my.telegram.org, open API development tools. If you already have an application, view its credentials instead of creating another."
      ],
      [
        "在 API development tools 按实际用途填写应用资料。原文示例：App title 为 MyKOLApp；Short name 为 mykolapp99；URL 留空；Platform 选 Desktop 或 Other；Description 为 testing。示例仅帮助识别字段，不保证审核通过；描述应符合真实用途，必填项以页面校验为准。",
        "Complete the application form in API development tools for your actual use. Source examples: App title MyKOLApp, Short name mykolapp99, URL blank, Platform Desktop or Other, and Description testing. These illustrate fields, not guaranteed approval; describe your real use and follow form validation."
      ],
      [
        "原文建议 Short name 避免 create、agent、bot 等词；这不是已确认的平台禁用词规则，不应依靠改名绕过审核。创建后，将数字 api_id 和字符串 api_hash 分别填入 OneCrew 的对应字段，不要混填。",
        "The source suggests avoiding create, agent, or bot in Short name. This is not a verified platform prohibition and renaming should not be used to evade review. After creation, enter the numeric api_id and string api_hash in their respective OneCrew fields."
      ]
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "telegram-qr",
    "title": [
      "Telegram · 扫码登录",
      "Telegram · QR sign-in"
    ],
    "steps": [
      [
        "从生成二维码到完成授权，保持网络 / 代理稳定，不要切换节点。",
        "Keep your network / proxy stable from QR generation through authorization."
      ],
      [
        "进入“账号/连接 → Telegram → 新增”，填写 api_id 和 api_hash，选择二维码登录。",
        "Open Accounts / Connections → Telegram → Add, enter api_id and api_hash, and select QR login."
      ],
      [
        "推荐扫码：选择二维码登录，在手机 Telegram“设置 → 设备 → 连接桌面设备”中扫码并确认。不要用普通相机扫码。",
        "Recommended: choose QR login, then scan and approve from Telegram on your phone under Settings → Devices → Link Desktop Device. Do not use the regular camera app."
      ],
      [
        "核对手机上的登录请求并确认；若启用两步验证，在专用字段中输入密码。",
        "Review and approve the login request on your phone. If two-step verification is enabled, enter its password in the dedicated field."
      ],
      [
        "返回 OneCrew，等待“就绪/可用”。",
        "Return to OneCrew and wait for Ready / Available."
      ]
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "telegram-phone",
    "title": [
      "Telegram · 手机号登录",
      "Telegram · Phone sign-in"
    ],
    "steps": [
      [
        "在 Telegram 新增连接中填写 api_id、api_hash，选择手机号登录，输入正确国家区号和绑定号码。",
        "In a new Telegram connection, enter api_id and api_hash, select phone login, and enter the correct country code and linked number."
      ],
      [
        "验证码通常发到已登录的 Telegram App，也可能通过短信或其他方式发送，以平台提示为准。",
        "The code usually arrives in a signed-in Telegram app; SMS or another method may be offered by the platform."
      ],
      [
        "仅在当前 OneCrew 登录流程的验证码字段填写，不要发送给 AI 或聊天群。",
        "Enter it only in the code field of the current OneCrew sign-in flow, never in AI or group chats."
      ],
      [
        "如启用两步验证，在专用字段填写两步验证密码。",
        "If enabled, enter your two-step verification password in its dedicated field."
      ],
      [
        "返回 OneCrew 等待就绪，测试自己的连接状态，或读取一个明确的公开账号。",
        "Wait for Ready in OneCrew, then test your connection status or read a specific public account."
      ]
    ],
    "links": [
      {
        "id": "choose-channel",
        "title": [
          "返回：选择渠道",
          "Back to channel selection"
        ]
      },
      {
        "id": "workflow",
        "title": [
          "连接完成：开始第一次搜索",
          "Connected? Start your first search"
        ]
      }
    ]
  },
  {
    "id": "email",
    "title": [
      "连接邮件 · Resend",
      "Connect email · Resend"
    ],
    "intro": [
      "企业邮箱负责收信与回复，Resend API Key 用于程序发信。可填写的发件地址不一定是真实可收信的邮箱。",
      "Your business mailbox receives mail and replies. A Resend API key enables programmatic sending. A sender address is not necessarily a working inbox."
    ],
    "steps": [
      [
        "在 Resend 后台打开 API Keys，点击 Create API Key。名称可用 OneCrew-员工名-Production；优先选择仅发送权限，并限制到已验证的发信域名。",
        "Open API Keys in Resend and choose Create API Key. A label such as OneCrew-Employee-Production helps identify its purpose. Prefer sending-only permissions restricted to a verified domain."
      ],
      [
        "完整 Key 通常只在创建时显示一次，立即复制到 OneCrew 的安全输入框；不要放进 AI 对话、共享文档、邮件、聊天群或截图。",
        "The full key is normally displayed only once when created. Copy it immediately into OneCrew’s secure field, never into AI chats, shared documents, email, group chats, or screenshots."
      ],
      [
        "进入“账号/连接 → Email → 新增”。填写用途标签、真实发件邮箱、显示名称和 Resend API Key。",
        "Go to Accounts / Connections → Email → Add. Enter a descriptive label, real sender email, display name, and Resend API key."
      ],
      [
        "保存并验证 Key 和发信域名。显示“可用/就绪”后，给自己的测试邮箱发信并回复，再同步邮箱确认闭环。",
        "Save and verify the key and sending domain. Once Ready / Available, send to your own test inbox, reply, and sync the mailbox to check the complete loop."
      ]
    ],
    "note": [
      "完整 Key 丢失时，应在 Resend 撤销旧 Key 并新建，不要向同事索要密钥截图。",
      "If you lose the full key, revoke it in Resend and create a new one. Do not ask colleagues for screenshots of credentials."
    ],
    "links": [
      {
        "id": "prompts",
        "title": [
          "邮箱已就绪：模板、草稿与发送",
          "Email ready? Templates, drafts, and sending"
        ]
      }
    ]
  },
  {
    "id": "standard-workflow",
    "title": [
      "进阶：从线索到外联的完整流程",
      "Next: from leads to outreach"
    ],
    "steps": [
      [
        "明确目标客户画像。",
        "Define your ideal customer profile."
      ],
      [
        "创建活动或线索分组。",
        "Create a campaign or lead group."
      ],
      [
        "搜索候选 Creator。",
        "Search for potential creators."
      ],
      [
        "核对账号真实性、活跃度和联系方式来源。",
        "Verify account authenticity, activity, and contact sources."
      ],
      [
        "将合格对象保存到分组。",
        "Save qualified prospects to the group."
      ],
      [
        "创建邮件模板。",
        "Create an email template."
      ],
      [
        "生成个性化草稿。",
        "Generate personalized drafts."
      ],
      [
        "人工预览并确认。",
        "Review and approve manually."
      ],
      [
        "执行发送。",
        "Send approved messages."
      ],
      [
        "同步邮箱并跟进回复。",
        "Sync your inbox and follow up on replies."
      ]
    ],
    "note": [
      "每条指令写清四件事：平台、目标、数量、是否允许保存或发送。",
      "Specify four things in every instruction: platform, goal, quantity, and whether saving or sending is authorized."
    ],
    "links": [
      {
        "id": "prompts",
        "title": [
          "查看外联指令",
          "View outreach prompts"
        ]
      }
    ]
  },
  {
    "id": "overview",
    "title": [
      "参考：OneCrew 的功能范围",
      "Reference: what OneCrew can do"
    ],
    "steps": [
      [
        "在 YouTube、Instagram、X、Telegram 等平台发现 Creator/KOL。",
        "Discover creators on YouTube, Instagram, X, and Telegram."
      ],
      [
        "把候选人保存为线索并按项目分组。",
        "Save candidates as leads and group them by project."
      ],
      [
        "保存邮箱、社交账号、网站等联系方式。",
        "Store email addresses, social profiles, websites, and other contact information."
      ],
      [
        "创建营销活动和跟进任务。",
        "Create campaigns and follow-up tasks."
      ],
      [
        "制作邮件模板、草稿，并跟踪往来邮件。",
        "Create email templates and drafts, and track email conversations."
      ],
      [
        "通过已连接的账号执行获客、筛选和沟通流程。",
        "Use connected accounts for prospecting, screening, and communication."
      ]
    ]
  }
];
export const prompts: { title: Text; text: Text }[] = [
  {
    "title": [
      "检查数据",
      "Check data"
    ],
    "text": [
      "检查 OneCrew 是否正常，告诉我线索、分组、活动、邮件模板和账号连接状态。",
      "Check whether OneCrew is working and report the status of leads, groups, campaigns, email templates, and account connections."
    ]
  },
  {
    "title": [
      "寻找 Creator",
      "Find creators"
    ],
    "text": [
      "帮我在 YouTube 搜索 10 个持续发布中文 AI 教学内容的频道，整理频道名称、链接、粉丝数、内容方向和公开商务邮箱，保存到“中文 AI 教学频道”分组。先给我看结果，不要发送邮件。",
      "Find 10 YouTube channels regularly publishing Chinese AI tutorials. Collect names, links, subscriber counts, content topics, and public business emails. Save them to a “Chinese AI education channels” group. Show me the results first; do not send emails."
    ]
  },
  {
    "title": [
      "建立邮件模板",
      "Create an email template"
    ],
    "text": [
      "为中文 AI 教学频道创建一封 OneCrew 产品体验邀请邮件模板，语气友好、简洁，先给我预览，不要发送。",
      "Create a friendly, concise OneCrew trial invitation template for Chinese AI education channels. Show me a preview; do not send."
    ]
  },
  {
    "title": [
      "准备邮件草稿",
      "Prepare email drafts"
    ],
    "text": [
      "使用 XXX 邮箱，根据“OneCrew 产品体验邀请”模板，为“中文 AI 教学频道”分组中的联系人生成个性化草稿。先保存草稿，不要发送。",
      "Using the XXX mailbox and “OneCrew trial invitation” template, create personalized drafts for the “Chinese AI education channels” group. Save drafts only; do not send."
    ]
  },
  {
    "title": [
      "发送前检查",
      "Check before sending"
    ],
    "text": [
      "检查这些草稿的收件人、称呼、主题、变量和退订说明，列出有问题的邮件，不要发送。",
      "Check recipients, greetings, subjects, variables, and unsubscribe information in these drafts. List any problematic emails; do not send."
    ]
  },
  {
    "title": [
      "发送已确认的邮件",
      "Send approved emails"
    ],
    "text": [
      "把已经确认的 10 封草稿从 XXX 邮箱发送，并记录每封邮件的发送结果。",
      "Send the 10 approved drafts from the XXX mailbox and record the result of each email."
    ]
  },
  {
    "title": [
      "管理 · 检查版本",
      "Manage · Check version"
    ],
    "text": [
      "请检查当前安装的 OneCrew 版本，并告诉我是否为最新安装版本。",
      "Check the installed OneCrew version and tell me whether it is the latest installation version."
    ]
  },
  {
    "title": [
      "管理 · 检查服务、数据和登录",
      "Manage · Check service, data, and login"
    ],
    "text": [
      "请检查 OneCrew 的服务状态、数据状态和登录状态，只读取，不要修改任何数据。",
      "Check OneCrew service, data, and login status. Read only; do not modify any data."
    ]
  },
  {
    "title": [
      "管理 · 启动登录",
      "Manage · Start sign-in"
    ],
    "text": [
      "请启动 OneCrew 登录流程并打开登录页面。需要输入密码、验证码或确认授权时让我接管。",
      "Start OneCrew sign-in and open the login page. Hand control to me for passwords, verification codes, or authorization."
    ]
  },
  {
    "title": [
      "管理 · 打开管理界面",
      "Manage · Open interface"
    ],
    "text": [
      "请启动 OneCrew 并打开管理界面。",
      "Start OneCrew and open its management interface."
    ]
  },
  {
    "title": [
      "仅限 Codex · 管理 · 安装或更新插件",
      "Codex only · Manage · Install or update plugin"
    ],
    "text": [
      "请安装或更新 OneCrew 的 Codex 插件，完成后检查插件版本和启用状态。",
      "Install or update the OneCrew Codex plugin, then check its version and enabled status."
    ]
  },
  {
    "title": [
      "仅限 Codex · 检查插件状态",
      "Codex only · Check plugin status"
    ],
    "text": [
      "请刷新并检查 OneCrew 的 Codex 插件状态，告诉我是否已经安装、启用，以及是否需要重启 Codex。",
      "Refresh and check the OneCrew Codex plugin status. Tell me whether it is installed and enabled, and whether Codex needs restarting."
    ]
  },
  {
    "title": [
      "保存已确认的候选人",
      "Save approved candidates"
    ],
    "text": [
      "请将我刚确认的候选频道去重后保存到“中文 AI 教学频道”分组，保留频道名称、链接、粉丝数、内容方向和公开商务邮箱来源。不要猜测缺失信息，不要发送邮件。完成后回读分组并告诉我实际保存数量。",
      "Deduplicate the channels I approved and save them to “Chinese AI education channels”. Keep names, links, subscriber counts, topics, and sources for public business emails. Do not guess missing information or send emails. Read back the group and report how many records were saved."
    ]
  },
  {
    "title": [
      "第一次搜索：先看结果",
      "First search: review results only"
    ],
    "text": [
      "帮我在 YouTube 搜索 10 个持续发布中文 AI 教学内容的频道，列出频道名称、链接、粉丝数、内容方向、筛选理由和公开商务邮箱来源；缺失信息标为未找到。先给我看结果，不要保存，也不要发送邮件。",
      "Find 10 YouTube channels regularly publishing Chinese AI tutorials. List names, links, subscriber counts, topics, selection reasons, and sources for public business emails. Mark missing information as not found. Show results only; do not save or send emails."
    ]
  },
  {
    "title": [
      "安装 OneCrew",
      "Install OneCrew"
    ],
    "text": [
      "请先检查当前操作系统及架构是否受这个安装链接支持（此处放安装提示词）。支持时帮我安装 OneCrew；不支持时停止并说明，不要安装其他系统的版本。安装完成后检查版本和服务状态；如果出现登录页面，请打开并告诉我下一步怎么做。不要删除或覆盖已有数据。",
      "Check whether this installation link supports my operating system and architecture (paste the installation prompt here). Install OneCrew only if supported; otherwise stop and explain. Do not install a build for a different operating system. Check its version and service status afterward. If a login page appears, open it and tell me what to do next. Do not delete or overwrite existing data."
    ]
  },
  {
    "title": [
      "检查安装结果",
      "Check installation"
    ],
    "text": [
      "请检查 OneCrew 的版本、服务状态和数据状态，确认是否安装成功。",
      "Check OneCrew’s version, service status, and data status to confirm installation succeeded."
    ]
  },
  {
    "title": [
      "启动登录",
      "Start sign-in"
    ],
    "text": [
      "请帮我启动 OneCrew 登录流程并打开登录页面。我会完成账号密码、验证码或授权操作；完成后请检查登录状态。",
      "Start OneCrew sign-in and open the login page. I will handle the password, verification code, and authorization. Check login status afterward."
    ]
  },
  {
    "title": [
      "确认登录状态",
      "Verify sign-in"
    ],
    "text": [
      "我已经完成登录，请检查 OneCrew 的服务、数据和登录状态。",
      "I have signed in. Check OneCrew’s service, data, and login status."
    ]
  },
  {
    "title": [
      "仅限 Codex · 安装或更新插件",
      "Codex only · Install or update plugin"
    ],
    "text": [
      "请安装或更新 OneCrew 的 Codex 插件，并确认插件版本和启用状态。不要修改 OneCrew 的业务数据。",
      "Install or update the OneCrew Codex plugin and confirm its version and enabled status. Do not modify OneCrew business data."
    ]
  },
  {
    "title": [
      "检查插件连接",
      "Check plugin connection"
    ],
    "text": [
      "检查 OneCrew 状态和账号连接。",
      "Check OneCrew status and account connections."
    ]
  },
  {
    "title": [
      "可选：安装桌面快捷入口",
      "Optional: install desktop launcher"
    ],
    "text": [
      "请先检查当前操作系统和 OneCrew 版本是否支持桌面快捷入口。支持时帮我创建并说明从哪里打开；不支持时告诉我如何通过 你使用的 AI 打开管理界面。不要修改或删除现有业务数据。",
      "Check whether my operating system and OneCrew version support a desktop shortcut. If supported, create it and explain where to open it. Otherwise, explain how to open the management interface through your AI assistant. Do not modify or delete existing business data."
    ]
  },
  {
    "title": [
      "修复找不到命令",
      "Fix a missing command"
    ],
    "text": [
      "你使用的 AI 找不到 OneCrew 命令。请检查标准安装位置和 PATH；如果确实没有安装，请使用我提供的官方链接重新安装。不要覆盖已有数据目录。",
      "The current AI tool cannot find OneCrew. Check the standard installation location and PATH. If it is not installed, reinstall using my official link. Do not overwrite existing data directories."
    ]
  },
  {
    "title": [
      "仅限 Codex · 检查插件不可用",
      "Codex only · Check unavailable tools"
    ],
    "text": [
      "Codex 看不到 OneCrew 工具。请检查插件安装和启用状态，修复插件后告诉我是否需要重启 Codex。不要启动独立的重复 MCP 服务。",
      "Check the OneCrew plugin installation and enabled status. Repair the plugin and tell me whether to restart Codex. Do not start a separate duplicate MCP service."
    ]
  },
  {
    "title": [
      "只诊断，不重置数据",
      "Diagnose without resetting data"
    ],
    "text": [
      "请检查 OneCrew 服务、数据目录和插件运行状态，只诊断，不要初始化空数据，不要删除或覆盖已有数据。",
      "Check the OneCrew service, data directory, and plugin status. Diagnose only; do not initialize empty data, delete, or overwrite existing data."
    ]
  },
  {
    "title": [
      "查找已保存的线索",
      "Find saved leads"
    ],
    "text": [
      "请回读“中文 AI 教学频道”分组及刚才的保存结果，列出实际保存数量和频道链接。只读取，不要初始化空数据或修改现有记录。",
      "Read back the “Chinese AI education channels” group and the previous save result. List the actual saved count and channel links. Read only; do not initialize empty data or modify records."
    ]
  }
];
export const guideFaq: { q: Text; a: Text; promptIndexes?: number[] }[] = [
  {
    "q": [
      "安装工具提示“找不到 onecrew 命令”",
      "The installation tool cannot find onecrew"
    ],
    "a": [
      "将自己的专属安装链接和下面的指令一起发给 你使用的 AI。",
      "Send your personal installation link to your AI assistant with this prompt."
    ],
    "promptIndexes": [
      21
    ]
  },
  {
    "q": [
      "看不到 OneCrew 工具（仅限 Codex）",
      "OneCrew tools are missing (Codex only)"
    ],
    "a": [
      "将下方指令发给 Codex。如需重启，使用应用的“退出 / Quit / Exit”功能完全退出，再重新打开并新建任务；不要只关闭窗口。",
      "Send this prompt to Codex. If a restart is required, use the app’s Quit or Exit action, reopen it, and start a new task. Do not just close the window."
    ],
    "promptIndexes": [
      22
    ]
  },
  {
    "q": [
      "页面能打开，但读取线索或邮件失败",
      "The interface opens but leads or emails fail to load"
    ],
    "a": [
      "不要初始化新数据或删除数据目录。先用下方指令诊断；保存错误截图并联系 OneCrew 支持，修复前不要反复卸载或覆盖数据。",
      "Do not initialize new data or delete data directories. Diagnose using this prompt, save an error screenshot, and contact OneCrew support. Do not repeatedly uninstall or overwrite data."
    ],
    "promptIndexes": [
      23
    ]
  },
  {
    "q": [
      "Instagram 或 X 显示需要重新登录",
      "Instagram or X needs sign-in again"
    ],
    "a": [
      "这是平台登录会话过期，不代表线索数据丢失。进入“连接”页面重新授权对应账号。",
      "The platform session has expired; this does not mean your leads were lost. Reauthorize the account on the Connections page."
    ]
  },
  {
    "q": [
      "YouTube 搜索失败",
      "YouTube search fails"
    ],
    "a": [
      "检查 API 凭据是否可用、项目额度是否耗尽，以及是否选中了正确的 YouTube 连接。",
      "Check API credentials, remaining project quota, and whether the correct YouTube connection is selected."
    ]
  },
  {
    "q": [
      "邮件可以看到，但不能发送",
      "Emails are visible but cannot be sent"
    ],
    "a": [
      "检查发件邮箱是否可用、模板变量是否完整，以及当前操作是否仍在等待发送确认。",
      "Check the sender mailbox, template variables, and whether the action is still awaiting send confirmation."
    ]
  }
];
export const guideLinks = [
  { title: ['Resend 发件地址与邮箱说明', 'Resend sender addresses and inboxes'] as Text, href: 'https://resend.com/docs/knowledge-base/how-do-I-create-an-email-address-or-sender-in-resend' },
  { title: ['Google API 密钥创建', 'Create a Google API key'] as Text, href: 'https://support.google.com/googleapi/answer/6158862' },
  { title: ['Resend 域名验证', 'Resend domain verification'] as Text, href: 'https://resend.com/docs/dashboard/domains/introduction' },
  { title: ['Google Cloud 控制台', 'Google Cloud Console'] as Text, href: 'https://console.cloud.google.com/' },
  { title: ['YouTube Data API 入门', 'YouTube Data API getting started'] as Text, href: 'https://developers.google.com/youtube/v3/getting-started' },
  { title: ['Telegram 应用凭据', 'Telegram application credentials'] as Text, href: 'https://my.telegram.org/' },
  { title: ['仅限 macOS：安全打开 Mac App', 'macOS only: safely open Mac apps'] as Text, href: 'https://support.apple.com/102445' },
];
