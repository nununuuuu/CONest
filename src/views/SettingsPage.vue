<template>
  <div class="workspace-page">
    <h1 class="w-full my-1.5">
      <i :class="[$fa.weight, 'fa-gear']"></i>
      <span>{{ $t('settings.title') }}</span>
    </h1>
    <div class="flex w-full flex-1 mt-4 gap-4 min-h-0">
      <Transition mode="out-in">
        <div :key="tab" class="flex flex-col flex-1 min-w-0 min-h-0 overflow-auto pr-3 pb-6">
          <component :is="list[tab].comp" :key="tab" />
        </div>
      </Transition>
      <div class="tab">
        <button
          v-for="(v, k) in list"
          :key="k"
          :class="{ active: tab === k }"
          @click="tab = k"
        >
          <span>{{ k === 'engines' ? '下載引擎' : $t('settings.' + k) }}</span>
          <i :class="[tab === k ? 'fa-solid' : 'fa-light', v.icon]"></i>
          <label class="primary-color"></label>
        </button>
      </div>
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref } from 'vue';
import Engines from '@/components/SettingsPage/Engines.vue';

import {
  General,
  Storage,
  Download,
  Strategy,
  Format,
} from '@/components/SettingsPage';
const list = {
  engines: { icon: 'fa-puzzle-piece', comp: Engines },
  general: { icon: 'fa-sliders', comp: General },
  storage: { icon: 'fa-database', comp: Storage },
  download: { icon: 'fa-download', comp: Download },
  strategy: { icon: 'fa-bullseye-arrow', comp: Strategy },
  format: { icon: 'fa-file-signature', comp: Format },
};

const tab = ref<keyof typeof list>('general');
</script>
<style scoped>
@reference 'tailwindcss';

:deep(h3) {
  @apply min-w-32 mr-4 inline-block;
  & + button {
    @apply my-2;
  }
}
:deep(hr) {
  @apply my-4;
}
:deep(section > div) {
  @apply mt-2;
}
</style>
