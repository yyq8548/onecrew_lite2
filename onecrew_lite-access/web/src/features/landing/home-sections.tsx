import { LandingLink as Link } from './landing-link';
import { useAccessLoginUrl } from './access-site';
import type { ReactNode } from 'react';
import { CreatorAvatar, type CreatorId } from './creator-avatar';
import { FeatureIntroConversation } from './feature-intro-conversation';
import styles from './home-sections.module.css';

type Locale = 'zh' | 'en';

const translations = {
  comparison: {
    zh: {
      title: ['传统外联靠人拼接', 'OneCrew 由 AI 推进'],
      subtitle: '同样是找人、沟通和跟进，差别在于 OneCrew 能让你的 AI 带着完整上下文工作。',
      old: ['传统达人外联', '名单、对话和跟进各自为战', '名单放在表格，邮件和私信散在不同平台，回复靠人识别，下一步靠人记。', ['反复复制达人资料与聊天记录', '回复、待办和负责人容易脱节', 'AI 只能给建议，无法进入执行流程']],
      next: ['用 OneCrew 外联', '同一份上下文，AI 接着往下做', 'OneCrew 把达人资料、渠道和历史沟通放进同一工作流，让 AI 准备下一步，你只需审核关键动作。', ['达人资料、沟通与进度持续关联', 'AI 识别意向并准备回复与跟进', '搜索、草稿、提醒和协作连续推进']],
    },
    en: {
      title: ['Traditional outreach breaks the workflow', 'OneCrew keeps it moving'],
      subtitle: 'More names won’t fix scattered conversations, follow-ups, and relationship history. OneCrew gives your AI the context to act on what comes next.',
      old: ['Traditional creator outreach', 'Every step lives in a different tool', 'Lists sit in spreadsheets while email and DMs live in separate inboxes. People have to piece together the context and remember the next move.', ['Copy creator data and conversations by hand', 'Miss replies, follow-ups, and ownership changes', 'Ask AI for advice without giving it the full workflow']],
      next: ['AI-native outreach with OneCrew', 'One context. A clear next move.', 'OneCrew keeps creator records, channels, and conversation history connected so your AI can prepare the next step while you control key actions.', ['Keep creator data, messages, and progress connected', 'Recognize intent and prepare replies and follow-ups', 'Move from search to outreach without rebuilding context']],
    },
  },
  steps: {
    zh: {
      title: ['3步', '开始达人合作'],
      subtitle: '连接一次，之后从一句目标开始，由 AI 协助推进。',
      action: '立即体验',
      cards: [
        ['Step1', '连接工作工具', '下载并登录 OneCrew 客户端，连接已有 AI、邮箱与支持的社交渠道。', '只需首次设置'],
        ['Step 2', '自然语言说出目标', '用一句话发起达人搜索、跟踪或外联任务，AI 根据上下文开始准备。', '自然语言发起'],
        ['Step 3', '确认并继续推进', '在工作台查看结果、调整条件、审核消息，并跟进合作状态。', '关键动作可控'],
      ],
    },
    en: {
      title: ['3 steps', 'to start a creator partnership'],
      subtitle: 'Connect once, then start with one clear goal and let AI help move the work forward.',
      action: 'Try OneCrew',
      cards: [
        ['Step 1', 'Connect your work tools', 'Download and sign in to OneCrew, then connect your existing AI service, inbox, and supported social channels.', 'Set up once'],
        ['Step 2', 'Describe your goal in plain language', 'Start a creator search, tracking, or outreach task with one sentence. AI prepares the work using the available context.', 'Just ask'],
        ['Step 3', 'Review and keep moving', 'Review results, refine criteria, approve messages, and track partnership status in the workspace.', 'You stay in control'],
      ],
    },
  },
  channels: {
    zh: {
      title: ['OneCrew', '支持哪些渠道'],
      subtitle: '覆盖常用的达人外联渠道，把不同渠道的沟通放进同一条合作记录，减少来回切换。\n密钥与登录信息保存在本机加密凭据库中，不在界面中回显。',
      action: '立即体验',
      cards: [
        ['X', '隔离浏览器登录', '连接品牌 X 账号，支持 X Chat。'],
        ['Instagram', '隔离浏览器登录', '连接品牌 Instagram 账号，管理达人沟通。'],
        ['Telegram', '二维码或手机号登录', '接入 Telegram 对话，需配置 SOCKS5 与 API 凭据。'],
        ['YouTube', 'YouTube Data API', '搜索 Creator 频道并分析公开数据。'],
        ['邮件', '品牌邮件身份', '统一发送、接收并管理达人外联邮件。'],
        ['TikTok', 'TikTok 达人搜索', '搜索、筛选并分析 TikTok 达人信息'],
      ],
    },
    en: {
      title: ['Bring your outreach channels', 'into your AI'],
      subtitle: 'Connect supported social accounts, creator data, and brand email to keep discovery, outreach, and follow-ups in one workflow.\nX and Instagram use isolated browser sessions. Credentials and API keys are encrypted locally and never exposed in the interface.',
      action: 'Try OneCrew',
      cards: [
        ['X', 'Isolated browser sign-in', 'Connect your brand account and work with X Chat.'],
        ['Instagram', 'Isolated browser sign-in', 'Connect your brand account for creator outreach.'],
        ['Telegram', 'QR code or phone sign-in', 'Bring Telegram conversations into the same workflow. SOCKS5 and API credentials required.'],
        ['YouTube', 'YouTube Data API', 'Search channels and analyze public creator data.'],
        ['Email', 'Brand email', 'Send, receive, and keep outreach threads in sync.'],
        ['TikTok', 'TikTok creator discovery', 'Search and evaluate TikTok creators.'],
      ],
    },
  },
} as const;

