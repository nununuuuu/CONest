<template>
  <div class="workspace-page">
    <div class="workspace-heading">
      <div>
        <h1>匯入與選取</h1>
        <p class="desc">匯入連結、選取項目，再交給下載佇列。</p>
      </div>
      <div class="workspace-heading-actions">
        <button @click="searchYouTube">YouTube 搜尋</button>
        <button @click="batchVisible = !batchVisible">批次貼上</button>
      </div>
    </div>
    <div class="workspace-import">
      <input
        v-model="v.searchInput"
        class="w-full rounded-2xl"
        type="text"
        spellcheck="false"
        placeholder="貼上網址或 Bilibili 識別碼"
        @keydown.enter="search()"
      />
      <Dropdown
        v-model="v.mediaType"
        :drop="[
          { id: 'auto', name: $t('search.autoDetect') },
          ...Object.values(Types.MediaType).map((id) => ({
            id,
            name: $t('mediaType.' + id),
          })),
        ]"
      />
      <button class="workspace-primary" @click="search()">解析連結</button>
    </div>
    <div v-if="batchVisible" class="workspace-batch">
      <textarea
        v-model="batchInput"
        placeholder="每行一個網址；Bilibili 識別碼仍可使用"
        aria-label="批次網址"
      ></textarea
      ><button :disabled="batchBusy" @click="parseBatch">
        {{ batchBusy ? '處理中…' : '解析下一筆' }}</button
      ><span class="desc">{{ batchRemaining }} 筆待解析</span>
    </div>
    <YtdlpWorkbench
      v-if="provider === 'ytdlp'"
      :input="providerInput"
      :generation="providerGeneration"
    />
    <div v-else class="workspace-columns native-workbench">
      <section class="workspace-content native-content" aria-label="Bilibili 內容清單">
        <div class="workspace-panel-heading">
          <h2>內容清單</h2>
          <span class="desc">{{ v.listActive ? v.mediaInfo.list.length : 0 }} 項 · 已選 {{ v.checkboxs.length }} 項</span>
        </div>
        <MediaInfo v-if="v.listActive" :info="v.mediaInfo" />
        <div class="native-list-frame">
          <div v-if="v.searching" class="workspace-empty" role="status">
            <span class="workspace-spinner" aria-hidden="true"></span>
            <p>正在解析內容…</p>
          </div>
          <Empty v-else-if="!v.listActive" :text="$t('search.suggest')" />
          <Empty v-else-if="!v.mediaInfo.list.length" :text="$t('empty')" />
          <MediaList
            v-else
            ref="mediaList"
            v-model="v.checkboxs"
            :list="v.mediaInfo.list"
            :edge="v.mediaInfo.edge"
            :update-edge="updateEdge"
          />
        </div>
      </section>
      <aside class="workspace-inspector bili-inspector" aria-label="Bilibili 輸出設定">
        <h3>輸出設定</h3>
        <p class="desc">選取項目後開啟完整設定，調整畫質、音軌、編碼、彈幕、字幕、封面與附加內容。</p>
        <button
          v-for="(i, k) in buttons"
          :key="k"
          :disabled="!v.listActive || v.searching || !v.mediaInfo.list.length || (k === 0 && !v.checkboxs.length)"
          @click="i.action"
        >
          <i :class="[$fa.weight, i.icon]"></i>
          <span>{{ $t(i.text) }}</span>
        </button>
        <template v-if="v.mediaInfo.pn">
          <label for="bili-page">{{ $t('page') }}</label>
          <input id="bili-page" v-model="v.pageIndex" type="number" min="1" @input="handlePage" />
        </template>
        <div v-if="v.mediaInfo.sections?.tabs?.length" class="tab">
          <button
            v-for="(t, k) in v.mediaInfo.sections.tabs"
            :key="k"
            class="w-full!"
            :class="{ active: v.tab === t.id }"
            @click="updateTab(t.id)"
          >
            <span class="truncate">{{ t.name }}</span>
            <label class="primary-color"></label>
          </button>
        </div>
      </aside>
    </div>
    <div class="workspace-dock">
      <span class="desc"
        >{{ activeTaskCount }} 項進行中 · 原生 Bilibili
        與多平台任務共用下載位置</span
      ><button @click="router.push('/down-page')">查看任務 →</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { MediaInfo, MediaList } from '@/components/SearchPage';
