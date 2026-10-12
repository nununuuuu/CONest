import { readText } from '@tauri-apps/plugin-clipboard-manager';
import { strip } from './utils';
import { useSettingsStore } from '@/store';

let lastText = '';
let initialized = false;
let running = false;

const callbacks = new Set<(s: string) => void>();

export function register(cb: (s: string) => void) {
  callbacks.add(cb);
  ensureLoop();
  return () => callbacks.delete(cb);
}

function ensureLoop() {
  if (running) return;
  running = true;
  tick();
}

function schedule() {
  if (!running) return;
  window.setTimeout(tick, 700);
}

async function tick() {
  try {
    const settings = useSettingsStore();

    if (!settings.clipboard) {
      // Reset the baseline while disabled. When enabled again, the current
      // clipboard becomes the baseline and only later changes are emitted.
      initialized = false;
      lastText = '';
      return;
    }

    const raw = await readText();
    const text = strip(raw ?? '').trim();

    if (!initialized) {
      lastText = text;
      initialized = true;
      return;
    }

    if (text === lastText) return;

    lastText = text;
    for (const cb of callbacks) {
      try {
        void cb(text);
      } catch {
        // One consumer must not stop clipboard monitoring.
      }
    }
  } catch (error) {
    console.warn('Clipboard monitor read failed', error);
  } finally {
    schedule();
  }
}
