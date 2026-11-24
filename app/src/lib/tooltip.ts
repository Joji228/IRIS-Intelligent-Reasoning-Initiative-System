// `use:tip={'Text'}`: a small hover bubble. One bubble is shared by the whole
// app and lives on <body>, so it's never clipped by scrolling containers and
// can glide from one button to the next instead of flickering.
//
// - It appears after a short pause (HOVER_DELAY); once one is showing, moving
//   to another button switches right away (it "stays warm" for WARM_FOR).
// - Side: data-tip-side="right|left|bottom|top", else chosen from where the
//   button is (the left rail → right, near the bottom → top, else below).
//   A side without room flips to the opposite one; it always stays on screen.
// - It hides on click, key press, scroll, the window losing focus, and isn't
//   shown for touch (a tap shouldn't leave a bubble behind).

type Side = 'top' | 'bottom' | 'left' | 'right';

const HOVER_DELAY = 380;
const WARM_FOR = 450;
const GAP = 8;
const EDGE = 6;

let bubble: HTMLDivElement | null = null;
let label: HTMLSpanElement | null = null;
let owner: HTMLElement | null = null;
let showTimer: ReturnType<typeof setTimeout> | undefined;
let hideTimer: ReturnType<typeof setTimeout> | undefined;
let warmUntil = 0;
let listening = false;

function ensureBubble() {
  if (bubble) return;
  bubble = document.createElement('div');
  bubble.className = 'iris-tip';
  bubble.setAttribute('role', 'tooltip');
  label = document.createElement('span');
  bubble.appendChild(label);
  document.body.appendChild(bubble);
}

function onGlobalDismiss() {
  hideNow();
}

function listen(on: boolean) {
  if (on === listening) return;
  listening = on;
  const opts = { capture: true, passive: true } as const;
  const fn = on ? window.addEventListener : window.removeEventListener;
  fn('scroll', onGlobalDismiss, opts);
  fn('wheel', onGlobalDismiss, opts);
  fn('keydown', onGlobalDismiss, opts);
  fn('blur', onGlobalDismiss);
  fn('resize', onGlobalDismiss);
}

function pickSide(node: HTMLElement, r: DOMRect): Side {
  const asked = node.dataset.tipSide as Side | undefined;
  if (asked) return asked;
  if (r.right < 90) return 'right';
  if (r.bottom > window.innerHeight - 90) return 'top';
  return 'bottom';
}

function place(node: HTMLElement, text: string) {
  ensureBubble();
  const b = bubble!;
  label!.textContent = text;
  b.classList.toggle('iris-tip-wrap', text.length > 60);
  const r = node.getBoundingClientRect();
  // Layout size, unaffected by the entrance's scale.
  const w = b.offsetWidth;
  const h = b.offsetHeight;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  let side = pickSide(node, r);
  const fits = (s: Side) =>
    s === 'right' ? r.right + GAP + w <= vw - EDGE
    : s === 'left' ? r.left - GAP - w >= EDGE
    : s === 'top' ? r.top - GAP - h >= EDGE
    : r.bottom + GAP + h <= vh - EDGE;
  const opposite: Record<Side, Side> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };
  if (!fits(side) && fits(opposite[side])) side = opposite[side];

  let x: number;
  let y: number;
  if (side === 'right' || side === 'left') {
    x = side === 'right' ? r.right + GAP : r.left - GAP - w;
    y = r.top + r.height / 2 - h / 2;
  } else {
    x = r.left + r.width / 2 - w / 2;
    y = side === 'bottom' ? r.bottom + GAP : r.top - GAP - h;
  }
  // Keep it on screen; the arrow still points at the button's middle.
  const cx = Math.min(vw - EDGE - w, Math.max(EDGE, x));
  const cy = Math.min(vh - EDGE - h, Math.max(EDGE, y));
  b.style.left = `${Math.round(cx)}px`;
  b.style.top = `${Math.round(cy)}px`;
  b.dataset.side = side;
  const arrowX = r.left + r.width / 2 - cx;
  const arrowY = r.top + r.height / 2 - cy;
  b.style.setProperty('--arrow-x', `${Math.min(w - 10, Math.max(10, arrowX))}px`);
  b.style.setProperty('--arrow-y', `${Math.min(h - 8, Math.max(8, arrowY))}px`);
}

function showFor(node: HTMLElement, text: string) {
  clearTimeout(hideTimer);
  const wasVisible = !!bubble?.classList.contains('show');
  // Gliding from one button to the next only when it was already showing.
  bubble?.classList.toggle('glide', wasVisible);
  place(node, text);
  owner = node;
  // Restart the entrance only when it wasn't showing.
  if (!wasVisible) {
    bubble!.classList.remove('show');
    void bubble!.offsetWidth;
  }
  bubble!.classList.add('show');
  listen(true);
}

function hideNow() {
  clearTimeout(showTimer);
  clearTimeout(hideTimer);
  if (bubble?.classList.contains('show')) warmUntil = Date.now() + WARM_FOR;
  bubble?.classList.remove('show', 'glide');
  owner = null;
  listen(false);
}

export function tip(node: HTMLElement, text: string) {
  let current = text;

  const enter = (e: PointerEvent) => {
    if (e.pointerType === 'touch' || !current) return;
    clearTimeout(showTimer);
    clearTimeout(hideTimer);
    const warm = Date.now() < warmUntil || !!bubble?.classList.contains('show');
    if (warm) showFor(node, current);
    else showTimer = setTimeout(() => node.isConnected && showFor(node, current), HOVER_DELAY);
  };
  const leave = () => {
    clearTimeout(showTimer);
    if (owner !== node) return;
    // A moment's grace, so moving to the next button glides instead of blinking.
    hideTimer = setTimeout(hideNow, 90);
  };
  const press = () => hideNow();

  node.addEventListener('pointerenter', enter);
  node.addEventListener('pointerleave', leave);
  node.addEventListener('pointerdown', press);
  return {
    update(next: string) {
      current = next;
      if (owner === node) {
        if (!next) hideNow();
        else place(node, next);
      }
    },
    destroy() {
      if (owner === node) hideNow();
      clearTimeout(showTimer);
      node.removeEventListener('pointerenter', enter);
      node.removeEventListener('pointerleave', leave);
      node.removeEventListener('pointerdown', press);
    },
  };
}
