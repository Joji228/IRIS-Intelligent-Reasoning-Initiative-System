// What an agent's orb shows: its state (what it's up to), the faces it
// cycles through in that state, and what its speech bubble says.

import type { Mood } from './types';

export type OrbState =
  | 'idle'
  | 'sleeping'
  | 'thinking'
  | 'searching'
  | 'reading'
  | 'working'
  | 'team'
  | 'reviewing'
  | 'waiting'
  | 'designing'
  | 'celebrate'
  | 'failed'
  | 'unread'
  | 'spawn'
  | 'spin';

/** One face: each eye's shape and place, the brows, and the eye style. */
export interface Face {
  /** Eye style: open pills, smiling arcs (^ ^) or closed lines. */
  eyes: 'open' | 'happy' | 'closed';
  /** Left and right eye: [scaleX, scaleY, rotate°, dx, dy]. */
  l: [number, number, number, number, number];
  r: [number, number, number, number, number];
  /** Brows: none, or [left rotate°, right rotate°, lift] (lift < 0 raises them). */
  brows?: [number, number, number];
}

const eye = (sx = 1, sy = 1, rot = 0, dx = 0, dy = 0): Face['l'] => [sx, sy, rot, dx, dy];

export const FACES = {
  neutral: { eyes: 'open', l: eye(), r: eye() },
  soft: { eyes: 'open', l: eye(1, 0.86, 0, 0, 0.4), r: eye(1, 0.86, 0, 0, 0.4) },
  glanceLeft: { eyes: 'open', l: eye(1, 1, 0, -2, 0), r: eye(0.94, 1, 0, -2, 0) },
  glanceRight: { eyes: 'open', l: eye(0.94, 1, 0, 2, 0), r: eye(1, 1, 0, 2, 0) },
  curious: { eyes: 'open', l: eye(1.08, 1.1, 0, 0.6, -0.4), r: eye(0.96, 0.78, 0, 0.6, 0.4), brows: [-6, 10, -1.4] },
  wide: { eyes: 'open', l: eye(1.12, 1.12), r: eye(1.12, 1.12), brows: [-8, 8, -2.2] },
  pondering: { eyes: 'open', l: eye(0.95, 0.9, 0, 1.8, -2.4), r: eye(0.95, 0.9, 0, 1.8, -2.4), brows: [-10, 0, -1.6] },
  ponderLeft: { eyes: 'open', l: eye(0.95, 0.9, 0, -1.8, -2.2), r: eye(0.95, 0.9, 0, -1.8, -2.2), brows: [0, 10, -1.6] },
  focused: { eyes: 'open', l: eye(1, 0.8, 0, 0, 0.4), r: eye(1, 0.8, 0, 0, 0.4), brows: [14, -14, 0.6] },
  determined: { eyes: 'open', l: eye(1.04, 0.66, -6, 0, 0.8), r: eye(1.04, 0.66, 6, 0, 0.8), brows: [20, -20, 1.2] },
  readingL: { eyes: 'open', l: eye(1, 0.72, 0, -1.6, 1.2), r: eye(1, 0.72, 0, -1.6, 1.2), brows: [6, -6, 0.6] },
  readingR: { eyes: 'open', l: eye(1, 0.72, 0, 1.6, 1.2), r: eye(1, 0.72, 0, 1.6, 1.2), brows: [6, -6, 0.6] },
  suspicious: { eyes: 'open', l: eye(1.04, 0.42, 0, 0.8, 0.6), r: eye(1.04, 0.5, 0, 0.8, 0.6), brows: [4, -12, 0.8] },
  lookUp: { eyes: 'open', l: eye(1, 1, 0, 0.6, -2.2), r: eye(1, 1, 0, 0.6, -2.2), brows: [-6, 6, -1.6] },
  worried: { eyes: 'open', l: eye(0.92, 0.9, 0, 0, -0.6), r: eye(0.92, 0.9, 0, 0, -0.6), brows: [-16, 16, -1.2] },
  happy: { eyes: 'happy', l: eye(), r: eye() },
  proud: { eyes: 'happy', l: eye(1.06, 1, -6, 0, -0.6), r: eye(1.06, 1, 6, 0, -0.6), brows: [-8, 8, -1.6] },
  wink: { eyes: 'open', l: eye(1.06, 1.06), r: eye(1, 0.14, 0, 0, 0.8), brows: [-6, 6, -1.2] },
  sad: { eyes: 'open', l: eye(1, 0.6, 14, 0, 1.4), r: eye(1, 0.6, -14, 0, 1.4), brows: [-18, 18, -0.6] },
  drowsy: { eyes: 'open', l: eye(1, 0.36, 0, 0, 1.4), r: eye(1, 0.36, 0, 0, 1.4) },
  asleep: { eyes: 'closed', l: eye(), r: eye() },
} satisfies Record<string, Face>;

