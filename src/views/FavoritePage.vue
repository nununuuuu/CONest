<template>
  <div>
    <h1 class="w-full mt-1.5 mb-auto">
      <i :class="[$fa.weight, 'fa-star']"></i>
      <span>{{ $t('favoritePage.title') }}</span>
    </h1>

    <div class="flex w-full h-full mt-5.5 flex-1 gap-3 min-h-0">
      <Transition name="slide">
        <VList
          v-if="v.listActive"
          v-slot="{ item }"
          :data="v.list.length ? v.list : ['empty']"
          class="flex-1 min-w-0"
        >
          <div>
            <Empty
              v-if="item === 'empty'"
              :text="$t(user.isLogin ? 'favoritePage.empty' : 'favoritePage.loginRequired')"
            />

            <button
              v-else
              class="favorite-card flex gap-4 p-3 rounded-lg my-px bg-(--block-color) text-sm h-24 w-full text-left"
              @click="openFolder(item)"
            >
              <div
                class="relative flex items-center justify-center rounded-lg min-w-32 h-full overflow-hidden folder-icon"
              >
                <img
                  v-if="item.cover"
                  :src="item.cover"
                  :alt="item.title"
                  class="w-full h-full object-cover"
                  referrerpolicy="no-referrer"
                />
                <i v-else :class="[$fa.weight, 'fa-folder-star']"></i>
              </div>

              <div class="w-full flex flex-col justify-center gap-2 min-w-0">
                <h2 class="text-base truncate">{{ item.title }}</h2>
                <div class="desc">
                  <i :class="[$fa.weight, 'fa-file-video']"></i>
                  <span>{{ $t('favoritePage.count', { count: item.media_count }) }}</span>
                </div>
              </div>

              <i
                :class="[$fa.weight, 'fa-chevron-right']"
                class="self-center mr-2 shrink-0"
              ></i>
            </button>
          </div>
        </VList>
      </Transition>

      <div class="flex flex-col w-32 gap-2 ml-auto">
        <button @click="refresh">
          <i :class="[$fa.weight, 'fa-rotate-right']"></i>
          <span>{{ $t('favoritePage.refresh') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { onActivated, reactive } from 'vue';
import { VList } from 'virtua/vue';

import { Empty } from '@/components';
import { useComponentsStore, useUserStore } from '@/store';
import { AppError } from '@/services/error';
import { tryFetch } from '@/services/utils';

type FavoriteFolder = {
  id: number;
  title: string;
  media_count: number;
  cover?: string;
};

const user = useUserStore();
const components = useComponentsStore();

const v = reactive({
  listActive: false,
  list: [] as FavoriteFolder[],
});

async function refresh() {
  v.listActive = false;
  try {
    if (!user.isLogin) {
      v.list = [];
      return;
    }
    const body = await tryFetch(
      'https://api.bilibili.com/x/v3/fav/folder/created/list-all',
      { params: { up_mid: user.mid } },
    );
    const folders: FavoriteFolder[] = (body?.data?.list ?? []).map(
      (item: any) => ({
        id: Number(item.id ?? item.fid),
        title: String(item.title ?? ''),
        media_count: Number(item.media_count ?? 0),
      }),
    );

    // The folder-list API does not provide the thumbnail shown on the Bilibili
    // website. Fetch only the first media item of each folder and use its cover.
    // Do this sequentially to avoid a burst of API requests.
    for (const folder of folders) {
      if (!folder.id || folder.media_count <= 0) continue;
      try {
        const detail = await tryFetch(
          'https://api.bilibili.com/x/v3/fav/resource/list',
          {
            params: {
              media_id: folder.id,
              pn: 1,
              ps: 1,
              platform: 'web',
            },
            ignoreErr: true,
          },
        );
        const media = detail?.data?.medias?.[0];
        folder.cover = String(media?.cover ?? media?.upper?.face ?? '');
      } catch (error) {
        console.warn(`Favorite cover unavailable: ${folder.id}`, error);
      }
    }

    v.list = folders;
  } catch (err) {
    new AppError(err).handle();
    v.list = [];
  } finally {
    v.listActive = true;
  }
}

async function openFolder(folder: FavoriteFolder) {
  const page = await components.navigate('searchPage');
  await page.search(
    `https://space.bilibili.com/${user.mid}/favlist?fid=${folder.id}`,
  );
}

onActivated(refresh);
</script>

<style scoped>
@reference 'tailwindcss';

.favorite-card {
  @apply border border-transparent transition-colors;
}
.favorite-card:hover {
  @apply border-(--primary-color);
}
.folder-icon {
  @apply bg-(--solid-block-color) text-(--primary-color) text-3xl;
}
.folder-icon img {
  display: block;
}
</style>
