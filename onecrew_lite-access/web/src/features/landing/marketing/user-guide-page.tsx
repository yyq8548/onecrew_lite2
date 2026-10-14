import { PUBLIC_CONTACT } from "@/lib/public-contact";
'use client';

import { useEffect, useRef, useState } from 'react';
import { LandingLink as Link } from "../landing-link";
import { SiteHeader, SiteFooter } from "../site-chrome";
import { DocumentMeta } from "./document-meta";
import { useMarketingContent } from "./use-marketing-content";
import { localizedAbsoluteUrl, oneCrewOrganization, schemaGraph } from "./schema";
import { guideSections, guideFaq, guideLinks, prompts, type GuideLocale } from './user-guide';
import styles from './UserGuide.module.css';

export function UserGuidePage() {
  const { locale } = useMarketingContent();
  const i = locale === 'en' ? 1 : 0;
  const t = (zh: string, en: string) => i ? en : zh;
  const schema = schemaGraph(oneCrewOrganization, {
    "@type": "Article",
    headline: t("OneCrew 使用指南", "OneCrew User Guide"),
    dateModified: "2026-09-09",
    inLanguage: locale === "en" ? "en" : "zh-CN",
    url: localizedAbsoluteUrl("/resources/user-guide", locale),
    author: oneCrewOrganization,
    publisher: oneCrewOrganization,
  });
  const [copyState, setCopyState] = useState<{ index: number; ok: boolean } | null>(null);
  const toc = [...guideSections.map(s => ({ id: s.id, title: s.title[i] })), { id: 'prompts', title: t('可复制指令', 'Prompt templates') }, { id: 'safety', title: t('安全与发送规则', 'Safety and sending') }, { id: 'faq', title: t('常见问题', 'Troubleshooting') }, { id: 'links', title: t('官方入口与帮助', 'Official links and help') }];
  async function copy(index: number) {
    try { await navigator.clipboard.writeText(prompts[index].text[i]); setCopyState({ index, ok: true }); }
    catch { setCopyState({ index, ok: false }); }
  }
  function renderPrompt(n: number) {
    const prompt = prompts[n];
    return <div className={styles.prompt} key={n}><div><h3>{prompt.title[i]}</h3><button type="button" onClick={() => copy(n)} aria-label={t('复制：', 'Copy: ') + prompt.title[i]}>{copyState?.index === n && copyState.ok ? t('已复制', 'Copied') : t('复制指令', 'Copy prompt')}</button></div><p>{prompt.text[i]}</p>{copyState?.index === n && <small role="status">{copyState.ok ? t('已复制，可粘贴到 AI 对话中。', 'Copied. Paste it into your AI conversation.') : t('浏览器未允许复制，请选中文字后手动复制。', 'Clipboard access was blocked. Select the text and copy it manually.')}</small>}</div>;
  }
  return <><SiteHeader /><DocumentMeta schema={schema} canonicalPath="/resources/user-guide" title={t("OneCrew 使用指南", "OneCrew User Guide")} description={t("从安装到第一次搜索与保存，区分通用 AI 与 Codex 的接入步骤。", "Install OneCrew, connect your AI, and complete your first search and save workflow.")} /><main className={styles.page} id="main-content" tabIndex={-1}>
    <nav className={styles.breadcrumb} aria-label={t('面包屑', 'Breadcrumb')}><Link href="/">{t('首页', 'Home')}</Link><span aria-hidden="true">/</span><span aria-current="page">{t('使用指南', 'User guide')}</span></nav>
    <header className={styles.intro}><div><h1>{t('OneCrew 快速开始与使用指南', 'Get started with OneCrew')}</h1><p>{t('从安装到第一次搜索，把合适的 Creator 保存为可跟进的线索。', 'Install OneCrew, run your first search, and save creators as leads.')}</p><small>{t('更新时间：2026 年 9 月 9 日', 'Last updated: September 9, 2026')}</small></div><a className={styles.pill} href="#quick-start">{t('开始使用', 'Get started')} <span aria-hidden="true">→</span></a></header>
    <div className={styles.layout}>
      <aside className={styles.sidebar}><nav aria-label={t('指南目录', 'Guide contents')}><h2>{t('本页目录', 'On this page')}</h2>{toc.map(item => <a key={item.id} href={'#' + item.id}>{item.title}</a>)}</nav></aside>
      <article className={styles.article}>
        {guideSections.map(section => <section id={section.id} key={section.id}>
          <h2>{section.title[i]}</h2>{section.intro && <p>{section.intro[i]}</p>}
          {section.id === 'open-interface' && section.promptIndexes?.map(renderPrompt)}
          {section.steps && <ol>{section.steps.map((step, n) => <li key={n}>{step[i]}{section.stepPrompts?.[n]?.map(renderPrompt)}{section.id === 'installation' && n === 0 && ['apply', 'review', 'install'].map(key => <GuideScreenshot key={key} locale={locale} image={guideImages[key]}/>)}</li>)}</ol>}
          {section.table && <div className={styles.tableWrap}><table><caption>{section.title[i]}</caption><thead><tr>{section.table.headers.map((header, n) => <th key={n} scope="col">{header[i]}</th>)}</tr></thead><tbody>{section.table.rows.map((row, n) => <tr key={n}>{row.map((cell, c) => c === 0 ? <th key={c} scope="row">{cell[i]}</th> : <td key={c}>{cell[i]}</td>)}</tr>)}</tbody></table></div>}
          {section.id === 'open-interface' && <GuideScreenshot locale={locale} image={guideImages.status}/>}
          {section.id === 'workflow' && <GuideScreenshot locale={locale} image={guideImages.task}/>}
          {section.id === 'email' && <GuideScreenshot locale={locale} image={guideImages.resend}/>}
          {section.id === 'instagram-x' && <GuideScreenshot locale={locale} image={guideImages.instagram}/>}
          {section.id === 'x' && <GuideScreenshot locale={locale} image={guideImages.x}/>}
          {section.id !== 'open-interface' && section.promptIndexes?.map(renderPrompt)}{section.links && <ul className={styles.links}>{section.links.map(link => <li key={link.id}><a href={'#' + link.id}>{link.title[i]} →</a></li>)}</ul>}{section.note && <p className={styles.note}>{section.note[i]}</p>}{section.notePrompts?.map(renderPrompt)}
        </section>)}
        <section id="prompts"><h2>{t('复制一条指令，开始小任务', 'Start small with a ready-to-use prompt')}</h2><p>{t('把平台、数量和邮箱改成你的真实需求。模板不会自动执行，也不会包含任何密钥。', 'Adjust the platform, quantity, and account to your needs. These templates do not run automatically and contain no credentials.')}</p>
          {prompts.map((_, n) => n === 0 || n === 9 || n >= 12 ? null : renderPrompt(n))}
        </section>
        <section id="safety"><h2>{t('安全与发送规则', 'Safety and sending rules')}</h2><ul>
          <li>{t('密钥、密码、OTP、2FA 和代理密码只填写在专用安全字段，不放入提示词或共享文档。', 'Enter API keys, passwords, OTPs, 2FA codes, and proxy passwords only in dedicated secure fields, never in prompts or shared documents.')}</li>
          <li>{t('使用自己获授权的连接，避免共享高权限密钥。发现泄露或人员离职时，断开相关账号并撤销密钥。', 'Use authorized personal connections instead of shared high-privilege keys. Disconnect accounts and revoke keys after a suspected leak or an employee departure.')}</li>
          <li>{t('从小批量、个性化外联开始。核对发件身份、收件人、链接和附件，不让 AI 猜测缺失邮箱。', 'Start with small, personalized batches. Verify sender identity, recipients, links, and attachments. Do not let AI guess missing email addresses.')}</li>
          <li>{t('新发信域名不要第一天就大量发送；先完成小批量测试，再根据实际反馈调整。', 'Do not send large volumes from a new domain on day one. Test small batches and adjust based on actual results.')}</li>
          <li>{t('只联系与业务相关的对象，清楚说明自己是谁、联系原因，并提供退订或停止联系的途径。', 'Contact only business-relevant recipients, clearly identify yourself and your reason for contact, and provide an unsubscribe or stop-contact option.')}</li>
          <li>{t('遵守收件人所在地区适用的隐私与反垃圾邮件要求；不要共享社交账号，人员离职或密钥泄露后撤销旧凭据并按需重新生成。', 'Follow applicable privacy and anti-spam requirements in recipients’ regions. Avoid shared social accounts; revoke old credentials after departures or leaks and replace them as needed.')}</li>
          <li>{t('遵守平台规则与适用要求，提供清晰的停止联系途径，不绕过验证码、限流或账号安全控制。', 'Follow platform rules and applicable requirements, provide a clear way to stop contact, and never bypass verification, rate limits, or account security controls.')}</li>
        </ul><p className={styles.note}>{t('搜索和整理与对外发送是不同权限。邮件、私信、发帖、删除连接或不可逆操作，必须先检查，再明确确认。', 'Searching and organizing do not authorize sending. Review and explicitly approve emails, DMs, posts, deleting connections, and irreversible actions.')}</p></section>
        <section id="faq"><h2>{t('常见问题', 'Troubleshooting')}</h2>{guideFaq.map((item,n) => <details className={styles.faq} key={n}><summary>{item.q[i]}</summary><p>{item.a[i]}</p></details>)}</section>
        <section id="links"><h2>{t('官方入口与帮助', 'Official links and help')}</h2><ul className={styles.links}>{guideLinks.map(link => <li key={link.href}><a href={link.href} target="_blank" rel="noopener noreferrer">{link.title[i]} ↗</a></li>)}</ul><p>{t('第三方界面、配额及审核规则可能调整，实际操作以当前官方页面为准。', 'Third-party interfaces, quotas, and review rules may change. Follow the current official instructions.')}</p><p>{t('需要帮助？联系 ', 'Need help? Contact ')}<a href={`mailto:${PUBLIC_CONTACT.email}`}>{PUBLIC_CONTACT.email}</a>{t('，请不要在邮件中附上密码或密钥。', '. Do not include passwords or keys in your email.')}</p><a href="#main-content">{t('返回顶部', 'Back to top')} ↑</a></section>
      </article>
    </div>
  </main><SiteFooter /></>;
}

