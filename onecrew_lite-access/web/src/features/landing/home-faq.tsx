'use client';

import { useState } from 'react';

type Locale = 'zh' | 'en';

const copy = {
  zh: {
    eyebrow: '常见问题',
    title: '关于 OneCrew，\n你可能还想知道',
    intro: '从产品运行方式、支持范围到数据安全与 AI 的操作边界。',
    items: [
      ['OneCrew 是什么？', 'OneCrew 是面向出海品牌的 AI 达人发现与外联工作台。它把达人数据、品牌邮箱、支持的社交渠道和你已有权限的 AI 服务连接起来，让团队通过对话完成达人搜索、外联和持续跟进。'],
      ['为什么需要下载 OneCrew 客户端？', 'OneCrew 通过自己的桌面客户端连接工作面板、本地凭据、隔离浏览器会话和你已有权限的 AI 服务，因此不能直接在 Claude 或 Codex 官方客户端中运行。'],
      ['OneCrew 支持哪些 AI 和外联渠道？', 'OneCrew 可连接已受支持且你已有使用权限的 AI 服务。目前覆盖 X、Instagram、Telegram、品牌邮箱、YouTube 达人公开数据和 TikTok 达人搜索；不同渠道支持的登录、搜索和沟通能力会有所不同。'],
      ['OneCrew 如何帮助筛选达人？', 'OneCrew 根据品牌、产品、目标市场、受众与内容信号缩小候选范围，并展示可理解的匹配依据，方便团队判断是否值得联系。'],
      ['邮件和社交私信会如何管理？', '邮件与支持的社交私信会归到对应联系人，历史触达、回复和下一步可以在同一处查看。'],
      ['AI Agent 会自动发送重要消息吗？', 'Agent 可以检查回复、提醒跟进并准备下一步；重要消息与合作承诺仍由团队确认后执行。'],
      ['账号凭据和 API 密钥如何处理？', '支持的社交账号使用隔离浏览器会话；凭据和 API 密钥在本机加密保存，不会在产品界面中明文显示。'],
      ['OneCrew 适合哪些团队？', '适合持续开展达人合作的出海品牌 BD、跨境商家、个人创业者和营销代理团队。无论一人负责外联，还是多人并行推进，都能围绕同一份达人和沟通记录协作。'],
      ['可以先体验再决定吗？', '可以。点击页面中的“立即体验”登录或创建账号，之后可在账号中心管理登录安全、邮箱和已连接设备。'],
    ],
  },
  en: {
    eyebrow: 'FAQ',
    title: 'Questions about\nOneCrew?',
    intro: 'How OneCrew works, what it connects, how it protects credentials, and where your team stays in control.',
    items: [
      ['What is OneCrew?', 'OneCrew is an AI-native creator discovery and outreach workspace for global brands. In the OneCrew desktop app, you connect creator data, brand email, supported social channels, and an AI service you already have access to.'],
      ['Why does OneCrew require a desktop app?', 'The desktop app connects the workspace, locally stored credentials, isolated browser sessions, and supported AI services you already use. OneCrew does not run inside the official Claude or Codex apps.'],
      ['Which AI services and outreach channels does OneCrew support?', 'OneCrew connects to supported AI services you already have access to. It currently works with X, Instagram, Telegram, brand email, public YouTube creator data, and TikTok creator discovery. Available sign-in, search, and messaging capabilities vary by channel.'],
      ['How does OneCrew help me find the right creators?', 'OneCrew narrows the field using your brand, product, target market, audience, and content signals. It also shows the reasoning behind each match so your team can decide who is worth contacting.'],
      ['How are email and social conversations organized?', 'Email and supported social messages are linked to the right creator record, so your team can review outreach history, replies, and the next step in one place.'],
      ['Does the AI Agent send important messages automatically?', 'The Agent can review replies, surface follow-ups, and prepare the next step. Your team stays in control of important messages and partnership commitments.'],
      ['How does OneCrew protect account credentials and API keys?', 'Supported social accounts use isolated browser sessions. Credentials and API keys are encrypted locally and never displayed in the product interface.'],
      ['Who is OneCrew built for?', 'OneCrew is designed for global brand teams, cross-border sellers, solo operators, and agencies that run ongoing creator outreach. It supports both individual workflows and teams working from shared creator and conversation records.'],
      ['Can I try OneCrew before deciding?', 'Yes. Select “Try OneCrew” to sign in or create an account, then manage login security, email, and connected devices in Account settings.'],
    ],
  },
} as const;

export function HomeFaq({ locale = 'zh' }: { locale?: Locale }) {
  const content = copy[locale];
  const [openIndex, setOpenIndex] = useState<number | null>(0);
  const schema = {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: content.items.map(([question, answer]) => ({
      '@type': 'Question',
      name: question,
      acceptedAnswer: { '@type': 'Answer', text: answer },
    })),
  };

  return <section id="faq" className="home-faq" aria-labelledby="home-faq-title">
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(schema) }} />
    <header>
      <span className="home-faq-eyebrow">{content.eyebrow}</span>
      <h2 id="home-faq-title">{content.title}</h2>
      <p>{content.intro}</p>
    </header>
    <div className="home-faq-list">
      {content.items.map(([question, answer], index) => {
        const isOpen = openIndex === index;
        const panelId = `home-faq-panel-${locale}-${index}`;
        return <article className={isOpen ? 'is-open' : ''} key={question}>
          <button type="button" aria-expanded={isOpen} aria-controls={panelId} onClick={() => setOpenIndex(isOpen ? null : index)}>
            <span>{question}</span>
            <span className="home-faq-toggle" aria-hidden="true"><i /><i /></span>
          </button>
          <div className="home-faq-answer" id={panelId} aria-hidden={!isOpen}>
            <div><p>{answer}</p></div>
          </div>
        </article>;
      })}
    </div>
  </section>;
}
