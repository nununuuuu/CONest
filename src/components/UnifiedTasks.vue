<template>
  <div class="task-center-scroll">
    <div class="workspace-list-tools">
      <button
        v-for="(label, key) in filters"
        :key="key"
        :class="{ 'workspace-primary': filter === key }"
        @click="filter = key"
      >
        {{ label }}</button
      ><select v-model="source" aria-label="平台篩選">
        <option value="all">所有平台</option>
        <option value="bilibili">Bilibili 原生</option>
        <option value="ytdlp">yt-dlp 多平台</option></select
      ><button :disabled="busy" @click="startAll">開始待下載項目</button>
    </div>
    <p v-if="error || yt.loadError" class="provider-error" role="alert">
      {{ error || yt.loadError }}
    </p>
    <table class="unified-task-table">
      <thead>
        <tr>
          <th>內容 / 來源</th>
          <th>狀態與進度</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.id">
          <td>
            <div class="task-title text">{{ row.title }}</div>
            <div class="desc">
              {{
                row.provider === 'bilibili' ? 'Bilibili 原生' : row.urlHost
              }}
              · {{ row.id }}
            </div>
          </td>
          <td>
            <span>{{ row.phase }}</span
            ><template v-if="row.state === 'active'"
              ><span class="desc"> · {{ row.progress.toFixed(1) }}%</span>
              <div class="provider-progress">
                <span
                  :style="{ width: `${row.progress}%` }"
                ></span></div></template
            ><span v-if="row.speed != null" class="desc"
              >{{ (row.speed / 1048576).toFixed(2) }} MB/s</span
            >
          </td>
          <td>
            <div class="task-actions">
              <button
                v-if="row.state === 'backlog'"
                :disabled="busy"
                @click="action(row, 'start')"
              >
                開始
              </button>
              <button
                v-if="row.provider === 'bilibili' && row.state === 'active'"
                :disabled="busy"
                @click="action(row, 'pause')"
              >
                暫停
              </button>
              <button
                v-if="row.provider === 'bilibili' && row.state === 'paused'"
                :disabled="busy"
                @click="action(row, 'resume')"
              >
                繼續
              </button>
              <button
                v-if="
                  row.provider === 'ytdlp' &&
                  ['active', 'processing', 'pending'].includes(row.state)
                "
                :disabled="busy"
                @click="action(row, 'stop')"
              >
                停止
              </button>
              <button
                v-if="['failed', 'stopped'].includes(row.state)"
                :disabled="busy"
                @click="action(row, 'retry')"
              >
                重試
              </button>
              <button @click="detail(row)">詳情 / 設定</button>
              <button
                v-if="row.state === 'completed'"
                @click="action(row, 'open')"
              >
                資料夾
              </button>
            </div>
          </td>
        </tr>
        <tr v-if="!rows.length">
          <td colspan="3" class="provider-placeholder">沒有符合篩選的任務</td>
        </tr>
      </tbody>
    </table>
    <details v-if="detailTask" open class="workspace-details">
      <summary>{{ detailTask.title }} · 完整任務控制</summary>
      <Task
        v-if="detailTask.provider === 'bilibili' && queue.tasks[detailTask.id]"
        :task="queue.tasks[detailTask.id]"
        :sid="queue.tasks[detailTask.id].sid || undefined"
      />
      <template v-else-if="yt.tasks[detailTask.id]"
        ><p class="provider-error text">{{ yt.tasks[detailTask.id].error }}</p>
        <p class="desc text">
          {{
            yt.tasks[detailTask.id].output ||
            yt.tasks[detailTask.id].options.output_dir
          }}
        </p>
        <pre class="text">{{ yt.tasks[detailTask.id].log.join('\n') }}</pre>
        <button
          v-if="
            !['running', 'processing', 'queued'].includes(
              yt.tasks[detailTask.id].state,
            )
          "
          @click="action(detailTask, 'remove')"
        >
          移除記錄（不刪除檔案）</button
        ><button @click="action(detailTask, 'open')">
          開啟資料夾
        </button></template
      >
    </details>
  </div>
</template>
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useQueueStore } from '@/store';
import { useYtdlpStore, message } from '@/services/ytdlp';
import { commands, type CtrlEvent } from '@/services/backend';
import { processQueue, processQueueTask } from '@/services/queue';
import { Task } from '@/components/DownPage';
type Row = {
  id: string;
  provider: 'bilibili' | 'ytdlp';
  title: string;
  state: string;
  phase: string;
  progress: number;
  speed: number | null;
  urlHost: string;
};
const queue = useQueueStore();
const yt = useYtdlpStore();
const filter = ref('all');
const source = ref('all');
const busy = ref(false);
const error = ref('');
const detailTask = ref<Row>();
const filters = {
  all: '全部',
  backlog: '待開始',
  active: '進行中',
  pending: '等待中',
  completed: '已完成',
  failed: '失敗 / 停止',
};
const stateName: Record<string, string> = {
  backlog: '待開始',
  pending: '等待中',
  active: '下載 / 後處理中',
  completed: '已完成',
  paused: '已暫停',
  cancelled: '已取消',
  failed: '失敗',
  stopped: '已停止',
};
function host(url: string) {
  try {
    return new URL(url).hostname;
  } catch {
    return 'yt-dlp';
  }
}
const rows = computed(() => {
  const bili: Row[] = Object.values(queue.tasks).map((t) => {
    let progress = 0;
    for (const sub of t.subtasks || []) {
      const v = t.status[sub.id];
      progress += v?.content ? Math.min(1, v.chunk / v.content) : 0;
    }
    progress = t.subtasks.length ? (progress / t.subtasks.length) * 100 : 0;
    return {
      id: t.id,
      provider: 'bilibili',
      title: t.item.title,
      state: t.state,
      phase: stateName[t.state] || t.state,
      progress,
      speed: null,
      urlHost: 'Bilibili',
    };
  });
  const external: Row[] = Object.values(yt.tasks).map((t) => ({
    id: t.id,
    provider: 'ytdlp',
    title: t.entry.title,
    state:
      ({ running: 'active', queued: 'pending' } as Record<string, string>)[
        t.state
      ] || t.state,
    phase: t.phase,
    progress: t.progress,
    speed: t.speed,
    urlHost: host(t.entry.url),
  }));
  return [...bili, ...external].filter(
    (r) =>
      (source.value === 'all' || r.provider === source.value) &&
      (filter.value === 'all' ||
        (filter.value === 'active'
          ? ['active', 'processing', 'paused'].includes(r.state)
          : filter.value === 'failed'
            ? ['failed', 'stopped', 'cancelled'].includes(r.state)
            : r.state === filter.value)),
  );
});
async function action(row: Row, act: string) {
  busy.value = true;
  error.value = '';
  try {
    if (row.provider === 'ytdlp') {
      await yt.control(row.id, act);
      if (act === 'remove') detailTask.value = undefined;
    } else if (act === 'start') {
      await processQueueTask(row.id);
    } else {
      const result =
        act === 'open'
          ? await commands.openFolder(null, row.id)
          : await commands.ctrlEvent(
              act as CtrlEvent,
              queue.tasks[row.id].sid,
              row.id,
            );
      if (result.status === 'error') throw result.error;
    }
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
function detail(row: Row) {
  detailTask.value = row;
}
async function startAll() {
  busy.value = true;
  error.value = '';
  try {
    await processQueue();
    for (const task of Object.values(yt.tasks).filter(
      (t) => t.state === 'backlog',
    ))
      await yt.control(task.id, 'start');
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
onMounted(() => {
  void yt.initialize().catch((e) => {
    error.value = message(e);
  });
});
</script>