import { Dropdown, Empty } from '@/components';

import { nextTick, onActivated, reactive, ref, watch, computed } from 'vue';
import YtdlpWorkbench from '@/components/YtdlpWorkbench.vue';
import { chooseProvider, type ProviderId } from '@/services/providers';
import { useYtdlpStore } from '@/services/ytdlp';
import { useQueueStore } from '@/store';
import router from '@/router';
import * as log from '@tauri-apps/plugin-log';
import pLimit from 'p-limit';

import { useSettingsStore, useComponentsStore } from '@/store';
import { AppLog, parseId, strip } from '@/services/utils';
import { data, extras } from '@/services/media';
import { save } from '@tauri-apps/plugin-dialog';
import { commands } from '@/services/backend';
import * as queue from '@/services/queue';
import { submitExtensionDownload } from '@/services/extensionDownload';
import * as Types from '@/types/shared.d';
import i18n from '@/i18n';

const provider = ref<ProviderId>('bilibili');
const providerInput = ref('');
const providerGeneration = ref(0);
const batchVisible = ref(false);
const batchInput = ref('');
const batchBusy = ref(false);
const batchRemaining = computed(
  () => batchInput.value.split(/\r?\n/).filter((s) => s.trim()).length,
);
const ytTasks = useYtdlpStore();
const biliTasks = useQueueStore();
const activeTaskCount = computed(
  () =>
    Object.values(biliTasks.tasks).filter((t) => t.state === 'active').length +
    Object.values(ytTasks.tasks).filter((t) =>
      ['running', 'processing'].includes(t.state),
    ).length,
);
async function parseBatch() {
  const lines = batchInput.value
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
  if (!lines.length) return;
  batchBusy.value = true;
  try {
    await search(lines[0]);
    batchInput.value = lines.slice(1).join('\n');
  } finally {
    batchBusy.value = false;
  }
}
async function searchYouTube() {
  const query = v.searchInput.trim();
  if (!query) return;
  await search(`ytsearch20:${query}`);
}

const v = reactive({
  anim: {} as Animation,
  mediaInfo: {} as Types.MediaInfo,
  mediaType: 'auto' as Types.MediaType | 'auto',
  checkboxs: [] as number[],
  searchInput: String(),
  listActive: false,
  searching: false,
  pageIndex: 1,
  tab: -1,
  offsetMap: new Map<number, string>(),
});

const buttons = [
  {
    icon: 'fa-cloud-arrow-down',
    text: 'search.general',
    action: () => initPopup(),
  },
  {
    icon: 'fa-file-export',
    text: 'search.export',
    action: () => exportData(),
  },
  {
    icon: 'fa-square-dashed-circle-plus',
    text: 'search.selectAll',
    action: () =>
      (v.checkboxs =
        v.checkboxs.length === v.mediaInfo.list.length
          ? []
          : [...Array(v.mediaInfo.list.length).keys()]),
  },
];

const mediaList = ref<InstanceType<typeof MediaList>>();

const settings = useSettingsStore();
const components = useComponentsStore();

function updateIndex() {
  const raw = v.mediaInfo.list.findIndex((v) => v.isTarget);
  const target = raw >= 0 ? raw : (v.mediaInfo.list[0]?.index ?? 0);
  v.checkboxs = [target];
  requestAnimationFrame(() => {
    mediaList.value?.vlist?.scrollToIndex(target);
  });
}

async function handlePage() {
  if (v.pageIndex < 1) {
    v.pageIndex = 1;
  }
  const pn = v.pageIndex;
  const offset = v.offsetMap.get(pn) ?? v.mediaInfo.offset;
  const target = v.mediaInfo.sections ? v.tab : undefined;
  v.searching = true;
  const result = await data.getMediaInfo(v.mediaInfo.id, v.mediaInfo.type, {
    pn,
    target,
    offset,
  });
  if (result.offset) v.offsetMap.set(pn + 1, result.offset);
  Object.assign(v.mediaInfo, result);
  v.searching = false;
  updateIndex();
}

