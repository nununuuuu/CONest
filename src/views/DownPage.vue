<template>
  <div class="workspace-page">
    <h1 class="w-full my-1.5">
      <i :class="[$fa.weight, 'fa-download']"></i>
      <span>{{ $t('down.title') }}</span>
    </h1>
    <div class="workspace-list-tools">
      <button
        :class="{ 'workspace-primary': mode === 'unified' }"
        @click="mode = 'unified'"
      >
        統一任務</button
      ><button
        :class="{ 'workspace-primary': mode === 'native' }"
        @click="mode = 'native'"
      >
        Bilibili 排程與完整控制</button
      ><button
        :class="{ 'workspace-primary': mode === 'external' }"
        @click="mode = 'external'"
      >
        多平台任務與日誌
      </button>
    </div>
    <UnifiedTasks v-if="mode === 'unified'" />
    <ProviderTasks v-else-if="mode === 'external'" />
    <div v-else class="flex w-full h-full mt-4 flex-1 gap-3 min-h-0">
      <Transition mode="out-in">
        <Queue :key="tab" v-model="tab" />
      </Transition>
      <div class="tab">
        <button
          v-for="(i, k) in tabs"
          :key="k"
          :class="{ active: tab === k }"
          @click="tab = k"
        >
          <span>{{ $t(`down.${k}`) }}</span>
          <i :class="[tab === k ? 'fa-solid' : 'fa-light', i]"></i>
          <label class="primary-color"></label>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import UnifiedTasks from '@/components/UnifiedTasks.vue';
import ProviderTasks from '@/components/ProviderTasks.vue';
const mode = ref<'unified' | 'native' | 'external'>('unified');

import { Queue } from '@/components/DownPage';

const tabs = {
  backlog: 'fa-books-medical',
  pending: 'fa-stopwatch',
  doing: 'fa-hourglass-half',
  complete: 'fa-check',
};
type Tab = keyof typeof tabs;

const tab = ref<Tab>('backlog');

defineExpose({ tab });
</script>

<style scoped>
@reference 'tailwindcss';

:deep(.wrapper) {
  @apply flex flex-col p-3 rounded-lg text-sm bg-(--block-color);
  @apply gap-0.5 my-px border border-(--split-color) w-full min-h-0;
}
</style>
