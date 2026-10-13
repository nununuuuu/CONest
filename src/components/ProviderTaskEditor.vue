<template>
  <details v-if="editable" class="workspace-details">
    <summary @click="reset">調整輸出設定 / 改用 CPU 重試</summary>
    <div class="provider-retry-options">
      <label v-if="draft.mode === 'video'"
        >容器<select v-model="draft.container">
          <option>mp4</option>
          <option>mkv</option>
          <option>webm</option>
        </select></label
      >
      <label v-if="draft.mode === 'video'"
        >視訊編碼<select v-model="draft.video_codec">
          <option value="original">保留來源</option>
          <option value="h264">H.264</option>
          <option value="h265">H.265</option>
          <option value="vp9">VP9</option>
          <option value="av1">AV1</option>
        </select></label
      >
      <label
        >音訊編碼<select v-model="draft.audio_codec">
          <option value="original">保留來源</option>
          <option value="aac">AAC</option>
          <option value="opus">Opus</option>
          <option value="mp3">MP3</option>
          <option value="flac">FLAC</option>
          <option value="wav">WAV</option>
        </select></label
      >
      <label v-if="draft.mode === 'video'"
        >轉碼裝置<select v-model="draft.accelerator">
          <option value="cpu">CPU</option>
          <option value="nvidia">NVIDIA</option>
          <option value="intel">Intel</option>
          <option value="amd">AMD</option>
          <option value="apple">Apple</option>
        </select></label
      >
      <label
        >音訊位元率<input
          v-model.number="draft.bitrate"
          type="number"
          min="0"
          max="512"
          step="1"
        />kbps（0 為預設）</label
      >
      <div class="workspace-list-tools">
        <button :disabled="busy" @click="apply(false)">儲存設定</button
        ><button
          class="workspace-primary"
          :disabled="busy"
          @click="apply(true)"
        >
          儲存並開始 / 重試
        </button>
      </div>
      <p class="desc">
        引擎與 Cookie
        使用目前下載引擎設定；解析項目與原始檔保留。更換容器可能重新下载來源。
      </p>
      <p v-if="status" class="provider-error" role="status">{{ status }}</p>
    </div>
  </details>
</template>
<script setup lang="ts">
import { computed, ref } from 'vue';
import { useYtdlpStore, message, type ProviderTask } from '@/services/ytdlp';
const props = defineProps<{ task: ProviderTask }>();
const yt = useYtdlpStore();
const draft = ref({ ...props.task.options });
const busy = ref(false);
const status = ref('');
const editable = computed(() =>
  ['backlog', 'failed', 'stopped'].includes(props.task.state),
);
function reset() {
  draft.value = { ...props.task.options };
  status.value = '';
}
async function apply(start: boolean) {
  busy.value = true;
  status.value = '';
  try {
    await yt.edit(props.task.id, { ...draft.value });
    if (start) await yt.control(props.task.id, 'start');
    else status.value = '設定已儲存';
  } catch (e) {
    status.value = message(e);
  } finally {
    busy.value = false;
  }
}
</script>
