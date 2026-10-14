'use client';

import { useEffect, useState } from 'react';
import { useReducedMotion } from './use-reduced-motion';

export function HeroHeadline({ locale }: { locale: 'zh' | 'en' }) {
  const [phase, setPhase] = useState(0);
  const reducedMotion = useReducedMotion();

  useEffect(() => {
    if (reducedMotion) return;
    const timer = window.setInterval(() => setPhase(value => (value + 1) % 2), 2600);
    return () => window.clearInterval(timer);
  }, [reducedMotion]);

  const en = locale === 'en';
  const visiblePhase = reducedMotion ? 0 : phase;
  return <h1 aria-label={en ? 'Turn Codex or Claude into a creator outreach agent' : '让 Codex 或 Claude 帮你找到对的人'}>
    <span className="hero-ai-line" aria-hidden="true">
      <span className="hero-ai-prefix">{en ? 'Turn your' : '让你的'}</span>
      <span className={`hero-ai-roller hero-ai-roller-phase-${visiblePhase}`} data-phase={visiblePhase}>
        <span className="hero-ai-roller-track">
          <b><img src="/codex-mark.png" alt="" />Codex</b>
          <b><img src="/claude-mark.png" alt="" />Claude</b>
        </span>
      </span>
    </span>
    <span className="hero-ai-outcome" aria-hidden="true">{en ? 'into a creator outreach agent' : '帮你找到对的人'}</span>
  </h1>;
}
