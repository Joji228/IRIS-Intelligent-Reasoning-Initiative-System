// Stops every looping animation while nobody is looking.
//
// The orbs float, blink and look around forever. On a machine that draws the
// window in software (the server's old graphics driver, say) that kept the
// window redrawing 60 times a second around the clock, using nearly two CPU
// threads with no one watching. So: when the window is hidden, or nobody has
// touched it for a while, `<html data-still>` pauses all CSS animation (see
// app.css), and anything that loops each frame checks `isStill()`. Any input
// brings it back at once.

const IDLE_MS = 2 * 60 * 1000;

let still = false;
let timer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<(still: boolean) => void>();

function set(next: boolean) {
  if (next === still) return;
  still = next;
  if (still) document.documentElement.dataset.still = '';
  else delete document.documentElement.dataset.still;
  for (const fn of listeners) fn(still);
}

function wake() {
  clearTimeout(timer);
  if (document.hidden) return set(true);
  set(false);
  timer = setTimeout(() => set(true), IDLE_MS);
}

export function isStill() {
  return still;
}

/** Calls `fn` whenever the window goes still or wakes; returns the unsubscribe. */
export function onStill(fn: (still: boolean) => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function installStill() {
  for (const type of ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart', 'focus']) {
    window.addEventListener(type, wake, { passive: true, capture: true });
  }
  document.addEventListener('visibilitychange', wake);
  wake();
}
