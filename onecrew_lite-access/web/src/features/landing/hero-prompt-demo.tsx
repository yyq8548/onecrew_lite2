'use client';

import { useEffect, useState } from 'react';

import { useAccessI18n } from '@/lib/access-i18n';

import { CreatorAvatar, type CreatorId } from "./creator-avatar";
import { useReducedMotion } from './use-reduced-motion';

const creators: CreatorId[] = [
  'ava', 'mei', 'imani', 'ethan', 'lina',
  'marcus', 'claire', 'sofia', 'nora', 'jonas',
];

const positions = ['one', 'two', 'three', 'four', 'five'] as const;
const layoutCount = 3;
const roundCount = layoutCount * (creators.length / positions.length);
// The final staggered card finishes its 5.5s animation after a 0.9s delay.
const resultsDuration = 6600;

export function HeroPromptDemo() {
  const { locale, t } = useAccessI18n();
  const reducedMotion = useReducedMotion();
  const [round, setRound] = useState(0);
  const promptStart = t('layout.showcaseDemo.promptStart');
  const [typedText, setTypedText] = useState(promptStart);
  const [phase, setPhase] = useState<'typing' | 'showing'>('typing');
  const prompt = t('layout.showcaseDemo.prompt');

  useEffect(() => {
    /* eslint-disable react-hooks/set-state-in-effect -- The effect intentionally restarts the timed demo and resolves reduced-motion users to its final state. */
    if (reducedMotion) {
      setTypedText(prompt);
      setPhase('showing');
      return;
    }

    setTypedText(promptStart);
    setPhase('typing');
    /* eslint-enable react-hooks/set-state-in-effect */
    let index = promptStart.length;
    let typingTimer = 0;
    const timers: number[] = [];
    timers.push(window.setTimeout(() => {
      typingTimer = window.setInterval(() => {
        index += 1;
        setTypedText(prompt.slice(0, index));
        if (index >= prompt.length) {
          window.clearInterval(typingTimer);
          setPhase('showing');
          timers.push(window.setTimeout(() => setRound(value => (value + 1) % roundCount), resultsDuration));
        }
      }, 34);
    }, 380));
    return () => {
      window.clearInterval(typingTimer);
      timers.forEach(window.clearTimeout);
    };
  }, [prompt, promptStart, round, reducedMotion]);

  const burst = Array.from({ length: 5 }, (_, index) => {
    const creatorId = creators[(round * 5 + index) % creators.length];
    return { creatorId, position: positions[index] };
  });
  const layout = round % layoutCount;
  const en = locale === 'en';

  return <section className="hero-prompt-stage" aria-label={t('layout.showcaseDemo.ariaLabel')}>
    <div key={round} className={`hero-kol-burst${phase === 'showing' ? ' is-showing' : ' is-waiting'}`} aria-live="off">
      {burst.map(({ creatorId, position }, index) => <article key={creatorId} className={`hero-kol-card hero-kol-card-${position} hero-kol-layout-${layout}-${position} hero-kol-delay-${index}`}>
        <CreatorAvatar person={creatorId} large />
        <div>
          <small>{t(`layout.showcaseDemo.creators.${creatorId}.channel`)}</small>
          <strong>{t(`layout.showcaseDemo.creators.${creatorId}.handle`)}</strong>
          <span>{t(`layout.showcaseDemo.creators.${creatorId}.detail`)}</span>
        </div>
      </article>)}
    </div>
    <div className="hero-prompt-demo">
      <header>
        <div><img src="/onecrew-ai-mark.png" alt="" /><span><strong>OneCrew AI</strong><small>{t('layout.showcaseDemo.creatorDiscovery')}</small></span></div>
      </header>
      <div className="hero-prompt-field">
        <p className={`hero-prompt-text${phase === 'typing' ? ' is-typing' : ''}`} lang={en ? 'en' : 'zh-CN'}><span>{typedText}</span></p>
        <div className={`hero-prompt-result${phase === 'showing' ? ' is-visible' : ''}`}>
          <span aria-hidden="true"><svg viewBox="0 0 24 24"><path d="m7 12 3 3 7-7" /></svg></span>
          <div><strong>{t('layout.showcaseDemo.resultTitle')}</strong><small>{t('layout.showcaseDemo.resultDescription')}</small></div>
        </div>
        <footer role="presentation">
          <span className="hero-prompt-attachment"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg> {t('layout.showcaseDemo.addBrief')}</span>
          <button
            className="hero-prompt-run"
            disabled={phase === 'typing'}
            onClick={() => setRound(value => (value + 1) % roundCount)}
            type="button"
          >
            {t('layout.showcaseDemo.runTask')} <span aria-hidden="true">→</span>
          </button>
        </footer>
      </div>
    </div>
  </section>;
}