async function updateTab(t: number) {
  v.tab = t;
  v.pageIndex = 1;
  v.searching = true;
  const info = await data.getMediaInfo(v.mediaInfo.id, v.mediaInfo.type, {
    target: t,
  });
  v.mediaInfo = info;
  await nextTick(); // trigger v-if
  v.searching = false;
  updateIndex();
}

watch(
  () => v.checkboxs.length,
  (len) => {
    if (len > 30) AppLog(i18n.global.t('error.selectLimit'), 'warning');
  },
);

onActivated(() => {
  // Force VList to refresh after keep-alive activation. DO NOT DELETE IT.
  mediaList.value?.vlist?.scrollBy(0);
});

defineExpose({ search, downloadFromExtension });

async function downloadFromExtension(
  input: string,
  scope: 'current' | 'all' = 'current',
) {
  await submitExtensionDownload(input, scope);
  const page = await components.navigate('downPage');
  page.tab = settings.auto_download ? 'doing' : 'backlog';
}

async function search(overrideInput?: string) {
  try {
    const input = (overrideInput ?? v.searchInput).trim();
    v.offsetMap.clear();
    v.searchInput = input;
    v.listActive = false;
    v.searching = false;
    v.checkboxs.length = 0;
    v.pageIndex = 1;
    v.tab = -1;
    if (!input.length) return;
    provider.value = chooseProvider(input, v.mediaType !== 'auto');
    if (provider.value === 'ytdlp') {
      providerInput.value = input;
      providerGeneration.value += 1;
      return;
    }
    v.searching = true;
    const raw = await parseId(input, v.mediaType !== 'auto');
    const query = {
      id: raw.id,
      type: v.mediaType === 'auto' ? raw.type! : v.mediaType,
    };
    log.info('Query: ' + JSON.stringify(query));
    const info = await data.getMediaInfo(query.id, query.type, {
      target: raw.target,
    });
    v.mediaInfo = info;
    v.listActive = true;
    if (info.sections) v.tab = info.sections.target;
    if (info.offset) v.offsetMap.set(1, '');
    updateIndex();
  } finally {
    await nextTick();
    v.searching = false;
  }
}

async function updateEdge(edge_id: number) {
  const { edge, list } = v.mediaInfo;
  const id = list[0].aid;
  if (!edge || !id) return;
  const graph_version = edge.graph_version;
  const edgeInfo = await extras.getEdgeInfo(id, graph_version, edge_id);
  v.mediaInfo.edge = {
    graph_version,
    edge_id: edgeInfo.edge_id,
    list: edgeInfo.story_list,
    choices: edgeInfo.edges.questions?.[0].choices,
    vars: edgeInfo.hidden_vars,
  };
  const item = edgeInfo.story_list.find((v) => v.is_current);
  if (!item) return;
  Object.assign(v.mediaInfo.list[0], {
    cid: item.cid,
    title: item.title,
    cover: item.cover,
  });
}

async function initPopup() {
  if (!v.checkboxs.length) return;
  v.checkboxs = v.checkboxs.filter(
    (i) => i >= 0 && i < v.mediaInfo.list.length,
  );
  v.checkboxs.sort((a, b) => a - b);
  const limit = pLimit(settings.max_conc);
  const tasks = v.checkboxs.map((i) =>
    limit(async () => {
      const info = v.mediaInfo.list[i];
      if (!info.cid && info.aid) {
        info.cid = await data.getCid(info.aid);
      }
    }),
  );
  await Promise.all(tasks);
  const item = v.mediaInfo.list[v.checkboxs[0]];
  const select = await components.c.selectPopup?.getSelect(item);
  if (!select) return;
  queue.submit(v.mediaInfo, select, v.checkboxs);
  const page = await components.navigate('downPage');
  page.tab = 'backlog';
}

async function exportData() {
  const title =
    v.mediaInfo.nfo?.showtitle ??
    i18n.global.t(`mediaType.` + v.mediaInfo.type);
  const path = await save({
    defaultPath: `${settings.down_dir}/${strip(title)}_${Date.now()}.json`,
  });
  if (!path) return;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const result = await commands.exportData(path, v.mediaInfo as any);
  if (result.status === 'error') throw result;
  AppLog(i18n.global.t('search.exported', [path]), 'success');
}
</script>
