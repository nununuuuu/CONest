<template>
  <section class="provider-tasks">
    <div class="workspace-list-tools">
      <h3>多平台任務</h3>
      <span class="desc">{{ list.length }} 項</span
      ><button :disabled="working" @click="startAll">開始待下載項目</button
      ><select v-model="filter" aria-label="多平台任務狀態">
        <option value="all">全部狀態</option>
        <option value="backlog">待開始</option>
        <option value="queued">等待中</option>
        <option value="running">下載中</option>
        <option value="processing">合併 / 轉碼</option>
        <option value="completed">已完成</option>
        <option value="failed">失敗</option>
        <option value="stopped">已停止</option>
      </select>
    </div>
    <p v-if="error || yt.loadError" class="provider-error">
      {{ error || yt.loadError }}
    </p>
    <div v-if="!list.length" class="provider-placeholder">
      沒有符合篩選的多平台任務
    </div>
    <article v-for="task in list" :key="task.id" class="provider-task">
      <div class="provider-task-main">
        <strong class="text">{{ task.entry.title }}</strong
        ><span class="desc">{{ task.phase }}</span>
      </div>
      <div class="desc text">
        yt-dlp ·
        {{
          task.options.mode === 'audio'
            ? '音訊'
            : task.options.container.toUpperCase()
        }}
        · {{ task.options.height || '最佳'
        }}{{ task.options.height ? 'p' : '' }} ·
        {{ task.options.video_codec }} / {{ task.options.audio_codec }}
      </div>
      <div v-if="task.state === 'running'" class="provider-progress">
        <span :style="{ width: `${task.progress}%` }"></span>
      </div>
      <div class="provider-task-controls">
        <span v-if="task.state === 'running'" class="desc"
          >{{ task.progress.toFixed(1) }}%
          <template v-if="task.speed != null"
            >· {{ (task.speed / 1048576).toFixed(2) }} MB/s</template
          ><template v-if="task.eta != null">
            · 約 {{ Math.ceil(task.eta) }} 秒</template
          ></span
        >
        <button
          v-if="['backlog', 'failed', 'stopped'].includes(task.state)"
          :disabled="working"
          @click="act(task.id, 'start')"
        >
          {{ task.state === 'backlog' ? '開始' : '重試 / 續傳' }}
        </button>
        <button
          v-if="['queued', 'running', 'processing'].includes(task.state)"
          :disabled="working"
          @click="act(task.id, 'stop')"
        >
          停止
        </button>
        <button :disabled="working" @click="act(task.id, 'open')">
          開啟資料夾
        </button>
        <button
          v-if="!['queued', 'running', 'processing'].includes(task.state)"
          :disabled="working"
          @click="act(task.id, 'remove')"
        >
          移除記錄
        </button>
      </div>
      <p v-if="task.error" class="provider-error text">{{ task.error }}</p>
      <details class="workspace-details">
        <summary>任務詳情 / 日誌</summary>
        <p class="desc text">{{ task.output || task.options.output_dir }}</p>
        <p class="desc">{{ task.id }} · 引擎路徑與輸出設定在建立任務時固定</p>
        <pre class="text">{{ task.log.join('\n') || '尚無日誌' }}</pre>
      </details>
    </article>
  </section>
</template>
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useYtdlpStore, message } from '@/services/ytdlp';
const yt = useYtdlpStore();
const filter = ref('all');
const error = ref('');
const working = ref(false);
const list = computed(() =>
  Object.values(yt.tasks)
    .filter((t) => filter.value === 'all' || t.state === filter.value)
    .reverse(),
);
async function act(id: string, action: string) {
  working.value = true;
  error.value = '';
  try {
    await yt.control(id, action);
  } catch (e) {
    error.value = message(e);
  } finally {
    working.value = false;
  }
}
async function startAll() {
  working.value = true;
  error.value = '';
  try {
    for (const t of Object.values(yt.tasks).filter(
      (t) => t.state === 'backlog',
    ))
      await yt.control(t.id, 'start');
  } catch (e) {
    error.value = message(e);
  } finally {
    working.value = false;
  }
}
onMounted(() => {
  void yt.initialize().catch((e) => {
    error.value = message(e);
  });
});
</script>
