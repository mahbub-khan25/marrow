// Link previews: hover a link in Preview (or Ctrl+hover one in Source) to
// read the note in a small card without leaving the one you're in.

import { h, icon } from "./util";

let card: HTMLElement | null = null;
let showTimer: number | undefined;
let hideTimer: number | undefined;
/** What the card (or the pending card) is for, so the same link isn't redone. */
let current = "";

export interface CardContent {
  title: string;
  icon: string;
  body: HTMLElement;
  open?: () => void;
}

/** Shows a card for `key` near `rect` after a short pause, unless the pointer moves on. */
export function hoverCard(key: string, rect: DOMRect, build: () => Promise<CardContent | null>, delay = 450) {
  window.clearTimeout(hideTimer);
  if (key === current) return;
  window.clearTimeout(showTimer);
  current = key;
  showTimer = window.setTimeout(async () => {
    const c = await build().catch(() => null);
    if (!c || current !== key) return;
    show(c, rect);
  }, delay);
}

/** Leaves a link: the card goes after a moment, unless the pointer moves onto it. */
export function leaveCard() {
  window.clearTimeout(showTimer);
  window.clearTimeout(hideTimer);
  if (!card) {
    current = "";
    return;
  }
  hideTimer = window.setTimeout(hideCard, 300);
}

export function hideCard() {
  window.clearTimeout(showTimer);
  window.clearTimeout(hideTimer);
  current = "";
  card?.remove();
  card = null;
}

function show(c: CardContent, rect: DOMRect) {
  card?.remove();
  const el = h(
    "div.hover-card",
    { role: "dialog", "aria-label": `Preview of ${c.title}` },
    h(
      "div.hover-head",
      null,
      icon(c.icon),
      h("span.hover-title", null, c.title),
      c.open
        ? h(
            "button.btn.btn-secondary.hover-open",
            {
              onclick: () => {
                hideCard();
                c.open!();
              },
            },
            "Open",
          )
        : null,
    ),
    h("div.hover-body.preview", null, c.body),
  );
  el.addEventListener("mouseenter", () => window.clearTimeout(hideTimer));
  el.addEventListener("mouseleave", leaveCard);
  document.body.append(el);
  card = el;
  const w = el.offsetWidth;
  const ht = el.offsetHeight;
  const left = Math.max(12, Math.min(rect.left, window.innerWidth - w - 12));
  const below = rect.bottom + 8;
  const top = below + ht <= window.innerHeight - 12 ? below : Math.max(12, rect.top - ht - 8);
  el.style.left = `${left}px`;
  el.style.top = `${top}px`;
}

// Scrolling, typing or clicking elsewhere puts the card away.
document.addEventListener("keydown", (e) => {
  if (card && (e.key === "Escape" || !(e.ctrlKey || e.metaKey))) hideCard();
});
document.addEventListener(
  "mousedown",
  (e) => {
    if (card && !card.contains(e.target as Node)) hideCard();
  },
  true,
);
document.addEventListener(
  "wheel",
  (e) => {
    if (card && !card.contains(e.target as Node)) hideCard();
  },
  { passive: true, capture: true },
);
