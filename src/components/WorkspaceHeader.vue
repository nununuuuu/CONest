<template>
  <header class="workspace-header">
    <button class="workspace-brand" @click="go('/')">
      CO<span>N</span>est
    </button>
    <nav aria-label="主要導覽">
      <button :class="{ active: $route.path === '/' }" @click="go('/')">
        內容工作台
      </button>
      <button
        :class="{ active: $route.path === '/down-page' }"
        @click="go('/down-page')"
      >
        任務中心 <small v-if="activeCount">{{ activeCount }}</small>
      </button>
      <button
        :class="{ active: $route.path === '/settings-page' }"
        @click="go('/settings-page')"
      >
        設定
      </button>
    </nav>
    <details class="workspace-sources">
      <summary>匯入來源 <i class="fa-light fa-chevron-down"></i></summary>
      <div class="workspace-source-menu">
        <button @click="go('/favorite-page')">
          <i class="fa-light fa-star"></i>{{ $t('favoritePage.title') }}
        </button>
        <button @click="go('/history-page')">
          <i class="fa-light fa-clock"></i>{{ $t('history.title') }}
        </button>
        <button @click="go('/user-page')">
          <i class="fa-light fa-user"></i
          >{{ user.isLogin ? user.name : '帳號與登入' }}
        </button>
        <button @click="settings.theme = settings.isDark ? 'light' : 'dark'">
          <i class="fa-light fa-moon-over-sun"></i
          >{{ settings.isDark ? '切換淺色' : '切換深色' }}
        </button>
        <button @click="go('/info-page')">
          <i class="fa-light fa-circle-info"></i>{{ $t('info.title') }}
        </button>
      </div>
    </details>
  </header>
</template>
<script setup lang="ts">
import { computed } from 'vue';
import { useQueueStore, useSettingsStore, useUserStore } from '@/store';
import { useYtdlpStore } from '@/services/ytdlp';
import router from '@/router';
const user = useUserStore();
const settings = useSettingsStore();
const queue = useQueueStore();
const yt = useYtdlpStore();
const activeCount = computed(
  () =>
    Object.values(queue.tasks).filter((t) => t.state === 'active').length +
    Object.values(yt.tasks).filter((t) =>
      ['running', 'processing'].includes(t.state),
    ).length,
);
function go(path: string) {
  void router.push(path);
  document.querySelector('.workspace-sources')?.removeAttribute('open');
}
</script>