type GuideImage = { src: string; width: number; height: number; title: [string, string]; alt: [string, string]; caption: [string, string] };
const guideImages: Record<string, GuideImage> = {
  apply: { src: '/images/user-guide/access-apply-0521.png', width: 1280, height: 563, title: ['申请体验', 'Apply for access'], alt: ['在官网点击“立即体验”，进入申请流程。', 'Select Try now on the official website to begin your application.'], caption: ['在官网填写体验申请。', 'Submit an access application on the official website.'] },
  review: { src: '/images/user-guide/access-review-0521.png', width: 1280, height: 734, title: ['等待审核', 'Await review'], alt: ['提交申请后查看审核状态。', 'Check the review status after applying.'], caption: ['提交申请后查看审核状态。', 'Check the review status after applying.'] },
  install: { src: '/images/user-guide/access-install-0521.png', width: 1280, height: 718, title: ['复制安装提示词', 'Copy the installation prompt'], alt: ['审核通过后复制自己的专属安装提示词；图中的时间和状态仅为示例。', 'After approval, copy your personal installation prompt. Dates and statuses shown are examples.'], caption: ['审核通过后复制自己的专属安装提示词；图中的时间和状态仅为示例。', 'After approval, copy your personal installation prompt. Dates and statuses shown are examples.'] },
  status: { src: '/images/user-guide/login-status-0521.png', width: 1280, height: 695, title: ['OneCrew 服务与登录状态', 'OneCrew service and login status'], alt: ['打开 OneCrew 后点击左侧“连接”：查看本地服务是否运行、账号是否已登录。', 'In OneCrew, select Connect to check the local service and login status.'], caption: ['在连接页核对本地服务、版本和登录状态。', 'Check the local service, version, and login status.'] },
  task: { src: '/images/user-guide/codex-task-0521.png', width: 1280, height: 417, title: ['执行任务（以 Codex 为例）', 'Run a task (Codex example)'], alt: ['Codex 操作示例：选择 OneCrew 并描述任务。', 'Codex example: select OneCrew and describe your task.'], caption: ['Codex 操作示例；其他 AI 的入口可能不同。', 'Codex example; other AI tools may use different entry points.'] },
  codex: { src: '/images/user-guide/connect-codex.png', width: 1280, height: 818, title: ['MCP / Codex 配置', 'MCP / Codex setup'], alt: ['连接页面：左侧选择连接，右上启动本地 MCP，下方配置 Codex 插件。', 'Original Chinese interface: Connect in the left menu, Start local MCP at the upper right, and Codex setup below.'], caption: ['原指南操作截图：旧版界面使用 Reacher 名称，当前 OneCrew 版本的名称与布局可能略有不同。', 'Source guide screenshot (Chinese). The older interface uses the Reacher name; labels may differ in the current OneCrew release.'] },
  resend: { src: '/images/user-guide/connect-resend.png', width: 1232, height: 1194, title: ['Resend 邮箱连接', 'Resend email connection'], alt: ['连接 Resend 邮箱：填写凭据名称、发件邮箱、显示名称及 API Key，点击保存并测试。', 'Chinese Resend connection form showing credential name, sender email, display name, masked API key, and Save and test.'], caption: ['原指南邮件配图：填写发件信息后，点击“保存并测试”。图中邮箱为示例，API Key 已隐藏。', 'Original email setup screenshot (Chinese): enter sender details, then Save and test. The address is an example and the API key is masked.'] },
  instagram: { src: '/images/user-guide/connect-instagram.png', width: 1118, height: 1046, title: ['Instagram 连接配置', 'Instagram connection setup'], alt: ['Instagram 连接向导：命名、家宽代理、登录、导入；当前显示代理类型、IP、端口和凭据字段。', 'Chinese Instagram connection wizard showing naming, proxy setup, login, and import; proxy type, IP, port, and credential fields are visible.'], caption: ['原指南 Instagram 配图：在登录前配置并检测代理，之后进入登录步骤。图中的 IP、端口及账号是示例，不可直接照填。', 'Original Instagram screenshot (Chinese): configure and test the proxy before sign-in. IP, port, and account values shown are examples, not usable credentials.'] },
  x: { src: '/images/user-guide/connect-x.png', width: 998, height: 976, title: ['X 连接配置', 'X connection setup'], alt: ['X 连接向导的家宽代理步骤：快速粘贴代理配置，检测后点击下一步进入登录。', 'Chinese X connection wizard: paste proxy settings, test the connection, and select Next to continue to sign-in.'], caption: ['原指南 X 配图：填写并检测代理配置，再点击“下一步”登录 X。图中参数为示例；X Chat passcode 不是这里的代理密码。', 'Original X screenshot (Chinese): enter and test proxy settings, then select Next to sign in. Values are examples; an X Chat passcode is not the proxy password.'] },
};

