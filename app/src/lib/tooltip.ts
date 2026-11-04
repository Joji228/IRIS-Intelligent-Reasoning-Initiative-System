// `use:tip={'Text'}`: a small tooltip rendered on <body>, so it is never
// clipped by scrolling containers. Side via data-tip-side="right|left|bottom|top";
// a side tip that doesn't fit flips to the other side.

export function tip(node: HTMLElement, text: string) {
  let current = text;
  let el: HTMLDivElement | null = null;

  const show = () => {
    if (!current) return;
    el = document.createElement('div');
    el.className = 'iris-tip';
    el.textContent = current;
    // Longer explanations wrap into a small box instead of one long line.
    if (current.length > 60) el.classList.add('iris-tip-wrap');
    document.body.appendChild(el);
    const r = node.getBoundingClientRect();
    const width = el.getBoundingClientRect().width;
    let side = node.dataset.tipSide ?? 'right';
    // Not enough room on the asked side (e.g. a menu at the window's edge):
    // use the other one.
    if (side === 'left' && r.left - 10 - width < 6) side = 'right';
    else if (side === 'right' && r.right + 10 + width > window.innerWidth - 6 && r.left - 10 - width >= 6) side = 'left';
    if (side === 'right') {
      el.style.left = `${r.right + 10}px`;
      el.style.top = `${r.top + r.height / 2}px`;
      el.style.transform = 'translateY(-50%)';
    } else if (side === 'left') {
      el.style.left = `${r.left - 10}px`;
      el.style.top = `${r.top + r.height / 2}px`;
      el.style.transform = 'translate(-100%, -50%)';
    } else {
      el.style.left = `${r.left + r.width / 2}px`;
      el.style.top = side === 'top' ? `${r.top - 8}px` : `${r.bottom + 8}px`;
      el.style.transform = side === 'top' ? 'translate(-50%, -100%)' : 'translateX(-50%)';
    }
    // Keep it inside the window.
    const b = el.getBoundingClientRect();
    const dx = b.left < 6 ? 6 - b.left : b.right > window.innerWidth - 6 ? window.innerWidth - 6 - b.right : 0;
    if (dx) el.style.left = `${parseFloat(el.style.left) + dx}px`;
  };
  const hide = () => {
    el?.remove();
    el = null;
  };

  node.addEventListener('mouseenter', show);
  node.addEventListener('mouseleave', hide);
  node.addEventListener('mousedown', hide);
  return {
    update(next: string) {
      current = next;
      if (el && !next) hide();
      else if (el) el.textContent = next;
    },
    destroy() {
      hide();
      node.removeEventListener('mouseenter', show);
      node.removeEventListener('mouseleave', hide);
      node.removeEventListener('mousedown', hide);
    },
  };
}
