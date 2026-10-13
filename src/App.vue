<template>
  <div id="main" @contextmenu.prevent="contextMenu?.init">
    <TitleBar />
    <WorkspaceHeader />
    <Toaster v-bind="toasterOptions" />
    <ContextMenu ref="contextMenu" />
    <div class="loading"></div>
    <router-view v-slot="{ Component }">
      <Transition mode="out-in">
        <keep-alive>
          <component :is="Component" ref="page" class="page" />
        </keep-alive>
      </Transition>
    </router-view>
    <ComponentsWrapper />
  </div>
</template>

<script setup lang="ts">
// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
import { ref, onMounted, watch, getCurrentInstance, reactive } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Toaster } from 'vue-sonner';
import {
  TitleBar,
  ContextMenu,
  ComponentsWrapper,
} from '@/components';

import WorkspaceHeader from '@/components/WorkspaceHeader.vue';
import { useYtdlpStore } from '@/services/ytdlp';

import { useAppStore, useQueueStore, useSettingsStore, useUserStore } from '@/store';
import { useComponentsStore, routeMap } from './store/components';
import router from './router';

import {
  toasterOptions,
  AppLog,
  parseId,
  setEventHook,
  acquireTransientBrowserCookie,
} from '@/services/utils';
import { fetchUser, activateCookies } from '@/services/login';
import * as clipboard from '@/services/clipboard';
import { commands } from '@/services/backend';
import { AppError } from '@/services/error';
import { processQueueTask } from '@/services/queue';
import { submitExtensionDownload } from '@/services/extensionDownload';

import { Task as TaskType } from './types/shared.d';
import i18n from './i18n';

const page = ref();
const contextMenu = ref<InstanceType<typeof ContextMenu>>();

const queues = useQueueStore();
const settings = useSettingsStore();
const components = useComponentsStore();
const app = useAppStore();
const user = useUserStore();
const context = getCurrentInstance()?.appContext;

type ExtensionRequest = {
  urls: string[];
  scope?: 'current' | 'all' | 'collection';
  browser_cookie?: string;
  request_id?: string;
  tab_id?: number;
};

type ExtensionConfigureRequest = {
  id: string;
  browser_cookie?: string;
};

const extensionInFlight = new Set<string>();

function extensionRequestKey(request: ExtensionRequest) {
  return (
    request.request_id ||
    `${request.tab_id ?? 'tab'}:${request.urls.join('|')}:${Date.now()}:${Math.random()}`
  );
}

async function processExtensionRequest(request: ExtensionRequest) {
  const key = extensionRequestKey(request);
  if (extensionInFlight.has(key)) return;
  extensionInFlight.add(key);

  const scope = request.scope === 'all' ? 'all' : 'current';
  const useBrowserCookie =
    !user.isLogin && Boolean(request.browser_cookie?.trim());

  let releaseBrowserCookie = () => {};
  try {
    if (useBrowserCookie) {
      releaseBrowserCookie = acquireTransientBrowserCookie(
        request.browser_cookie ?? null,
      );
    }

    // Request creation is independent per browser tab. queue.submit() remains
    // the single source of truth for auto-download and CONest's own scheduler
    // remains responsible for max_conc/download concurrency.
    for (const url of request.urls) {
      await submitExtensionDownload(url, scope);
    }
  } catch (e) {
    new AppError(e, {
      name: (request.tab_id === null || request.tab_id === undefined)
        ? 'ExtensionRequest'
        : `ExtensionRequest (tab ${request.tab_id})`,
    }).handle();
  } finally {
    releaseBrowserCookie();
    extensionInFlight.delete(key);
  }
}

async function pullExtensionRequests() {
  if (!app.inited) return;

  const pending = await invoke<ExtensionRequest[]>('take_extension_requests');
  for (const request of pending) {
    void processExtensionRequest(request);
  }
}

void listen('extension-request-pending', () => {
  void pullExtensionRequests();
});

void listen<string>('extension-start-task', async (event) => {
  if (!queues.backlog.includes(event.payload)) return;
  await processQueueTask(event.payload);
});

void listen<ExtensionConfigureRequest>('extension-configure-task', async (event) => {
  const task = queues.tasks[event.payload.id];
  if (!task || task.state !== 'backlog') return;

  const useBrowserCookie =
    !user.isLogin && Boolean(event.payload.browser_cookie?.trim());
  let releaseBrowserCookie = () => {};
  if (useBrowserCookie) {
    try {
      releaseBrowserCookie = acquireTransientBrowserCookie(
        event.payload.browser_cookie ?? null,
      );
    } catch (error) {
      new AppError(error, { name: 'ExtensionTaskSettings' }).handle();
      return;
    }
  }

  const appWindow = getCurrentWindow();
  const wasVisible = await appWindow.isVisible();
  document.body.classList.add('extension-settings-only');

  try {
    await appWindow.show();
    await appWindow.unminimize();
    await appWindow.setFocus();
    // Use the real CONest selector; only the selector is shown while the
    // extension edits a backlog task, so there is no second settings UI.
    const select = await components.c.selectPopup?.getSelect(task.item, task.select);
    if (!select) return;

    const updated = await invoke<{
      select: TaskType['select'];
      subtasks: TaskType['subtasks'];
      status: TaskType['status'];
    }>('update_extension_task_settings', {
      id: task.id,
      select,
    });

    task.select = updated.select;
    task.subtasks = updated.subtasks;
    task.status = updated.status;
  } catch (error) {
    new AppError(error, { name: 'ExtensionTaskSettings' }).handle();
  } finally {
    document.body.classList.remove('extension-settings-only');
    releaseBrowserCookie();
    if (!wasVisible) await appWindow.hide();
  }
});