export type FaceName = keyof typeof FACES;

/** Faces each state cycles through (the first is shown first). */
export const POOLS: Record<OrbState, FaceName[]> = {
  idle: ['neutral', 'soft', 'glanceLeft', 'neutral', 'glanceRight', 'curious'],
  sleeping: ['asleep'],
  thinking: ['pondering', 'ponderLeft', 'soft', 'pondering'],
  searching: ['curious', 'glanceLeft', 'wide', 'glanceRight', 'focused'],
  reading: ['readingL', 'readingR'],
  working: ['focused', 'determined', 'focused', 'glanceRight'],
  team: ['neutral', 'happy', 'glanceLeft'],
  reviewing: ['suspicious', 'focused'],
  waiting: ['lookUp', 'worried', 'lookUp', 'soft'],
  designing: ['curious', 'wide', 'happy', 'pondering'],
  celebrate: ['happy', 'proud', 'wink'],
  failed: ['sad', 'worried'],
  unread: ['wide', 'happy', 'curious'],
  spawn: ['wide', 'happy'],
  spin: ['happy'],
};

/** How long each face holds in a state (ms, before a little random jitter). */
export const HOLD: Partial<Record<OrbState, number>> = {
  idle: 3600,
  reading: 900,
  searching: 1300,
  working: 1900,
  thinking: 2400,
  celebrate: 900,
  waiting: 2600,
};

const SEARCH = ['web_search', 'web_fetch', 'search_chats', 'code_search', 'search_apps', 'browser_open', 'browser_click', 'browser_type', 'browser_scroll', 'browser_back', 'browser_login'];
const READ = ['read_file', 'list_files', 'code_read', 'code_diff', 'browser_read', 'view_image', 'use_skill', 'get_settings', 'list_routines', 'list_accounts', 'list_models', 'get_time', 'system_status', 'code_status'];
const TEAM = ['list_agents', 'create_agent', 'delegate_task', 'check_task', 'dismiss_agent', 'keep_agent', 'request_tools'];

/** What the orb is up to, from the agent's run, mood and unread replies. */
export function orbState(opts: {
  activity?: string;
  mood?: Mood;
  working?: boolean;
  waiting?: boolean;
  unread?: boolean;
  sleepy?: boolean;
}): OrbState {
  const { activity, mood, working, waiting, unread, sleepy } = opts;
  if (mood === 'spawn') return 'spawn';
  if (mood === 'spin') return 'spin';
  if (mood === 'happy') return 'celebrate';
  if (mood === 'sad') return 'failed';
  if (waiting || activity === 'waiting') return 'waiting';
  if (activity || working) {
    const a = activity ?? 'thinking';
    if (a === 'thinking' || a === 'compacting') return 'thinking';
    if (a === 'reviewing') return 'reviewing';
    if (a === 'designing') return 'designing';
    if (TEAM.includes(a)) return 'team';
    if (SEARCH.includes(a) || a.startsWith('browser_')) return 'searching';
    if (READ.includes(a)) return 'reading';
    return 'working';
  }
  if (unread) return 'unread';
  if (sleepy) return 'sleeping';
  return 'idle';
}