const channelIcons = ['/images/figma-channel-x.svg', '/images/figma-channel-instagram.svg', '/images/figma-channel-telegram.svg', '/images/figma-channel-youtube.svg', '/images/figma-channel-email.svg', '/images/figma-channel-tiktok.png'];
const stepIcons = ['/images/workflow-step-connect.svg', '/images/workflow-step-goal.svg', '/images/workflow-step-approve.svg'];

function RequestButton({ label }: { label: string }) {
  const ACCESS_LOGIN_URL = useAccessLoginUrl();
  return <a className={styles.ctaButton} href={ACCESS_LOGIN_URL}>{label}<span aria-hidden="true">→</span></a>;
}

export function OutreachComparison({ locale = 'zh' }: { locale?: Locale }) {
  const content = translations.comparison[locale];
  return <section id="comparison" className={`${styles.section} ${styles.comparison}`}>
    <header className={styles.heading}><h2><span>{content.title[0]}</span><strong>{content.title[1]}</strong></h2><p>{content.subtitle}</p></header>
    <div className={`${styles.inner} ${styles.comparisonStage}`}>
      {[content.old, content.next].map(([badge, title, description, points]) => <article className={styles.comparisonCard} key={badge as string}><div><span className={styles.comparisonBadge}>{badge}</span><h3>{title}</h3><p>{description}</p><ul>{points.map(point => <li key={point}>{point}</li>)}</ul></div></article>)}
      <span className={styles.upgradeBadge} aria-hidden="true"><img src="/images/figma-upgrade-arrow.svg" alt="" /></span>
    </div>
  </section>;
}

export function WorkflowSteps({ locale = 'zh' }: { locale?: Locale }) {
  const content = translations.steps[locale];
  const en = locale === 'en';
  return <section id="workflow" className={`${styles.section} ${styles.steps}`}>
    <div className={`${styles.inner} ${styles.stepsLayout}`}>
      <div className={styles.stepsCopy}>
        <div className={styles.stepsIntro}><h2><span>{content.title[0]}</span><strong>{content.title[1]}</strong></h2><p>{content.subtitle}</p><RequestButton label={content.action} /></div>
        <div className={styles.stepList}>{content.cards.map(([label, title, description, note], index) => <article className={styles.stepCard} key={label}><img src={stepIcons[index]} alt="" aria-hidden="true" /><div><span>{label}</span><h3>{title}</h3><p>{description}</p><small>{note}</small></div></article>)}</div>
      </div>
      <figure className={styles.creatorVisual}>
        <img src="/images/creator-cutout-ava-figma.png" alt={en ? 'Beauty creator Ava Morgan greeting the viewer' : '美妆达人 Ava Morgan 挥手'} />
        <div className={`${styles.creatorCard} ${styles.creatorProfile}`}><span><span className={styles.creatorInstagramIcon}><img src="/images/figma-channel-instagram.svg" alt="" width="24" height="24" /></span>Instagram</span><strong>@avamorgan</strong></div>
        <div className={`${styles.creatorCard} ${styles.creatorAudience}`}><strong>{en ? 'North American audience' : '北美受众'}</strong><small>{en ? 'High engagement' : '高互动'}</small></div>
      </figure>
      <div className={`${styles.creatorCard} ${styles.creatorFit}`}><span>{en ? 'CONTENT FIT' : '内容匹配'}</span><strong>{en ? 'Skincare tutorials' : '护肤教程'}</strong><small><img src="/images/creator-email.svg" alt="" />ava.morgan@example.com</small></div>
    </div>
  </section>;
}

