import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface EngineSettings {
  executable: string;
  deno: string;
  ffmpeg: string;
  cookie_file: string;
  cookie_browser: string;
}
export interface MediaEntry {
  id: string;
  url: string;
  title: string;
  thumbnail: string;
  duration: number | null;
  index: number;
  available: boolean;
}
export interface MediaCollection {
  title: string;
  platform: string;
  entries: MediaEntry[];
  is_playlist: boolean;
  warnings: string;
}
export interface DownloadOptions {
  mode: 'video' | 'audio';
  height: number;
  container: string;
  video_codec: string;
  audio_codec: string;
  bitrate: number;
  accelerator: string;
  output_dir: string;
  subtitles: string;
  auto_subtitles: boolean;
  embed_subtitles: boolean;
  thumbnail: boolean;
  metadata: boolean;
  sponsorblock: boolean;
  section_start: number | null;
  section_end: number | null;
}
export interface ProviderTask {
  id: string;
  entry: MediaEntry;
  options: DownloadOptions;
  engine: EngineSettings;
  state: string;
  phase: string;
  progress: number;
  speed: number | null;
  eta: number | null;
  error: string;
  log: string[];
  output: string;
}
export function message(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object' && 'message' in error)
    return String(error.message);
  return String(error);
}
export const useYtdlpStore = defineStore('ytdlp', () => {
  const settings = ref<EngineSettings>({
    executable: '',
    deno: '',
    ffmpeg: '',
    cookie_file: '',
    cookie_browser: '',
  });
  const tasks = ref<Record<string, ProviderTask>>({});
  const loadError = ref('');
  let initialized: Promise<void> | undefined;
  async function initialize() {
    if (initialized) return initialized;
    initialized = (async () => {
      const pending = new Map<string, ProviderTask>();
      let loaded = false;
      const unlisten = await listen<ProviderTask>(
        'provider-task',
        ({ payload }) => {
          if (!loaded) pending.set(payload.id, payload);
          else tasks.value[payload.id] = payload;
        },
      );
      try {
        settings.value = await invoke<EngineSettings>('ytdlp_settings', {
          settings: null,
        });
        const list = await invoke<ProviderTask[]>('ytdlp_tasks');
        tasks.value = Object.fromEntries(list.map((t) => [t.id, t]));
        for (const [id, task] of pending) tasks.value[id] = task;
        loaded = true;
        loadError.value = '';
      } catch (error) {
        unlisten();
        loadError.value = message(error);
        throw error;
      }
    })();
    try {
      await initialized;
    } catch (error) {
      initialized = undefined;
      throw error;
    }
  }
  async function saveSettings() {
    settings.value = await invoke<EngineSettings>('ytdlp_settings', {
      settings: { ...settings.value },
    });
  }
  async function analyze(url: string, playlist: boolean) {
    await initialize();
    return invoke<MediaCollection>('ytdlp_analyze', {
      url,
      playlist,
      settings: { ...settings.value },
    });
  }
  async function submit(entries: MediaEntry[], options: DownloadOptions) {
    await initialize();
    const list = await invoke<ProviderTask[]>('ytdlp_submit', {
      entries,
      options,
      settings: { ...settings.value },
    });
    // Auto-download events can precede this response; don't replace a more
    // recent live state with the command's initial backlog snapshot.
    for (const task of list)
      if (!tasks.value[task.id]) tasks.value[task.id] = task;
    return list;
  }
  async function edit(id: string, options: DownloadOptions) {
    return invoke<ProviderTask>('ytdlp_edit', {
      id,
      options,
      settings: { ...settings.value },
    });
  }
  async function control(id: string, action: string) {
    await invoke('ytdlp_control', { id, action });
    if (action === 'remove') delete tasks.value[id];
  }
  return {
    settings,
    tasks,
    initialize,
    saveSettings,
    analyze,
    submit,
    control,
    edit,
    loadError,
  };
});