if (!context) throw new Error('No AppContext');

watch(
  () => settings.isDark,
  (isDark) => {
    const props = context.config.globalProperties;
    const weight = isDark ? 'fa-solid' : 'fa-light';
    if (!props.$fa)
      props.$fa = reactive({
        weight,
        isDark,
      });
    props.$fa.weight = weight;
    props.$fa.isDark = isDark;
  },
  { immediate: true },
);

router.afterEach((to) =>
  components.regRoute(to.name as keyof typeof routeMap, page),
);

context.app.config.errorHandler = (e) =>
  new AppError(e, { name: 'AppError' }).handle();

document.addEventListener('contextmenu', (e) => {
  e.preventDefault();
});

clipboard.register(async (s) => {
  const text = s.trim();
  if (!text) return;

  // Generic URL monitoring for current and future providers.
  // Existing Bilibili IDs still work through parseId.
  const isUrl = /^https?:\/\/\S+$/i.test(text);
  if (!isUrl) {
    try {
      await parseId(text);
    } catch {
      return;
    }
  }

  try {
    const page = await components.navigate('searchPage');
    await page.search(text);
  } catch {
    // Unsupported providers are ignored until a provider handler is available.
  }
});

onMounted(async () => {
  router.push('/');
  setEventHook();

  const meta = await commands.meta();
  if (meta.status === 'error') throw new AppError(meta.error);
  const m = meta.data;

  const { version, hash } = m;
  app.$patch({
    version,
    hash,
  });
  const { config } = m;
  settings.$patch({
    ...config,
    // Migrate the original CONest Traditional Chinese locale id.
    language: config.language === 'zh-HK' ? 'zh-TW' : config.language,
  });
  const { tasks, schedulers, queue } = m;
  queues.$patch({
    tasks: Object.fromEntries(
      Object.entries(tasks).map(([id, t]) => [
        id,
        { ...t?.meta, ...t?.prepare, ...t?.hot } as TaskType,
      ]),
    ),
    schedulers,
    ...queue,
  });

  if (queue.doing?.length || queue.pending?.length) {
    AppLog(i18n.global.t('down.restored'), 'info');
  }

  const init = await commands.init();
  if (init.status === 'error') throw new AppError(init.error);

  // Startup must remain usable without a Bilibili login. A transient
  // Bilibili API block must not surface as an application-startup error.
  try {
    await fetchUser();
  } catch (error) {
    console.warn('User session lookup failed', error);
  }
  if (user.isLogin) {
    try {
      await activateCookies();
    } catch (error) {
      console.warn('Cookie activation failed', error);
    }
  }
  app.inited = true;
  // A yt-dlp-specific storage or tool failure must never block Bilibili startup.
  void useYtdlpStore().initialize().catch(error => console.warn('yt-dlp initialization failed', error));
  await invoke('set_extension_frontend_ready', { ready: true });
  await pullExtensionRequests();
});
</script>

<style>
@reference 'tailwindcss';

body.extension-settings-only #app > ul {
  display: none !important;
}
body.extension-settings-only #main > :not(.el) {
  visibility: hidden !important;
  pointer-events: none !important;
}
body.extension-settings-only #main > .el {
  visibility: visible !important;
  pointer-events: auto !important;
}

.loading {
  @apply absolute w-8 h-8 top-9.5 right-6 opacity-0 z-99 pointer-events-none transition-opacity;
  @apply border-solid border-2 border-(--solid-block-color) border-l-(--content-color) rounded-full;
  animation: circle infinite 0.75s linear;
  &.active {
    opacity: 1;
  }
}
@keyframes circle {
  0% {
    transform: rotate(0);
  }
  100% {
    transform: rotate(360deg);
  }
}
.v-enter-active {
  transition: opacity 0.3s ease;
}
.v-leave-active,
.v-enter-from,
.v-leave-to {
  opacity: 0;
}
.slide-enter-active,
.slide-leave-active {
  transition:
    translate 0.5s cubic-bezier(0, 1, 0.6, 1),
    opacity 0.3s;
}
.slide-enter-from,
.slide-leave-to {
  @apply translate-y-8 opacity-0;
}
</style>
