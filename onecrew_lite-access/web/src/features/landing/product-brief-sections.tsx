type Locale = 'zh' | 'en';

const painPoints = {
  zh: [
    ['筛选没有依据', '找到大量 Creator，却不知道谁更值得先联系。'],
    ['对话散在各处', '邮件、Instagram、X 与 Telegram 消息需要反复切换。'],
    ['跟进依赖记忆', '没有稳定提醒时，正向回复也可能错过窗口。'],
    ['资料与关系无法沉淀', '达人资料、历史沟通和多张表格长期分散，团队只能反复复制、去重与更新。'],
  ],
  en: [
    ['No reason to prioritize', 'Teams find plenty of creators but lack context on who to contact first.'],
    ['Conversations are scattered', 'Email, Instagram, X, and Telegram require constant switching.'],
    ['Follow-up depends on memory', 'Even positive replies can go cold without a reliable reminder.'],
    ['Knowledge never compounds', 'Creator profiles, conversation history, and multiple spreadsheets stay fragmented, forcing teams to copy, deduplicate, and update records repeatedly.'],
  ],
} as const;

const painImages = [
  '/images/pain-prioritize.jpg',
  '/images/pain-scattered-conversations.jpg',
  '/images/pain-follow-up.jpg',
  '/images/pain-spreadsheets.jpg',
] as const;

const connections = {
  zh: [
    ['X', '隔离浏览器登录', '连接品牌 X 账号，支持 X Chat。'],
    ['Instagram', '隔离浏览器登录', '连接品牌 Instagram 账号，管理达人沟通。'],
    ['Telegram', '二维码或手机号登录', '接入 Telegram 对话，需配置 SOCKS5 与 API 凭据。'],
    ['YouTube', 'YouTube Data API', '搜索 Creator 频道并分析公开数据。'],
    ['邮件', '品牌邮件身份', '统一发送、接收并管理达人外联邮件。'],
    ['TikTok', 'TikTok 达人搜索', '搜索、筛选并分析 TikTok 达人信息。'],
  ],
  en: [
    ['X', 'Isolated browser login', 'Supports X Chat'],
    ['Instagram', 'Isolated browser login', 'Use your brand account'],
    ['Telegram', 'QR code or phone login', 'Requires SOCKS5 and API credentials'],
    ['YouTube', 'YouTube Data API', 'Search and analyze channels'],
    ['Email', 'Brand email identity', 'Send, receive, and manage creator outreach in one place.'],
    ['TikTok', 'TikTok creator search', 'Search, filter, and analyze TikTok creator profiles.'],
  ],
} as const;

const connectionIcons = [
  '/images/figma-channel-x.svg',
  '/images/figma-channel-instagram.svg',
  '/images/figma-channel-telegram.svg',
  '/images/figma-channel-youtube.svg',
  '/images/figma-channel-email.svg',
  '/images/figma-channel-tiktok.png',
] as const;

export function ProductPainPoints({ locale = 'zh' }: { locale?: Locale }) {
  const en = locale === 'en';
  return <section className="brief-pain section-shell" aria-labelledby="brief-pain-title">
    <header><h2 id="brief-pain-title"><span>{en ? 'Why is' : '为什么'}</span><strong>{en ? 'traditional creator outreach so inefficient?' : '传统达人外联这么低效？'}</strong></h2><p>{en ? 'The real drag on creator outreach is context scattered across tools.' : '真正拖慢达人外联的，是信息散在各处'}</p></header>
    <ol>{painPoints[locale].map(([title, detail], index) => <li key={title} tabIndex={0}>
      <figure><img src={painImages[index]} alt="" loading="lazy" /></figure>
      <div><span>{String(index + 1).padStart(2, '0')}</span><h3>{title}</h3><p>{detail}</p></div>
    </li>)}</ol>
  </section>;
}

export function ProductCoverage({ locale = 'zh' }: { locale?: Locale }) {
  const en = locale === 'en';
  return <section id="channels" className="brief-connections section-shell" aria-labelledby="brief-connections-title">
    <header><h2 id="brief-connections-title"><span>OneCrew</span><strong>{en ? 'Supported channels' : '支持哪些渠道'}</strong></h2><p>{en ? 'Connect X, Instagram, Telegram, YouTube, TikTok, and your brand inbox to keep creator discovery, conversations, and follow-up in one workflow.' : '连接 X、Instagram、Telegram、YouTube 和品牌邮箱，把达人发现、沟通与跟进集中到同一个工作流。'}<br />{en ? 'X and Instagram use isolated browser sessions. Keys and login details stay encrypted locally and are never displayed back in the interface.' : 'X 和 Instagram 通过隔离浏览器登录；密钥与登录信息保存在本机加密凭据库中，不在界面中回显。'}</p></header>
    <div className="brief-connection-grid">{connections[locale].map(([name, access, use], index) => <article key={name}><div><b className={`channel-icon channel-icon-${index}`}><img src={connectionIcons[index]} alt="" /></b><strong>{name}</strong></div><p>{access}</p><small>{use}</small></article>)}</div>
  </section>;
}