function GuideScreenshot({ locale, image }: { locale: GuideLocale; image: GuideImage }) {
  const english = locale === 'en';
  const i = english ? 1 : 0;
  const [open, setOpen] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (!open) return;
    const element = dialog.current;
    const previousOverflow = document.body.style.overflow;
    element?.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      element?.close();
      document.body.style.overflow = previousOverflow;
    };
  }, [open]);
  const alt = image.alt[i];
  return <figure className={styles.screenshot}>
    <button type="button" className={styles.imageTrigger} onClick={() => setOpen(true)} aria-haspopup="dialog" aria-label={(english ? 'View image: ' : '查看大图：') + image.title[i]}>
      <img src={image.src} width={image.width} height={image.height} loading="lazy" decoding="async" alt={alt}/>
      <span>{english ? 'View larger image' : '点击查看大图'}</span>
    </button>
    <figcaption>{image.caption[i]}</figcaption>
    <dialog ref={dialog} className={styles.imageDialog} aria-label={image.title[i]} onCancel={() => setOpen(false)} onClose={() => setOpen(false)} onClick={event => { if (event.target === event.currentTarget) setOpen(false); }}>
      <div className={styles.imageToolbar}><span>{image.title[i]}</span><button type="button" onClick={() => setOpen(false)}>{english ? 'Close' : '关闭'}<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg></button></div>
      {open && <img className={styles.largeImage} src={image.src} width={image.width} height={image.height} alt={alt}/>}
    </dialog>
  </figure>;
}
