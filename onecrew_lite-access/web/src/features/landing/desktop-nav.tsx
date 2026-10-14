'use client';

import { useEffect, useRef, useState } from 'react';
import { PUBLIC_BLOG_URL } from "@/lib/public-site";
import { LandingLink as Link } from "./landing-link";
import { LangText, useLandingLocale } from "./language-switcher";

export function DesktopNav(){
  const locale = useLandingLocale();
  const [capabilitiesOpen, setCapabilitiesOpen] = useState(false);
  const detailsRef = useRef<HTMLDetailsElement>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  function cancelClose(){
    if(!closeTimerRef.current) return;
    clearTimeout(closeTimerRef.current);
    closeTimerRef.current = null;
  }

  function scheduleClose(){
    cancelClose();
    closeTimerRef.current = setTimeout(() => setCapabilitiesOpen(false), 360);
  }

  useEffect(() => {
    function closeFromOutside(event: PointerEvent){
      if(!detailsRef.current?.contains(event.target as Node)) setCapabilitiesOpen(false);
    }

    function closeFromKeyboard(event: KeyboardEvent){
      if(event.key !== 'Escape') return;
      setCapabilitiesOpen(false);
      detailsRef.current?.querySelector('summary')?.focus();
    }

    document.addEventListener('pointerdown', closeFromOutside);
    document.addEventListener('keydown', closeFromKeyboard);

    return () => {
      cancelClose();
      document.removeEventListener('pointerdown', closeFromOutside);
      document.removeEventListener('keydown', closeFromKeyboard);
    };
  }, []);

  return (
    <nav className="desktop-nav" aria-label={locale === "en" ? "Primary navigation" : "主导航"}>
      <details
        ref={detailsRef}
        className="nav-capabilities"
        open={capabilitiesOpen}
        onMouseEnter={cancelClose}
        onMouseLeave={scheduleClose}
        onToggle={(event) => setCapabilitiesOpen(event.currentTarget.open)}
      >
        <summary
          onClick={(event) => {
            event.preventDefault();
            cancelClose();
            setCapabilitiesOpen((open) => !open);
          }}
        >
          <LangText zh="能力" en="Capabilities" />
          <img
            className="nav-chevron"
            src="/images/figma-nav-chevron.svg"
            alt=""
            aria-hidden="true"
          />
        </summary>
        <div
          className="header-dropdown-menu nav-capabilities-menu"
          onClick={() => setCapabilitiesOpen(false)}
        >
          <Link href="/product/creator-discovery">
            <LangText zh="达人发现" en="Creator discovery" />
          </Link>
          <Link href="/product/unified-inbox">
            <LangText zh="统一收件箱" en="Unified inbox" />
          </Link>
          <Link href="/product/ai-agent">AI Agent</Link>
        </div>
      </details>
      <ResourceMenu />
      <a href={PUBLIC_BLOG_URL}><LangText zh="博客" en="Blog" /></a>
      <Link href="/pricing"><LangText zh="定价" en="Pricing" /></Link>
      <Link href="/about"><LangText zh="关于" en="About" /></Link>
    </nav>
  );
}

function ResourceMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    function outside(event: PointerEvent) { if (!ref.current?.contains(event.target as Node)) setOpen(false); }
    function escape(event: KeyboardEvent) {
      if (event.key === "Escape" && ref.current?.open) { setOpen(false); ref.current.querySelector("summary")?.focus(); }
    }
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => { document.removeEventListener("pointerdown", outside); document.removeEventListener("keydown", escape); };
  }, []);
  return <details ref={ref} className="nav-capabilities" open={open}>
    <summary onClick={event => { event.preventDefault(); setOpen(value => !value); }}>
      <LangText zh="资源" en="Resources" /><img className="nav-chevron" src="/images/figma-nav-chevron.svg" alt="" aria-hidden="true" />
    </summary>
    <div className="header-dropdown-menu nav-resources-menu" onClick={() => setOpen(false)}>
      <Link href="/resources/user-guide"><LangText zh="使用指南" en="User guide" /></Link>
      <Link href="/resources/kol-outreach-guide"><LangText zh="KOL 外联指南" en="Creator outreach guide" /></Link>
      <Link href="/resources"><LangText zh="全部资源" en="All resources" /></Link>
    </div>
  </details>;
}