export function ChannelCoverage({ locale = 'zh' }: { locale?: Locale }) {
  const content = translations.channels[locale];
  return <section id="channels" className={`${styles.section} ${styles.channels}`}>
    <header className={styles.heading}><div><h2><span>{content.title[0]}</span><strong>{content.title[1]}</strong></h2><p>{content.subtitle.split('\n').map((line, index) => <span key={line}>{index > 0 && <br />}{line}</span>)}</p></div><RequestButton label={content.action} /></header>
    <div className={`${styles.inner} ${styles.channelGrid}`}>{content.cards.map(([name, access, description], index) => <article className={styles.channelCard} key={name}><div><span className={styles.channelIcon}><img src={channelIcons[index]} alt="" width="48" height="48" /></span><h3>{name}</h3></div><strong>{access}</strong><p>{description}</p></article>)}</div>
  </section>;
}

function DemoChrome({ title, subtitle, children, showStatus = false, activeRail = 'creators' }: { title: string; subtitle: string; children: ReactNode; showStatus?: boolean; activeRail?: 'creators' | 'tasks' | 'agent' | 'mail' }) {
  return <div className="envoy-demo-shell"><aside className="envoy-demo-rail" aria-hidden="true"><img src="/images/figma-onecrew-mark.svg" alt="" /><span className={activeRail === 'creators' ? 'active' : ''}><img src="/images/figma-rail-creators.svg" alt="" /></span><span className={activeRail === 'tasks' || activeRail === 'agent' ? 'active' : ''}><img src={activeRail === 'agent' ? '/images/figma-rail-agent.svg' : '/images/figma-rail-tasks.svg'} alt="" /></span><span className={activeRail === 'mail' ? 'active' : ''}><img src="/images/figma-rail-mail.svg" alt="" /></span></aside><div className="envoy-demo-main"><header className="envoy-demo-title"><div><strong>{title}</strong><small>{subtitle}</small></div>{showStatus && <span><i />MCP healthy</span>}</header>{children}</div></div>;
}

function DemoToolbar({ en }: { en: boolean }) {
  return <div className="envoy-demo-toolbar"><label><img src="/images/figma-search.svg" alt="" /><span>{en ? 'Search creators or contact channels…' : '搜索 Creator 或任意建联通道…'}</span></label><button type="button">{en ? 'Reset' : '重置'}</button><button className="primary" type="button">＋ {en ? 'Add Creator' : '新增 Creator'}</button></div>;
}

function SearchDemo({ en }: { en: boolean }) {
  const rows = [['ava', 'Ava Morgan', '@avamorgan', 'Instagram', 'Email'], ['sofia', 'Sofia Lee', '@sofiaskin', 'Instagram', 'Website'], ['nora', 'Nora Reed', '@noranotes', 'YouTube', 'Email']] as Array<[CreatorId, string, string, string, string]>;
  return <div className={`${styles.featureDemo} feature-demo search-demo`}><DemoChrome title={en ? 'Leads' : '线索'} subtitle={en ? 'Creator profiles and contact channels' : 'Creator 画像与建联通道'}><DemoToolbar en={en} /><div className="envoy-filter-row"><span>{en ? 'United States' : '美国'}</span><span>{en ? 'Beauty' : '美妆'}</span><span>{en ? 'Engagement > 4%' : '互动率 > 4%'}</span></div><div className="envoy-table"><div className="envoy-table-head"><b>Creator</b><b>{en ? 'Initial source' : '初次收录'}</b><b>{en ? 'Contact' : '建联方式'}</b><b /></div>{rows.map(row => <div className="envoy-table-row" key={row[1]}><div><CreatorAvatar person={row[0]} channel={row[3] === 'YouTube' ? 'youtube' : 'instagram'} /><span><strong>{row[1]}</strong><small>{row[2]}</small></span></div><span className="channel-chip">◎ {row[2]}</span><span className="channel-chip">{row[3]} · {row[4]}</span><b>•••</b></div>)}</div></DemoChrome></div>;
}

