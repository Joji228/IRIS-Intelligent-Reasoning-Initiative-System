// Press feedback for buttons.
//
// `installPressFlash()`: every button flashes briefly when clicked. On touch screens
// `:active` lasts only as long as the finger, so a quick tap showed nothing.
//
// `use:busy={handler}`: for a button whose handler waits on the server. While it
// runs, the button shows a spinner and ignores more clicks.

export function installPressFlash() {
  document.addEventListener(
    'click',
    (e) => {
      const button = (e.target as Element | null)?.closest?.('button, [role="button"]') as HTMLElement | null;
      if (!button || (button as HTMLButtonElement).disabled) return;
      button.classList.remove('is-pressed');
      // Restart the animation even on a second quick click.
      void button.offsetWidth;
      button.classList.add('is-pressed');
      setTimeout(() => button.classList.remove('is-pressed'), 260);
    },
    { capture: true, passive: true },
  );
}

export function busy(node: HTMLElement, handler: () => unknown) {
  let run = handler;
  async function onclick() {
    if (node.getAttribute('aria-busy') === 'true') return;
    node.setAttribute('aria-busy', 'true');
    try {
      await run();
    } finally {
      node.removeAttribute('aria-busy');
    }
  }
  node.addEventListener('click', onclick);
  return {
    update(next: () => unknown) {
      run = next;
    },
    destroy() {
      node.removeEventListener('click', onclick);
    },
  };
}
