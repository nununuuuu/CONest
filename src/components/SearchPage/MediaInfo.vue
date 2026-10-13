<template>
  <div
    class="workspace-media-summary"
  >
    <Image :src="info.nfo.thumbs[0]?.url" :height="64" :ratio="16 / 10" class="workspace-summary-cover" />
    <div class="text flex flex-col gap-1 flex-1 min-w-0">
      <div class="flex gap-3 min-w-0">
        <div class="relative flex flex-col gap-1 flex-1 min-w-0">
          <h2 class="text-lg w-full truncate">
            {{ info.nfo.showtitle ?? $t('mediaType.' + info.type) }}
          </h2>
          <div class="workspace-media-stats">
            <template v-for="(v, k) in info.nfo.stat" :key="k">
              <template v-if="typeof v === 'number'">
                <i class="bcc-iconfont" :class="iconMap[k]"></i>
                <span class="ml-1.5!">{{ stat(v) }}</span>
              </template>
            </template>
          </div>
        </div>
        <a
          v-if="props.info.nfo.upper?.avatar"
          class="flex flex-col items-center cursor-pointer w-20"
          @click="openUrl('https://space.bilibili.com/' + info.nfo.upper?.mid)"
        >
          <Image
            :src="props.info.nfo.upper.avatar"
            :height="36"
            class="rounded-full!"
          />
          <span class="text-xs truncate mt-1 w-20 text-center">{{
            info.nfo.upper?.name
          }}</span>
        </a>
      </div>
      <span class="workspace-media-intro">{{
        info.nfo.intro
      }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { openUrl } from '@tauri-apps/plugin-opener';
import { MediaInfo } from '@/types/shared.d';
import { stat } from '@/services/utils';
import Image from '../Image.vue';

const props = defineProps<{
  info: MediaInfo;
}>();

const iconMap = {
  play: 'bcc-icon-icon_list_player_x1',
  danmaku: 'bcc-icon-danmuguanli',
  reply: 'bcc-icon-pinglunguanli',
  like: 'bcc-icon-ic_Likesx',
  coin: 'bcc-icon-icon_action_reward_n_x',
  favorite: 'bcc-icon-icon_action_collection_n_x',
  share: 'bcc-icon-icon_action_share_n_x',
};
</script>
