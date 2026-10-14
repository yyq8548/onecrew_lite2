import { useEffect, useRef, useState } from "react";
import { CreatorAvatar, type CreatorId } from "@/features/landing/creator-avatar";
import { useAccessI18n } from "@/lib/access-i18n";
import "./auth-photo-carousel.css";

const slides: { id: string; position: string; person: CreatorId }[] = [
  { id: "connected", position: "62% center", person: "ava" },
  { id: "follow-up", position: "52% center", person: "sofia" },
  { id: "history", position: "54% center", person: "nora" },
  { id: "prioritize", position: "50% center", person: "jonas" },
  { id: "projects", position: "56% center", person: "mei" },
];

export function AuthPhotoCarousel() {
  const { t } = useAccessI18n();
  const [active, setActive] = useState(0);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [reduced, setReduced] = useState(false);
  const startX = useRef<number | null>(null);
  const go = (offset: number) => setActive(value => (value + offset + slides.length) % slides.length);
  useEffect(() => {
    const media = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (!media) return;
    const update = () => setReduced(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useEffect(() => {
    if (hovered || focused || reduced) return;
    const timer = window.setInterval(() => setActive(value => (value + 1) % slides.length), 5200);
    return () => window.clearInterval(timer);
  }, [hovered, focused, reduced]);
  return <section className="auth-photo-carousel" role="region" aria-roledescription="carousel"
    aria-label={t("auth.design.carouselLabel")} tabIndex={0}
    onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
    onFocusCapture={() => setFocused(true)} onBlurCapture={() => setFocused(false)}
    onKeyDown={event => {
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault(); go(event.key === "ArrowRight" ? 1 : -1);
      }
    }}
    onPointerDown={event => { startX.current = event.clientX; event.currentTarget.setPointerCapture?.(event.pointerId); }}
    onPointerCancel={() => { startX.current = null; }}
    onPointerUp={event => {
      if (startX.current !== null && Math.abs(event.clientX - startX.current) >= 50) go(event.clientX < startX.current ? 1 : -1);
      startX.current = null;
    }}>
    {slides.map((slide, index) => <article className="auth-photo-slide" key={slide.id} aria-hidden={active !== index} data-active={active === index}>
      <img className="auth-photo" src={`/images/auth-story-${slide.id}.jpg`} alt="" width="1122" height="1402" draggable={false} style={{ objectPosition: slide.position }} />
      <figure className="auth-photo-quote">
        <blockquote>{t(`auth.design.stories.${slide.id}.quote`)}</blockquote>
        <figcaption><CreatorAvatar person={slide.person} /><span>
          <strong>{t(`auth.design.stories.${slide.id}.role`)}</strong>
          <small>{t(`auth.design.stories.${slide.id}.company`)}</small>
        </span></figcaption>
      </figure>
    </article>)}
  </section>;
}
