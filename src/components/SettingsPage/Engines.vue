<template>
  <section class="engine-settings">
    <h3>下載引擎與平台 Cookie</h3>
    <p class="desc">
      Bilibili 使用既有登入與原生下載；以下設定只供 yt-dlp 插件使用，不覆寫
      Bilibili 帳號。
    </p>
    <label
      >yt-dlp 執行檔（留空使用隨附引擎）
      <div>
        <input v-model="yt.settings.executable" type="text" /><button
          @click="pick('executable')"
        >
          選擇
        </button>
      </div></label
    >
    <label
      >Deno 執行檔（留空使用隨附引擎）
      <div>
        <input v-model="yt.settings.deno" type="text" /><button
          @click="pick('deno')"
        >
          選擇
        </button>
      </div></label
    >
    <label
      >轉碼 FFmpeg（留空使用隨附轉碼引擎）
      <div>
        <input v-model="yt.settings.ffmpeg" type="text" /><button
          @click="pick('ffmpeg')"
        >
          選擇
        </button>
      </div>
    </label>
    <label
      >Cookie 檔案（Netscape 格式）
      <div>
        <input v-model="yt.settings.cookie_file" type="text" /><button
          @click="pick('cookie_file')"
        >
          選擇
        </button>
      </div></label
    >
    <label
      >或讀取指定瀏覽器 Cookie<select v-model="yt.settings.cookie_browser">
        <option value="">不讀取</option>
        <option
          v-for="browser in [
            'chrome',
            'edge',
            'firefox',
            'brave',
            'vivaldi',
            'opera',
            'chromium',
            'safari',
          ]"
          :key="browser"
        >
          {{ browser }}
        </option>
      </select></label
    >
    <p class="desc">
      Cookie
      檔案優先於瀏覽器設定。讀取可能受瀏覽器加密或使用狀態限制，錯誤會顯示在任務中。
    </p>
    <div class="workspace-list-tools">
      <button class="workspace-primary" :disabled="busy" @click="saveSettings">
        儲存</button
      ><button :disabled="busy" @click="check">檢查引擎</button
      ><button :disabled="busy" @click="updateEngine">更新 yt-dlp</button>
    </div>
    <p v-if="status" class="desc text" role="status">{{ status }}</p>
  </section>
</template>
<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { onMounted, ref } from 'vue';
import { useYtdlpStore, message, type EngineSettings } from '@/services/ytdlp';
const yt = useYtdlpStore();
const busy = ref(false);
const status = ref('');
async function pick(key: keyof EngineSettings) {
  const path = await open({ multiple: false });
  if (typeof path === 'string') yt.settings[key] = path;
}
async function saveSettings() {
  busy.value = true;
  try {
    await yt.saveSettings();
    status.value = '設定已儲存';
  } catch (e) {
    status.value = message(e);
  } finally {
    busy.value = false;
  }
}
async function check() {
  busy.value = true;
  try {
    status.value = await invoke<string>('ytdlp_engine_info', {
      settings: { ...yt.settings },
    });
  } catch (e) {
    status.value = message(e);
  } finally {
    busy.value = false;
  }
}
async function updateEngine() {
  busy.value = true;
  try {
    status.value = await invoke<string>('ytdlp_update', {
      settings: { ...yt.settings },
    });
  } catch (e) {
    status.value = message(e);
  } finally {
    busy.value = false;
  }
}
onMounted(() => {
  void yt.initialize().catch((e) => {
    status.value = message(e);
  });
});
</script>