function DnaMatchDemo({ en }: { en: boolean }) {
  return <div className={`${styles.featureDemo} feature-demo`}><DemoChrome title={en ? 'Leads' : '线索'} subtitle={en ? 'Creator profiles and contact channels' : 'Creator 画像与建联通道'}><DemoToolbar en={en} /><div className="envoy-detail-grid"><div className="envoy-profile-card"><span>{en ? 'Brand brief' : '品牌简报'}</span><strong>Clean beauty · US</strong><small>{en ? 'Natural ingredients · Understated voice · Everyday skincare' : '天然成分 · 克制表达 · 日常护肤'}</small><div className="envoy-tag-row"><i>{en ? 'Beauty' : '美妆'}</i><i>{en ? 'US' : '美国'}</i><i>{en ? 'Everyday care' : '日常护理'}</i></div></div><div className="envoy-match-card"><div><CreatorAvatar person="ava" large /><span><strong>Ava Morgan</strong><small>@avamorgan</small></span><b>94%</b></div><dl><div><dt>{en ? 'Content fit' : '内容匹配'}</dt><dd>{en ? 'Skincare tutorials' : '护肤教程'}</dd></div><div><dt>{en ? 'Audience' : '受众市场'}</dt><dd>{en ? 'United States' : '美国'}</dd></div><div><dt>{en ? 'Contact' : '建联方式'}</dt><dd>Email · Instagram</dd></div></dl></div></div></DemoChrome></div>;
}

function EmailDemo({ en }: { en: boolean }) {
  return <div className={`${styles.featureDemo} feature-demo`}><DemoChrome title={en ? 'Conversations' : '对话'} subtitle={en ? 'Email and supported social messages' : '邮件与支持的社交私信'} activeRail="mail"><div className="envoy-inbox"><aside><label>{en ? 'Search conversations…' : '搜索对话…'}</label><div className="selected"><CreatorAvatar person="ava" /><span><strong>Ava Morgan</strong><small>{en ? 'Interested in the brief' : '希望先看合作简报'}</small></span></div><div><CreatorAvatar person="sofia" /><span><strong>Sofia Lee</strong><small>{en ? 'Follow-up due tomorrow' : '明天需要跟进'}</small></span></div></aside><section><header><div><strong>Ava Morgan</strong><small>Email · Instagram</small></div><span>{en ? 'Active' : '推进中'}</span></header><div className="envoy-message">{en ? 'I’m interested. Could you send over the partnership brief?' : '很感兴趣，可以先看一下合作 brief 吗？'}</div><div className="envoy-draft"><span>{en ? 'Agent draft · review required' : 'Agent 草稿 · 需要审核'}</span><p>{en ? 'Hi Ava, absolutely — I’ve attached the spring collaboration brief…' : 'Hi Ava，当然可以，春季合作简报已经附上…'}</p><button type="button">{en ? 'Review and send' : '审核并发送'}</button></div></section></div></DemoChrome></div>;
}

function RelationshipDemo({ en }: { en: boolean }) {
  return <div className={`${styles.featureDemo} feature-demo`}><DemoChrome title={en ? 'Tasks' : '任务'} subtitle={en ? 'Agent actions and partnership progress' : 'Agent 行动与合作进度'} activeRail="tasks"><div className="envoy-task-toolbar"><strong>{en ? 'Creator follow-ups' : 'Creator 跟进任务'}</strong><button type="button">＋ {en ? 'New task' : '新增任务'}</button></div><div className="envoy-task-list"><article><span className="task-state done">✓</span><div><strong>{en ? 'Ava Morgan replied' : 'Ava Morgan 已回复'}</strong><small>{en ? 'Intent detected: interested in collaboration' : '识别意向：愿意合作'}</small></div><b>{en ? 'Done' : '已完成'}</b></article><article className="current"><span className="task-state">2</span><div><strong>{en ? 'Review the prepared response' : '审核已准备的回复'}</strong><small>{en ? 'Human approval is required before sending' : '发送前需要人工确认'}</small></div><b>{en ? 'Needs review' : '待审核'}</b></article><article><span className="task-state">3</span><div><strong>{en ? 'Follow up in 3 business days' : '3 个工作日后跟进'}</strong><small>{en ? 'The Agent will remind the owner' : 'Agent 将提醒负责人'}</small></div><b>{en ? 'Scheduled' : '已定时'}</b></article></div></DemoChrome></div>;
}