/** Plain words for the speech bubble, a few to rotate through on long steps. */
export function phrases(state: OrbState, activity?: string): string[] {
  const a = activity ?? '';
  const by: Record<string, string[]> = {
    web_search: ['Searching the web…', 'Looking it up…', 'Going through results…'],
    web_fetch: ['Reading a page…', 'Skimming the page…'],
    search_chats: ['Searching past chats…', 'Jogging my memory…'],
    read_file: ['Reading files…', 'Looking it over…'],
    list_files: ['Looking at files…'],
    write_file: ['Writing it down…', 'Saving a file…'],
    run_command: ['Running a command…', 'Checking the output…'],
    view_image: ['Looking at the picture…'],
    memory: ['Making a note…'],
    get_time: ['Checking the time…'],
    system_status: ['Checking the server…', 'Taking its pulse…'],
    app_updates: ['Checking for updates…'],
    search_apps: ['Looking for the app…'],
    update_apps: ['Updating apps…'],
    install_app: ['Installing an app…'],
    uninstall_app: ['Removing an app…'],
    windows_updates: ['Checking Windows Update…'],
    power: ['Getting ready to restart…'],
    list_agents: ['Checking the team…'],
    create_agent: ['Bringing in a new agent…'],
    delegate_task: ['Handing out a task…', 'Passing it along…'],
    check_task: ['Checking on the team…'],
    dismiss_agent: ['Wrapping up with an agent…'],
    keep_agent: ['Keeping an agent on…'],
    request_tools: ['Asking Iris for tools…'],
    create_routine: ['Scheduling a routine…'],
    list_routines: ['Checking the routines…'],
    delete_routine: ['Removing a routine…'],
    get_settings: ['Checking the settings…'],
    change_setting: ['Changing a setting…'],
    list_models: ['Checking the models…'],
    switch_model: ['Switching models…'],
    use_skill: ['Reading a how-to…'],
    save_skill: ['Saving a how-to…'],
    list_accounts: ['Checking saved accounts…'],
    browser_open: ['Opening a page…', 'Browsing…'],
    browser_read: ['Reading the page…'],
    browser_click: ['Clicking through…'],
    browser_type: ['Typing…'],
    browser_scroll: ['Scrolling…'],
    browser_login: ['Signing in…'],
    code_status: ['Checking the code…'],
    code_search: ['Searching the code…'],
    code_read: ['Reading the code…'],
    code_edit: ['Editing the code…', 'Making changes…'],
    code_write: ['Writing code…'],
    code_diff: ['Looking at the changes…'],
    code_apply: ['Testing the change…'],
    code_install: ['Building the new version…'],
    code_verify: ['Checking the new version…'],
    compacting: ['Tidying up the chat…'],
    designing: ['Getting to know my job…', 'Setting myself up…'],
    reviewing: ['Double-checking that command…'],
    ask_user: ['Asking you something…'],
  };
  switch (state) {
    case 'waiting':
      return ['Waiting for you…', 'Need your OK…'];
    case 'celebrate':
      return ['Done!', 'All done!'];
    case 'failed':
      return ['Hit a snag…'];
    case 'unread':
      return ['New reply!'];
    case 'spawn':
      return ['Hi!'];
    case 'sleeping':
      return ['Zzz…'];
    case 'idle':
      return [];
    case 'thinking':
      if (a === 'compacting') return by.compacting;
      return ['Thinking…', 'Mulling it over…', 'Piecing it together…', 'Working it out…'];
  }
  if (by[a]) return by[a];
  if (a.startsWith('mcp__')) return [`Using ${a.slice(5).split('__')[0]}…`];
  return ['Working on it…', 'Chipping away…'];
}
