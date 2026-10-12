import pLimit from 'p-limit';

import { useComponentsStore, useSettingsStore } from '@/store';
import { data } from '@/services/media';
import { parseId } from '@/services/utils';
import * as queue from '@/services/queue';
import { invoke } from '@tauri-apps/api/core';

export async function submitExtensionDownload(
  input: string,
  scope: 'current' | 'all' = 'current',
) {
  const settings = useSettingsStore();
  const components = useComponentsStore();

  const raw = await parseId(input, false);
  const mediaInfo = await data.getMediaInfo(raw.id, raw.type!, {
    target: raw.target,
  });
  if (!mediaInfo.list?.length) return;

  const target = mediaInfo.list.findIndex((item) => item.isTarget);
  let selected = [target >= 0 ? target : (mediaInfo.list[0]?.index ?? 0)];
  if (scope === 'all') {
    selected = [...Array(mediaInfo.list.length).keys()];
  }
  selected = selected.filter((i) => i >= 0 && i < mediaInfo.list.length);
  if (!selected.length) return;

  const limit = pLimit(settings.max_conc);
  await Promise.all(
    selected.map((i) =>
      limit(async () => {
        const item = mediaInfo.list[i];
        if (!item.cid && item.aid) item.cid = await data.getCid(item.aid);
      }),
    ),
  );

  const first = mediaInfo.list[selected[0]];
  const select = await components.c.selectPopup?.getDefaultSelect(first);
  if (!select) return;

  const createdIds = await queue.submit(mediaInfo, select, selected);

  // The extension settings panel is only relevant while tasks stay in backlog.
  // Build its option list from the same provider used by CONest's real Popup,
  // then store only compact capability metadata (never stream URLs).
  if (!settings.auto_download && createdIds.length) {
    await Promise.all(
      createdIds.map(async (id, offset) => {
        const item = mediaInfo.list[selected[offset]];
        if (!item) return;

        try {
          const provider = await components.c.selectPopup?.getProvider(item);
          if (!provider) return;

          const encByRes: Record<string, number[]> = {};
          for (const video of provider.video ?? []) {
            if (!video.codecid) continue;
            const key = String(video.id);
            encByRes[key] ??= [];
            if (!encByRes[key].includes(video.codecid)) {
              encByRes[key].push(video.codecid);
            }
          }

          const options = {
            res: [...new Set((provider.video ?? []).map((v) => v.id))],
            encByRes,
            abr: [...new Set((provider.audio ?? []).map((v) => v.id))],
            fmt:
              provider.video?.length || provider.audio?.length
                ? ['dash', 'mp4', 'flv']
                : [],
            misc: {
              opusContent: provider.misc.opusContent,
              opusImages: provider.misc.opusImages,
              aiSummary: provider.misc.aiSummary,
              subtitles: provider.misc.subtitles,
            },
            nfo: provider.nfo,
            danmaku: provider.danmaku,
            thumb: provider.thumb,
            media: {
              audioVideo: Boolean(provider.video?.length && provider.audio?.length),
              video: Boolean(provider.video?.length),
              audio: Boolean(provider.audio?.length),
            },
          };

          await invoke('set_extension_task_options', { id, options });
        } catch (error) {
          console.warn('Failed to prepare extension task options', error);
        }
      }),
    );
  }
}
