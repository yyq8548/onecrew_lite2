'use client';

import { useEffect, useRef, useState } from 'react';

export function FeatureIntroConversation({ en }: { en: boolean }) {
  const panelRef = useRef<HTMLElement>(null);
  const [isPlaying, setIsPlaying] = useState(false);

  useEffect(() => {
    const panel = panelRef.current;
    if (
      !panel ||
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ||
      typeof IntersectionObserver === "undefined"
    ) return;

    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      setIsPlaying(true);
      observer.disconnect();
    }, { threshold: 0.35 });

    observer.observe(panel);
    return () => observer.disconnect();
  }, []);

  return <section ref={panelRef} className={`codex-task-pane${isPlaying ? ' is-playing' : ''}`}>
    <div className="codex-task-body">
      <h4>{en ? 'What are we working on?' : '这次要做什么？'}</h4>
      <div className="codex-task-prompt">
        <p>{en ? 'Find 20 Instagram creators for a North American skincare launch. Prioritize authentic engagement and recent skincare content.' : '为北美市场的护肤新品寻找匹配达人，并准备自然、不像群发的第一轮外联。'}</p>
      </div>
      <div className="codex-task-response"><p>{en ? 'I found 48 candidates and narrowed the list to 20 based on brand fit, recent skincare content, engagement quality, and North American audience share. I ranked the 10 strongest matches first and included a reason for each recommendation.' : '找到了 48 位候选达人，已根据品牌匹配度、近期护肤内容、互动质量和北美受众占比筛选出 20 位。我把最匹配的 10 位排在前面，并附上了推荐理由。'}</p></div>
      <nav aria-label={en ? 'Next OneCrew actions' : 'OneCrew 后续操作'}><span>{en ? 'View shortlist' : '找人'}</span><span>{en ? 'Refine' : '补全'}</span><span>{en ? 'Draft outreach' : '触达'}</span><span>{en ? 'Schedule' : '定时'}</span></nav>
      <div className="codex-conversation-composer" aria-label={en ? 'AI conversation input' : 'AI 对话输入框'}>
        <footer role="presentation">
          <span className="codex-composer-left">
            <button className="codex-composer-plus" type="button" aria-label={en ? 'Add attachment' : '添加附件'}><img src="/images/figma-composer-plus.svg" alt="" /></button>
            <span className="codex-composer-access"><img src="/images/figma-composer-shield.svg" alt="" />{en ? 'Full access' : '完全访问'}</span>
          </span>
          <span className="codex-composer-right">
            <span className="codex-composer-model"><img src="/images/figma-composer-bolt.svg" alt="" /><span>5.6 Sol</span><span className="codex-composer-mode-detail"><small>{en ? 'Light' : '轻度'}</small><img src="/images/figma-composer-chevron.svg" alt="" /></span></span>
            <button className="codex-composer-send" type="button" aria-label={en ? 'Send' : '发送'}><img src="/images/figma-composer-send.svg" alt="" /></button>
          </span>
        </footer>
      </div>
    </div>
  </section>;
}