function FeatureIntroDemo({ en }: { en: boolean }) {
  const creators = [['ava', 'Ava Morgan', '@avamorgan', 'Instagram', 'Email'], ['sofia', 'Sofia Lee', '@sofiaskin', 'Instagram', 'Website'], ['nora', 'Nora Reed', '@noranotes', 'YouTube', 'Email']] as Array<[CreatorId, string, string, string, string]>;
  return <div className={styles.featureIntro}><h3>{en ? 'Work in chat. Keep every detail in one workspace.' : '用对话推进工作，用面板掌控细节。'}</h3><div className="codex-browser-demo"><div className="codex-browser-bar"><span><i /><i /><i /></span><b>OneCrew · AI</b><small>{en ? 'Built-in browser' : '内置浏览器'}</small></div><FeatureIntroConversation en={en} /><section className="codex-product-pane"><DemoChrome title={en ? 'Leads' : '线索'} subtitle={en ? 'Creator profiles and contact channels' : 'Creator 画像与建联通道'} showStatus activeRail="agent"><DemoToolbar en={en} /><div className="envoy-filter-row"><span>{en ? 'United States' : '美国'}</span><span>{en ? 'Beauty' : '美妆'}</span><span>{en ? 'Engagement > 4%' : '互动率 > 4%'}</span></div><div className="envoy-table"><div className="envoy-table-head"><b>Creator</b><b>{en ? 'Source' : '初次收录'}</b><b>{en ? 'Contact' : '建联方式'}</b><b /></div>{creators.map(row => <div className="envoy-table-row" key={row[1]}><div><CreatorAvatar person={row[0]} channel={row[3] === 'YouTube' ? 'youtube' : 'instagram'} /><span><strong>{row[1]}</strong><small>{row[2]}</small></span></div><span className="channel-chip">◎ {row[2]}</span><span className="channel-chip">{row[3]} · {row[4]}</span><b>•••</b></div>)}</div></DemoChrome></section></div></div>;
}

const featureCopy = {
  zh: [
    ['发现与筛选', '一句话，\n找到值得联系的达人', '告诉 AI 目标市场、品类、受众和内容偏好。OneCrew 通过 AI 评分、粉丝数据、合作阶段和联系渠道，快速判断下一步该联系谁', ['按平台、地区、关键词发现 Creator', '支持评分、评估、粉丝数、AI 自动评价等 Creator 属性', '记录数据来源、观察结果和身份关联，减少同一达人被重复建档']],
    ['匹配判断', '为什么推荐他，\n一眼看懂', '候选人不只是一行名字。OneCrew 组合市场、品类、受众与互动表现，把海量名单收窄成一组可以判断、保存和联系的候选人。', ['组合条件筛选', '查看匹配原因', '保存到 Campaign']],
    ['沟通与触达', 'AI 先准备，\n你确认后发送', 'AI 结合达人资料、品牌目标和历史对话准备个性化外联与回复草稿。你可以修改、审核，再通过支持的渠道发送。', ['多邮箱切换，结合上下文起草', '收件、已发送、草稿与归档', 'Agent 触达与人工会话管理', '设定跟进节奏，重要消息人工确认']],
    ['跟进与协作', 'AI 识别下一步，\n团队不会漏跟', 'AI 识别回复意向、归纳重点并准备下一步；团队同步负责人、任务状态和沟通进度，在关键节点确认后继续推进。', ['一位联系人，一套上下文', '回复意向与行动建议', '合作阶段持续更新']],
  ],
  en: [
    ['CREATOR SEARCH', 'Find creators you can\nactually act on', 'Search by market, platform, content, audience, and brand fit—not just follower count.', ['Find creators by platform, market, and keyword', 'Compare audience, engagement, and fit signals', 'Keep sources and creator identities connected']],
    ['BRAND FIT', 'Teach your AI\nwhat fits your brand', 'Define your market, audience, positioning, and creator criteria once. OneCrew carries that context into every search.', ['Combine brand and audience criteria', 'See why each creator matches', 'Save qualified creators to a campaign']],
    ['OUTREACH', 'Scale outreach without\nsounding mass-produced', 'Use creator context, brand voice, and conversation history to prepare personal outreach and replies. You review before anything is sent.', ['Draft from creator and brand context', 'Keep inbox, sent, drafts, and archive together', 'Set follow-up timing', 'Approve important messages']],
    ['SHARED CONTEXT', 'Pick up every conversation\nwhere it left off', 'Connect creator profiles, email, supported social messages, and follow-ups so your AI works from the full relationship history.', ['One creator, one history', 'Surface reply intent and follow-ups', 'Keep partnership stages current']],
  ],
} as const;

