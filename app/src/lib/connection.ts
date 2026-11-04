// Where to find the engine and which token to use.
//
// 1. Inside the desktop app: ask the Tauri shell (it reads the engine's
//    local-token.txt on this machine).
// 2. UI dev server: VITE_IRIS_URL + VITE_IRIS_TOKEN from app/.env.local
//    (blanked in builds by vite.config.ts, so only `npm run dev` sees them).
// 3. Anything else: a connection the user entered, kept in localStorage.

import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Connection } from './api';

const KEY = 'iris.connection';

export const inDesktopApp = isTauri();

/** The web app served by the relay: it reaches the engine through I.R.I.S. Link. */
export const inWebLink = !inDesktopApp && import.meta.env.VITE_IRIS_LINK === '1';

export async function resolveConnection(): Promise<Connection | null> {
  if (inDesktopApp) {
    try {
      return await invoke<Connection>('connection');
    } catch (e) {
      console.warn('connection from shell failed', e);
    }
  }
  const url = import.meta.env.VITE_IRIS_URL as string | undefined;
  const token = import.meta.env.VITE_IRIS_TOKEN as string | undefined;
  if (url && token) return { url, token };
  try {
    const saved = localStorage.getItem(KEY);
    if (saved) return JSON.parse(saved) as Connection;
  } catch {
    /* storage unavailable */
  }
  return null;
}

export function saveConnection(conn: Connection) {
  try {
    localStorage.setItem(KEY, JSON.stringify(conn));
  } catch {
    /* storage unavailable */
  }
}
