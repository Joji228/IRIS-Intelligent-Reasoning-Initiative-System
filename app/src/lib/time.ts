// Human-friendly times for lists and headers.

const DAY = 86_400_000;

function startOfDay(ms: number) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** "just now", "5m", "3h", "Yesterday", "Mon", "12 Mar" */
export function relativeTime(ms: number, now = Date.now()): string {
  const diff = now - ms;
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  const today = startOfDay(now);
  if (ms >= today) return `${Math.floor(diff / 3_600_000)}h ago`;
  if (ms >= today - DAY) return 'Yesterday';
  if (ms >= today - 6 * DAY) return new Date(ms).toLocaleDateString(undefined, { weekday: 'short' });
  return new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** A chat's day divider: "Today", "Yesterday", "Monday", "Mon 2 Nov", "2 Nov 2025". */
export function dayLabel(ms: number, now = Date.now()): string {
  const today = startOfDay(now);
  if (ms >= today) return 'Today';
  if (ms >= today - DAY) return 'Yesterday';
  if (ms >= today - 6 * DAY) return new Date(ms).toLocaleDateString(undefined, { weekday: 'long' });
  const sameYear = new Date(ms).getFullYear() === new Date(now).getFullYear();
  return new Date(ms).toLocaleDateString(undefined, sameYear
    ? { weekday: 'short', day: 'numeric', month: 'short' }
    : { day: 'numeric', month: 'short', year: 'numeric' });
}

/** A chat list's time: "14:05" today, then "Yesterday", "Mon", "2 Nov". */
export function listTime(ms: number, now = Date.now()): string {
  const today = startOfDay(now);
  if (ms >= today) return clockTime(ms);
  if (ms >= today - DAY) return 'Yesterday';
  if (ms >= today - 6 * DAY) return new Date(ms).toLocaleDateString(undefined, { weekday: 'short' });
  return new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** True when two times fall on different days. */
export function newDay(a: number, b: number): boolean {
  return startOfDay(a) !== startOfDay(b);
}

/** Section label for grouping threads by recency. */
export function dayGroup(ms: number, now = Date.now()): string {
  const today = startOfDay(now);
  if (ms >= today) return 'Today';
  if (ms >= today - DAY) return 'Yesterday';
  if (ms >= today - 7 * DAY) return 'Previous 7 days';
  if (ms >= today - 30 * DAY) return 'Previous 30 days';
  return 'Older';
}

export function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 5) return 'Working late';
  if (h < 12) return 'Good morning';
  if (h < 18) return 'Good afternoon';
  return 'Good evening';
}

export function clockTime(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}