const featureDemos = [SearchDemo, DnaMatchDemo, EmailDemo, RelationshipDemo];

export function FeatureShowcase({ locale = 'zh' }: { locale?: Locale }) {
  const en = locale === 'en';
  return <section id="capabilities" className={`${styles.section} ${styles.features}`}>
    <header className={styles.heading}><h2><span>{en ? 'Say what you need.' : 'OneCrew'}</span><strong>{en ? 'OneCrew gets it moving.' : '如何让 AI 参与工作？'}</strong></h2><p>{en ? 'Inside the OneCrew desktop app, your AI gets the creator data, brand context, channels, and actions it needs to move the work forward.' : '在 OneCrew 客户端中连接你已有权限的 AI，用对话发起搜索、外联和跟进，并在面板中审核关键动作。'}</p></header>
    <FeatureIntroDemo en={en} />
    <div className={styles.featureRows}>{featureCopy[locale].map(([label, title, description, bullets], index) => { const Demo = featureDemos[index]; return <article className={styles.featureRow} key={label}><div className={styles.featureCopy}><span>{label}</span><h3>{title}</h3><p>{description}</p><ul>{bullets.map(item => <li key={item}>{item}</li>)}</ul><Link href={index < 2 ? '/product/creator-discovery' : index === 2 ? '/product/unified-inbox' : '/product/ai-agent'}>{en ? 'View details →' : '查看详情 →'}</Link></div><Demo en={en} /></article>; })}</div>
  </section>;
}

export function AudienceSection({ locale = 'zh' }: { locale?: Locale }) {
  const en = locale === 'en';
  const cards = en ? [
    ['/use-cases/ecommerce-brands', '/images/audience-brand-bd.svg', 'BRANDS', 'Find creators who actually fit', 'Go from a campaign brief to a qualified shortlist without hours of manual research.'],
    ['/use-cases/solo-founders', '/images/audience-founder.svg', 'FOUNDERS & GROWTH TEAMS', 'Move from research to outreach faster', 'Find opportunities, prepare personal outreach, and keep momentum without switching tools.'],
    ['/use-cases/agencies', '/images/audience-agency.svg', 'AGENCIES', 'Run more campaigns without more spreadsheets', 'Keep creators, conversations, owners, and follow-ups organized across clients.'],
  ] : [
    ['/use-cases/ecommerce-brands', '/images/audience-brand-bd.svg', '出海品牌 BD', '建立稳定的达人合作管线', '按市场与品牌匹配筛选达人，把建联记录和合作阶段统一管理。'],
    ['/use-cases/solo-founders', '/images/audience-founder.svg', '商家与个人创业者', '不搭复杂系统，也能持续外联', '快速找到合适的人、准备个性化沟通，并由 Agent 提醒下一步。'],
    ['/use-cases/agencies', '/images/audience-agency.svg', '营销代理团队', '跨项目协作，不丢客户上下文', '在多个并行项目中同步达人名单、沟通记录、负责人和跟进节奏。'],
  ];
  return <section id="audiences" className={`${styles.section} ${styles.audience}`}><header className={styles.heading}><div><h2><span>{en ? 'Different teams.' : 'OneCrew'}</span><strong>{en ? 'One creator outreach workflow.' : '满足不同的合作需求'}</strong></h2><p>{en ? 'From a campaign brief to follow-up, keep every creator relationship moving in one place.' : '从找人、建联到持续跟进，OneCrew 对应不同团队最常见的工作方式。'}</p></div><RequestButton label={en ? 'Try OneCrew' : '立即体验'} /></header><div className={`${styles.inner} ${styles.audienceGrid}`}>{cards.map(([href, icon, label, title, description]) => <Link className={styles.audienceCard} href={href} key={label}><img src={icon} alt="" /><div><span>{label}</span><h3>{title}</h3><p>{description}</p></div></Link>)}</div></section>;
}
