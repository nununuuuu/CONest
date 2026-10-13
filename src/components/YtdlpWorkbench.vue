<template>
  <div class="provider-workbench">
    <div class="workspace-heading">
      <div>
        <h2>{{ collection?.title || '多平台下載' }}</h2>
        <p class="desc">
          {{
            collection
              ? `${collection.platform} · ${collection.is_playlist ? '播放清單 / 合輯' : '單一內容'} · ${collection.entries.length} 項`
              : '使用 yt-dlp 解析其他平台；Bilibili 保留原生解析。'
          }}
        </p>
      </div>
      <label
        ><input v-model="playlist" type="checkbox" />解析合輯 / 播放清單</label
      ><button :disabled="busy" @click="analyze">
        {{ busy ? '解析中…' : '重新解析' }}
      </button>
    </div>
    <p v-if="error" class="provider-error" role="alert">{{ error }}</p>
    <div class="workspace-columns">
      <section class="workspace-content">
        <div v-if="collection" class="workspace-list-tools">
          <label
            ><input
              type="checkbox"
              :checked="allSelected"
              @change="toggleAll"
            />全選可用項目</label
          ><span class="desc">已選取 {{ selected.length }} 項</span
          ><button @click="exportCollection">匯出解析資料</button>
        </div>
        <div v-if="busy" class="provider-placeholder">
          解析中；大型播放清單需要較長時間。
        </div>
        <div v-else-if="!collection" class="provider-placeholder">
          貼上網址，或使用「YouTube 搜尋」找內容。
        </div>
        <label
          v-for="item in collection?.entries"
          :key="`${item.index}-${item.id}`"
          class="workspace-media-row"
          :class="{ unavailable: !item.available }"
        >
          <input
            v-model="selected"
            type="checkbox"
            :value="item.index"
            :disabled="!item.available"
          />
          <img
            v-if="item.thumbnail"
            :src="item.thumbnail"
            alt=""
            referrerpolicy="no-referrer"
          />
          <span v-else class="provider-cover"
            ><i class="fa-light fa-film"></i
          ></span>
          <span class="workspace-media-title"
            ><strong>{{ item.title }}</strong
            ><span class="desc"
              >{{ item.index }} ·
              {{
                item.available ? item.id : '內容無法取得，原始位置已保留'
              }}</span
            ></span
          >
          <span class="desc">{{ duration(item.duration) }}</span>
        </label>
        <details v-if="collection?.warnings" class="workspace-details">
          <summary>解析提醒 / 部分項目失敗</summary>
          <pre class="text">{{ collection.warnings }}</pre>
        </details>
      </section>
      <aside class="workspace-inspector">
        <h3>輸出設定</h3>
        <label
          >下載內容<select v-model="options.mode">
            <option value="video">影片＋音訊</option>
            <option value="audio">僅音訊</option>
          </select></label
        >
        <template v-if="options.mode === 'video'">
          <label
            >畫質上限<select v-model.number="options.height">
              <option :value="0">最佳可用畫質</option>
              <option
                v-for="height in [4320, 2160, 1440, 1080, 720, 480, 360]"
                :key="height"
                :value="height"
              >
                {{ height }}p
              </option>
            </select></label
          >
          <label
            >輸出容器<select v-model="options.container">
              <option>mp4</option>
              <option>mkv</option>
              <option>webm</option>
            </select></label
          >
          <label
            >視訊編碼<select v-model="options.video_codec">
              <option value="original">保留來源，不轉碼</option>
              <option value="h264">轉為 H.264</option>
              <option value="h265">轉為 H.265 / HEVC</option>
              <option value="vp9">轉為 VP9</option>
              <option value="av1">轉為 AV1</option>
            </select></label
          >
        </template>
        <label
          >音訊編碼<select v-model="options.audio_codec">
            <option value="original">保留來源</option>
            <option value="aac">AAC</option>
            <option value="opus">Opus</option>
            <option value="mp3">MP3</option>
            <option value="flac">FLAC</option>
            <option value="wav">WAV / PCM</option>
          </select></label
        >
        <label v-if="!['original', 'flac', 'wav'].includes(options.audio_codec)"
          >音訊位元率<select v-model.number="options.bitrate">
            <option :value="0">來源 / 編碼器預設</option>
            <option
              v-for="rate in [320, 256, 192, 128, 96, 64]"
              :key="rate"
              :value="rate"
            >
              {{ rate }} kbps
            </option>
          </select></label
        >
        <label
          v-if="options.mode === 'video' && options.video_codec !== 'original'"
          >轉碼裝置<select v-model="options.accelerator">
            <option value="cpu">CPU 軟體編碼</option>
            <option value="nvidia">NVIDIA NVENC</option>
            <option value="intel">Intel QSV</option>
            <option value="amd">AMD AMF</option>
            <option value="apple">Apple VideoToolbox</option>
          </select></label
        >
        <p v-if="options.video_codec !== 'original'" class="desc provider-note">
          選擇編碼會重新轉碼。執行前會驗證編碼器；不會默默換回 CPU 或降低畫質。
        </p>
        <p
          v-if="['flac', 'wav'].includes(options.audio_codec)"
          class="desc provider-note"
        >
          有損來源轉成 FLAC/WAV 不會恢復已流失音質。
        </p>
        <label
          >儲存位置
          <div class="workspace-path text">
            {{ options.output_dir
            }}<button @click="chooseDirectory">更改</button>
          </div></label
        >
        <details class="workspace-details">
          <summary>字幕、封面與中繼資料</summary>
          <label
            >字幕語言<input
              v-model="options.subtitles"
              placeholder="留空不下載；all 或 zh.*,en.*" /></label
          ><label
            ><input
              v-model="options.auto_subtitles"
              type="checkbox"
            />包含自動字幕</label
          ><label
            ><input
              v-model="options.embed_subtitles"
              type="checkbox"
            />嵌入字幕</label
          ><label
            ><input
              v-model="options.thumbnail"
              type="checkbox"
            />下載封面</label
          ><label
            ><input v-model="options.metadata" type="checkbox" />嵌入中繼資料 /
            章節</label
          >
        </details>
        <details class="workspace-details">
          <summary>裁剪與 SponsorBlock</summary>
          <label
            ><input v-model="clipEnabled" type="checkbox" />按時間裁剪</label
          ><template v-if="clipEnabled"
            ><label
              >開始秒數<input
                v-model.number="clipStart"
                type="number"
                min="0" /></label
            ><label
              >結束秒數<input
                v-model.number="clipEnd"
                type="number"
                min="0" /></label></template
          ><label
            ><input v-model="options.sponsorblock" type="checkbox" />移除
            SponsorBlock 贊助片段（支援時）</label
          >
        </details>
        <button
          class="workspace-primary"
          :disabled="!selected.length || submitting || busy"
          @click="submit"
        >
          {{ submitting ? '建立任務中…' : `加入佇列 · ${selected.length} 項` }}
        </button>
      </aside>
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref, reactive, computed, onMounted, watch } from 'vue';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useSettingsStore } from '@/store';
import { commands } from '@/services/backend';
import {
  useYtdlpStore,
  message,
  type MediaCollection,
  type DownloadOptions,
} from '@/services/ytdlp';
import router from '@/router';
const props = defineProps<{ input: string; generation: number }>();
const yt = useYtdlpStore();
const settings = useSettingsStore();
const collection = ref<MediaCollection>();
const selected = ref<number[]>([]);
const error = ref('');
const busy = ref(false);
const submitting = ref(false);
const playlist = ref(true);
const clipEnabled = ref(false);
const clipStart = ref(0);
const clipEnd = ref(60);
const options = reactive<DownloadOptions>({
  mode: 'video',
  height: 0,
  container: 'mkv',
  video_codec: 'original',
  audio_codec: 'original',
  bitrate: 0,
  accelerator: 'cpu',
  output_dir: settings.down_dir,
  subtitles: '',
  auto_subtitles: false,
  embed_subtitles: false,
  thumbnail: false,
  metadata: settings.add_metadata,
  sponsorblock: false,
  section_start: null,
  section_end: null,
});
let request = 0;
const allSelected = computed(
  () =>
    Boolean(collection.value?.entries.some((e) => e.available)) &&
    collection.value?.entries
      .filter((e) => e.available)
      .every((e) => selected.value.includes(e.index)),
);
function toggleAll() {
  selected.value = allSelected.value
    ? []
    : collection.value?.entries
        .filter((e) => e.available)
        .map((e) => e.index) || [];
}
function duration(v: number | null) {
  if (v === null) return '';
  return `${Math.floor(v / 60)}:${String(Math.floor(v % 60)).padStart(2, '0')}`;
}
async function analyze() {
  const token = ++request;
  const input = props.input;
  busy.value = true;
  error.value = '';
  collection.value = undefined;
  selected.value = [];
  try {
    const result = await yt.analyze(input, playlist.value);
    if (token !== request) return;
    collection.value = result;
    selected.value = result.entries
      .filter((e) => e.available)
      .map((e) => e.index);
  } catch (e) {
    if (token === request) error.value = message(e);
  } finally {
    if (token === request) busy.value = false;
  }
}
async function chooseDirectory() {
  const path = await open({
    directory: true,
    defaultPath: options.output_dir || undefined,
  });
  if (typeof path === 'string') options.output_dir = path;
}
async function exportCollection() {
  if (!collection.value) return;
  const path = await save({
    defaultPath: 'media-collection.json',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  });
  if (!path) return;
  const result = await commands.exportData(
    path,
    JSON.parse(JSON.stringify(collection.value)),
  );
  if (result.status === 'error') error.value = message(result.error);
}
async function submit() {
  submitting.value = true;
  error.value = '';
  try {
    options.section_start = clipEnabled.value ? clipStart.value : null;
    options.section_end = clipEnabled.value ? clipEnd.value : null;
    await yt.submit(
      collection.value?.entries.filter((e) =>
        selected.value.includes(e.index),
      ) || [],
      JSON.parse(JSON.stringify(options)),
    );
    await router.push('/down-page');
  } catch (e) {
    error.value = message(e);
  } finally {
    submitting.value = false;
  }
}
watch(
  () => props.generation,
  () => void analyze(),
);
onMounted(() => {
  if (!options.output_dir) options.output_dir = settings.down_dir;
  void analyze();
});
</script>
