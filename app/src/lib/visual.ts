// Visual cards (Intelligent UI): a reply can carry a small interactive page as a
// ```visual block of HTML. The page runs in a sandboxed frame (public/visual.html),
// never in the app's own document.

import { app } from './app.svelte';
import { inDesktopApp, inWebLink } from './connection';

/** Context key: how a visual card sends its answer to the chat (set by the chat view). */
export const VISUAL_SEND = Symbol('visual-send');

export type Segment =
  | { kind: 'md'; text: string }
  | { kind: 'visual'; title: string; html: string; closed: boolean };

const fence = /^\s{0,3}(`{3,}|~{3,})\s*(.*)$/;

/** Splits a reply into Markdown and visual blocks. A visual whose closing fence hasn't
 *  streamed in yet is `closed: false`. Other code blocks stay in the Markdown. */
export function splitVisuals(text: string): Segment[] {
  if (!/^\s{0,3}(`{3,}|~{3,})\s*visual\b/im.test(text)) return [{ kind: 'md', text }];
  const out: Segment[] = [];
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  let md: string[] = [];
  const flush = () => {
    if (md.join('').trim()) out.push({ kind: 'md', text: md.join('\n') });
    md = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const m = fence.exec(lines[i]);
    if (!m) {
      md.push(lines[i]);
      continue;
    }
    const marker = m[1];
    const info = m[2].trim();
    const body: string[] = [];
    let j = i + 1;
    while (j < lines.length && !lines[j].trimStart().startsWith(marker)) body.push(lines[j++]);
    const closed = j < lines.length;
    if (/^visual\b/i.test(info)) {
      flush();
      out.push({ kind: 'visual', title: info.replace(/^visual\b/i, '').trim(), html: body.join('\n'), closed });
    } else {
      md.push(...lines.slice(i, closed ? j + 1 : j));
    }
    i = j;
  }
  flush();
  return out;
}

/** Where the frame is served: the engine for the desktop app (its own pages get the
 *  app's strict CSP, which would block the visual's scripts), the web app's origin
 *  otherwise (the relay sends it with its own CSP). */
export function frameUrl(): string {
  if (inDesktopApp && app.api) return `${app.api.base}/visual`;
  // The relay (Cloudflare) serves public/visual.html at /visual; the dev server doesn't.
  return inWebLink ? '/visual' : '/visual.html';
}

const THEME_VARS = [
  'bg', 'surface', 'elevated', 'hover', 'line', 'line-strong', 'text', 'muted', 'faint',
  'accent', 'accent-hi', 'accent-soft', 'success', 'danger', 'warn',
];

/** The app's current colours, for the visual's style kit. */
export function visualTheme(): Record<string, string> {
  const css = getComputedStyle(document.documentElement);
  const theme: Record<string, string> = {};
  for (const name of THEME_VARS) theme[name] = css.getPropertyValue(`--c-${name}`).trim();
  const bg = theme.bg.replace('#', '');
  const light = bg.length >= 6 && parseInt(bg.slice(0, 2), 16) > 0x80;
  theme.scheme = light ? 'light' : 'dark';
  return theme;
}
