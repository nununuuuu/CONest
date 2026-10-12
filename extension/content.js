(() => {
  const ROOT_ID = 'conest-extension-root';
  const PANEL_OPEN_KEY = 'conestPanelOpen';
  const TAB_Y_KEY = 'conestTabY';
  let lastUrl = location.href;
  let taskTimer = null;
  let routeTimer = null;
  let refreshPromise = null;
  let refreshQueued = false;
  let bridgeOnline = false;
  let contextInvalidated = false;

  function isContextInvalidatedError(error) {
    return /extension context invalidated/i.test(String(error?.message || error || ''));
  }

  function handleContextInvalidated(error) {
    if (contextInvalidated || !isContextInvalidatedError(error)) return false;
    contextInvalidated = true;
    clearInterval(taskTimer);
    taskTimer = null;
    clearTimeout(routeTimer);
    routeTimer = null;

    const status = document.querySelector(`#${ROOT_ID} .conest-status`);
    if (status) {
      status.textContent = '插件已更新，正在重新載入頁面…';
      status.classList.remove('error');
    }

    // A Chrome MV3 content script cannot reconnect after its extension was
    // reloaded/updated. A single page reload is the only way to obtain a fresh
    // extension context; this also avoids stale duplicate UI on Bilibili SPA routes.
    window.setTimeout(() => location.reload(), 120);
    return true;
  }

  async function runtimeMessage(message) {
    if (contextInvalidated) return null;
    try {
      if (!chrome?.runtime?.id) throw new Error('Extension context invalidated.');
      return await chrome.runtime.sendMessage(message);
    } catch (error) {
      if (handleContextInvalidated(error)) return null;
      throw error;
    }
  }

  async function storageGet(keys) {
    if (contextInvalidated) return {};
    try {
      if (!chrome?.runtime?.id) throw new Error('Extension context invalidated.');
      return await chrome.storage.local.get(keys);
    } catch (error) {
      handleContextInvalidated(error);
      return {};
    }
  }

  function storageSet(value) {
    if (contextInvalidated) return;
    try {
      if (!chrome?.runtime?.id) throw new Error('Extension context invalidated.');
      void chrome.storage.local.set(value).catch(handleContextInvalidated);
    } catch (error) {
      handleContextInvalidated(error);
    }
  }

  function supportedPage() {
    return /\/video\//.test(location.pathname) || /medialist|list|collection/.test(location.pathname);
  }

  function videoUrlFromHref(href) {
    try {
      const u = new URL(href, location.origin);
      if (u.hostname !== 'www.bilibili.com') return null;
      const m = u.pathname.match(/^\/video\/(BV[a-zA-Z0-9]+)/i);
      return m ? `${u.origin}/video/${m[1]}` : null;
    } catch (_) {
      return null;
    }
  }

  function collectLoadedCollectionUrls() {
    const found = new Set();
    const current = videoUrlFromHref(location.href);
    if (current) found.add(current);

    const dedicatedListPage = /medialist|\/list\//.test(location.pathname);
    const selectors = dedicatedListPage
      ? ['main', '#app', '.medialist-container']
      : [
          '.video-pod',
          '.video-pod__body',
          '.video-sections-content-list',
          '.base-video-sections-v1',
          '[class*="playlist"]',
          '[class*="collection"]'
        ];

    const roots = selectors.flatMap(selector => [...document.querySelectorAll(selector)]);
    roots.forEach(root => {
      root.querySelectorAll('a[href*="/video/BV"]').forEach(a => {
        const url = videoUrlFromHref(a.href);
        if (url) found.add(url);
      });
    });
    return [...found].slice(0, 200);
  }

  async function send(scope) {
    let urls = [location.href];
    let apiScope = scope;
    if (scope === 'collection') {
      urls = collectLoadedCollectionUrls();
      apiScope = 'current';
      if (urls.length <= 1) {
        setStatus('找不到已載入的合集影片', true);
        return;
      }
    }

    setStatus(bridgeOnline ? '正在傳送…' : '正在啟動 CONest…');
    let response = null;
    try {
      response = await runtimeMessage({
        type: 'conest-download',
        payload: { urls, scope: apiScope }
      });
    } catch (error) {
      setStatus(`無法連線到 CONest：${String(error)}`, true);
      return;
    }
    if (contextInvalidated) return;
    if (response?.ok) {
      setStatus(
        scope === 'collection'
          ? `已送出 ${urls.length} 個影片，正在建立任務…`
          : '已送出，正在建立任務…'
      );
      void checkBridge();
      setTimeout(refreshTasks, 500);
      setTimeout(refreshTasks, 2000);
      setTimeout(refreshTasks, 5000);
    } else {
      const detail =
        response?.data?.error ||
        response?.error ||
        (response?.status ? `HTTP ${response.status}` : '');
      setStatus(`無法連線到 CONest${detail ? `：${detail}` : ''}`, true);
    }
  }

  function setStatus(text, error = false) {
    const el = document.querySelector(`#${ROOT_ID} .conest-status`);
    if (!el) return;
    el.textContent = text;
    el.classList.toggle('error', error);
    clearTimeout(setStatus.timer);
    setStatus.timer = setTimeout(() => {
      if (el && el.textContent === text) el.textContent = '';
    }, 5000);
  }

  function mediaModeLabel(select, fallback = '') {
    const media = select?.media;
    if (!media) return fallback || '';

    const modes = [];
    if (media.audioVideo) modes.push('影+音');
    if (media.video) modes.push('僅影片');
    if (media.audio) modes.push('僅音訊');

    return modes.length ? modes.join(' + ') : (fallback || '');
  }

  function stateLabel(state) {
    const map = {
      backlog: '備選區',
      pending: '等待中',
      active: '進行中',
      completed: '已完成',
      paused: '已暫停',
      failed: '失敗',
      cancelled: '已取消'
    };
    return map[state] || state || '未知';
  }

  let cachedTasks = [];

  function renderTasks(tasks) {
    cachedTasks = Array.isArray(tasks) ? tasks : [];
    const list = document.querySelector(`#${ROOT_ID} .conest-task-list`);
    const count = document.querySelector(`#${ROOT_ID} .conest-task-count`);
    if (!list) return;
    count.textContent = String(cachedTasks.length);
    const clearCompleted = document.querySelector(`#${ROOT_ID} .conest-clear-completed`);
    if (clearCompleted) {
      clearCompleted.hidden = !cachedTasks.some(
        task => task.state === 'completed' || task.state === 'failed'
      );
    }
    if (!cachedTasks.length) {
      list.innerHTML = '<div class="conest-task-empty">目前沒有任務</div>';
      return;
    }

    list.innerHTML = cachedTasks.slice(0, 30).map(task => {
      const progress = Math.max(0, Math.min(100, Number(task.progress || 0)));
      const backlog = task.state === 'backlog';
      const removable =
        backlog || task.state === 'completed' || task.state === 'failed';
      return `
        <div class="conest-task" data-task-id="${escapeHtml(task.id)}">
          <div class="conest-task-cover-wrap">
            ${task.cover ? `<img class="conest-task-cover" src="${escapeHtml(task.cover)}" referrerpolicy="no-referrer">` : '<div class="conest-task-cover placeholder"></div>'}
          </div>
          <div class="conest-task-body">
            <div class="conest-task-title" title="${escapeHtml(task.title || '')}">${escapeHtml(task.title || '未命名任務')}</div>
            <div class="conest-task-meta">
              <span class="conest-task-state state-${escapeHtml(task.state || '')}">${stateLabel(task.state)}</span>
              <span>${escapeHtml(mediaModeLabel(task.select, task.mediaType || ''))}</span>
            </div>
            ${(backlog || removable) ? `
              <div class="conest-task-actions">
                ${backlog ? `
                  <button class="conest-task-settings" data-task-id="${escapeHtml(task.id)}" type="button">設定</button>
                  <button class="conest-task-start" data-task-id="${escapeHtml(task.id)}" type="button">下載</button>
                ` : ''}
                ${removable ? `
                  <button class="conest-task-remove" data-task-id="${escapeHtml(task.id)}" type="button" title="移除任務" aria-label="移除任務">
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 3h6l1 2h4v2H4V5h4l1-2Zm-2 6h10l-1 11H8L7 9Zm3 2v7h2v-7h-2Zm4 0v7h2v-7h-2Z"/></svg>
                  </button>
                ` : ''}
              </div>` : ''}
            <div class="conest-progress"><span style="width:${progress}%"></span></div>
            <div class="conest-progress-text">${progress.toFixed(0)}%</div>
          </div>
        </div>
      `;
    }).join('');
  }

  const qualityLabels = {
    res: {
      6: '240P 極速',
      16: '360P 流暢',
      32: '480P 標清',
      64: '720P 準高清',
      80: '1080P 高清',
      112: '1080P 高碼率',
      116: '1080P 60 幀',
      120: '4K 超高清',
      125: 'HDR 真彩',
      126: '杜比視界',
      127: '8K 超高清'
    },
    abr: {
      30216: '64K',
      30228: '128K',
      30232: '132K',
      30250: '杜比全景聲',
      30251: 'Hi-Res 無損',
      30252: '無損 FLAC',
      30280: '192K',
      30380: '320K'
    },
    enc: {
      7: 'AVC (H.264)',
      12: 'HEVC (H.265)',
      13: 'AV1'
    },
    fmt: {
      dash: 'DASH 格式',
      mp4: 'MP4 格式',
      flv: 'FLV 格式'
    }
  };

  const thumbLabels = {
    pic: '封面',
    cover: '封面',
    square_cover: '方形封面',
    first_frame: '首幀畫面',
    ugc_season_cover: '合集封面',
    season_cover: '本季封面',
    season_horizontal_cover_1610: '本季海報 (16:10)',
    season_horizontal_cover_169: '本季海報 (16:9)'
  };

  function cloneSettings(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function optionLabel(group, value) {
    return qualityLabels[group]?.[value] || String(value);
  }

  function thumbLabel(id) {
    if (thumbLabels[id]) return thumbLabels[id];
    const brief = String(id).match(/^brief-(\d+)$/);
    return brief ? `預覽-${brief[1]}` : String(id);
  }

  function selectedButton(group, value, current) {
    return `<button type="button" class="conest-setting-choice${String(current) === String(value) ? ' selected' : ''}" data-setting-group="${group}" data-setting-value="${escapeHtml(value)}">${escapeHtml(optionLabel(group, value))}</button>`;
  }

  function toggleButton(group, key, label, active) {
    return `<button type="button" class="conest-setting-choice${active ? ' selected' : ''}" data-setting-toggle="${group}" data-setting-key="${escapeHtml(key)}">${escapeHtml(label)}</button>`;
  }

  function section(title, body, desc = '') {
    return `
      <section class="conest-setting-section">
        <h4>${escapeHtml(title)}</h4>
        ${desc ? `<div class="conest-setting-desc">${desc}</div>` : ''}
        <div class="conest-setting-choices">${body}</div>
      </section>`;
  }

  function renderTaskSettings(task) {
    const panel = document.querySelector(`#${ROOT_ID} .conest-settings-panel`);
    const body = document.querySelector(`#${ROOT_ID} .conest-settings-body`);
    if (!panel || !body) return false;

    const options = task.settingsOptions;
    if (!options) {
      setStatus('設定資料尚未準備完成，請稍後重新整理', true);
      return false;
    }

    const select = cloneSettings(task.select || {});
    select.misc ??= {};
    select.nfo ??= {};
    select.danmaku ??= {};
    select.thumb ??= [];
    select.media ??= {};

    panel.dataset.taskId = task.id;
    panel._select = select;
    panel._options = options;

    const res = Array.isArray(options.res) ? options.res : [];
    const enc = Array.isArray(options.encByRes?.[String(select.res)])
      ? options.encByRes[String(select.res)]
      : [];
    const abr = Array.isArray(options.abr) ? options.abr : [];
    const fmt = Array.isArray(options.fmt) ? options.fmt : [];

    const sections = [];

    // 1. 雜項 — same order/items as Popup.vue extras.misc.
    const misc = [];
    if (options.misc?.opusContent) {
      misc.push(toggleButton(
        'misc', 'opusContent', '圖文/專欄/動態內容', Boolean(select.misc.opusContent)
      ));
    }
    if (options.misc?.opusImages) {
      misc.push(toggleButton(
        'misc', 'opusImages', '圖文/專欄/動態圖像', Boolean(select.misc.opusImages)
      ));
    }
    if (options.misc?.aiSummary) {
      misc.push(toggleButton(
        'misc', 'aiSummary', 'AI 摘要', Boolean(select.misc.aiSummary)
      ));
    }
    if (Array.isArray(options.misc?.subtitles) && options.misc.subtitles.length) {
      const current =
        typeof select.misc.subtitles === 'string'
          ? select.misc.subtitles
          : (options.misc.subtitles[0]?.id || '');
      const active =
        typeof select.misc.subtitles === 'string' && Boolean(select.misc.subtitles);
      misc.push(`
        <span class="conest-setting-inline-choice">
          <button type="button" class="conest-setting-choice${active ? ' selected' : ''}" data-setting-special="subtitles">字幕</button>
          <select class="conest-setting-select conest-setting-select-inline" data-setting-select="subtitles">
            ${options.misc.subtitles
              .map(v => `<option value="${escapeHtml(v.id)}"${v.id === current ? ' selected' : ''}>${escapeHtml(v.name || v.id)}</option>`)
              .join('')}
          </select>
        </span>
      `);
    }
    if (misc.length) sections.push(section('雜項', misc.join('')));

    // 2. NFO 中繼資料.
    const nfo = [];
    if (options.nfo?.album) {
      nfo.push(toggleButton(
        'nfo', 'album', '合集/劇集刮削', Boolean(select.nfo.album)
      ));
    }
    if (options.nfo?.single) {
      nfo.push(toggleButton(
        'nfo', 'single', '單集刮削', Boolean(select.nfo.single)
      ));
    }
    if (nfo.length) sections.push(section('NFO 中繼資料', nfo.join('')));

    // 3. 彈幕 — live button + history button/date in the same section.
    const danmaku = [];
    if (Array.isArray(options.danmaku) && options.danmaku.includes('live')) {
      danmaku.push(toggleButton(
        'danmaku', 'live', '即時彈幕', Boolean(select.danmaku.live)
      ));
    }
    if (Array.isArray(options.danmaku) && options.danmaku.includes('history')) {
      const active =
        typeof select.danmaku.history === 'string' && Boolean(select.danmaku.history);
      const date =
        active ? select.danmaku.history : new Date().toISOString().slice(0, 10);
      danmaku.push(`
        <span class="conest-setting-inline-choice">
          <button type="button" class="conest-setting-choice${active ? ' selected' : ''}" data-setting-special="history">歷史彈幕</button>
          <input class="conest-setting-date conest-setting-date-inline" type="date" data-setting-history-date value="${escapeHtml(date)}">
        </span>
      `);
    }
    if (danmaku.length) sections.push(section('彈幕', danmaku.join('')));

    // 4. 圖像 — use CONest's Traditional Chinese labels instead of raw ids.
    const thumbs = Array.isArray(options.thumb) ? options.thumb : [];
    if (thumbs.length) {
      sections.push(section(
        '圖像',
        thumbs
          .map(id =>
            toggleButton(
              'thumb',
              id,
              thumbLabel(id),
              Array.isArray(select.thumb) && select.thumb.includes(id)
            )
          )
          .join('')
      ));
    }

    // 5-8. Quality sections — same order as Popup.vue quality.
    if (res.length) {
      sections.push(section(
        '解析度',
        res.map(v => selectedButton('res', v, select.res)).join('')
      ));
    }

    if (enc.length) {
      sections.push(section(
        '編碼格式',
        enc.map(v => selectedButton('enc', v, select.enc)).join('')
      ));
    }

    if (abr.length) {
      sections.push(section(
        '比特率',
        abr.map(v => selectedButton('abr', v, select.abr)).join(''),
        '可於「設定 → 進階 → 轉換策略」修改是否轉換為 MP3 格式'
      ));
    }

    if (fmt.length) {
      sections.push(section(
        '串流格式',
        fmt.map(v => selectedButton('fmt', v, select.fmt)).join(''),
        '請勿隨意更改。若確有需要，請先閱讀：<a class="conest-setting-link" href="https://btjawa.top/conest/stream" target="_blank" rel="noreferrer">關於 DASH / FLV / MP4</a>，並確保你明白自己正在做什麼'
      ));
    }

    // 9. Media buttons — same order AND same independent-toggle semantics.
    const media = [];
    if (options.media?.audioVideo) {
      media.push(toggleButton(
        'media', 'audioVideo', '影+音', Boolean(select.media.audioVideo)
      ));
    }
    if (options.media?.video) {
      media.push(toggleButton(
        'media', 'video', '僅影片', Boolean(select.media.video)
      ));
    }
    if (options.media?.audio) {
      media.push(toggleButton(
        'media', 'audio', '僅音訊', Boolean(select.media.audio)
      ));
    }
    if (media.length) {
      sections.push(`
        <section class="conest-setting-media-bar">
          <div class="conest-setting-choices">${media.join('')}</div>
        </section>
      `);
    }

    body.innerHTML =
      sections.join('') ||
      '<div class="conest-setting-empty">此任務沒有可調整的下載參數</div>';

    panel.hidden = false;
    return true;
  }

  function closeTaskSettings() {
    const panel = document.querySelector(`#${ROOT_ID} .conest-settings-panel`);
    if (!panel) return;
    panel.hidden = true;
    panel.dataset.taskId = '';
    panel._select = null;
    panel._options = null;
  }

  function rerenderTaskSettings(panel) {
    renderTaskSettings({
      id: panel.dataset.taskId,
      select: panel._select,
      settingsOptions: panel._options
    });
  }

  function updateSettingPanelSelection(panel, group) {
    if (!panel?._select || !panel?._options) return;
    if (group === 'res') {
      const enc = panel._options.encByRes?.[String(panel._select.res)] || [];
      if (enc.length && !enc.map(String).includes(String(panel._select.enc))) {
        panel._select.enc = enc[0];
      }
    }
    rerenderTaskSettings(panel);
  }

  async function saveTaskSettings() {
    const panel = document.querySelector(`#${ROOT_ID} .conest-settings-panel`);
    if (!panel?._select || !panel.dataset.taskId) return;

    const save = panel.querySelector('.conest-settings-save');
    if (save) save.disabled = true;
    const response = await runtimeMessage({
      type: 'conest-task-settings',
      id: panel.dataset.taskId,
      select: panel._select
    }).catch(() => null);
    if (save) save.disabled = false;

    if (response?.ok) {
      setStatus('下載設定已更新');
      closeTaskSettings();
      await refreshTasks();
    } else {
      setStatus(
        response?.data?.error ||
          response?.error ||
          '無法更新下載設定',
        true
      );
    }
  }

  function escapeHtml(value) {
    return String(value ?? '')
      .replaceAll('&', '&amp;')
      .replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;')
      .replaceAll('"', '&quot;')
      .replaceAll("'", '&#039;');
  }

  async function checkBridge() {
    const response = await runtimeMessage({ type: 'conest-health' }).catch(() => null);
    if (contextInvalidated) return false;
    const dot = document.querySelector(`#${ROOT_ID} .conest-connection-dot`);
    if (response?.ok) {
      bridgeOnline = true;
      dot?.classList.add('online');
      return true;
    }
    bridgeOnline = false;
    dot?.classList.remove('online');
    return false;
  }

  async function refreshTasks() {
    if (contextInvalidated) return;
    if (refreshPromise) {
      refreshQueued = true;
      return refreshPromise;
    }

    refreshPromise = (async () => {
      const response = await runtimeMessage({ type: 'conest-tasks' }).catch(() => null);
      if (contextInvalidated) return;
      const dot = document.querySelector(`#${ROOT_ID} .conest-connection-dot`);
      if (response?.ok && Array.isArray(response.data?.tasks)) {
        bridgeOnline = true;
        renderTasks(response.data.tasks);
        dot?.classList.add('online');
      } else {
        bridgeOnline = false;
        dot?.classList.remove('online');
      }
    })().finally(() => {
      refreshPromise = null;
      if (refreshQueued && !contextInvalidated) {
        refreshQueued = false;
        queueMicrotask(() => void refreshTasks());
      }
    });

    return refreshPromise;
  }

  function setPanelOpen(open, persist = true) {
    const root = document.getElementById(ROOT_ID);
    if (!root) return;
    root.classList.toggle('open', open);
    if (persist) storageSet({ [PANEL_OPEN_KEY]: open });
    clearInterval(taskTimer);
    taskTimer = null;
    if (open) {
      checkBridge();
      refreshTasks();
      taskTimer = setInterval(refreshTasks, 2000);
    }
  }

  function installTabDrag(tab) {
    let dragging = false;
    let startY = 0;
    let startTop = 0;
    let moved = false;

    tab.addEventListener('pointerdown', e => {
      if (e.button !== 0) return;
      dragging = true;
      moved = false;
      startY = e.clientY;
      startTop = tab.getBoundingClientRect().top;
      tab.setPointerCapture(e.pointerId);
      e.preventDefault();
    });

    tab.addEventListener('pointermove', e => {
      if (!dragging) return;
      const delta = e.clientY - startY;
      if (Math.abs(delta) > 4) moved = true;
      if (!moved) return;
      const top = Math.max(12, Math.min(window.innerHeight - tab.offsetHeight - 12, startTop + delta));
      tab.style.top = `${top}px`;
      tab.style.bottom = 'auto';
    });

    const finish = e => {
      if (!dragging) return;
      dragging = false;
      try { tab.releasePointerCapture(e.pointerId); } catch (_) {}
      if (moved) {
        storageSet({ [TAB_Y_KEY]: tab.getBoundingClientRect().top });
        tab.dataset.dragged = '1';
        setTimeout(() => { tab.dataset.dragged = ''; }, 0);
      }
    };
    tab.addEventListener('pointerup', finish);
    tab.addEventListener('pointercancel', finish);
  }

  async function build() {
    if (contextInvalidated || !supportedPage()) return;
    if (document.getElementById(ROOT_ID)) return;

    clearInterval(taskTimer);
    taskTimer = null;

    const root = document.createElement('div');
    root.id = ROOT_ID;
    root.innerHTML = `
      <button class="conest-tab" type="button" title="CONest（可上下拖曳）">
        <span class="conest-tab-mark">C</span>
        <span class="conest-tab-label">CONest</span>
      </button>
      <aside class="conest-panel" aria-label="CONest 下載側邊欄">
        <header class="conest-header">
          <div class="conest-brand"><span class="conest-connection-dot"></span><strong>CONest</strong></div>
          <button class="conest-close" type="button" aria-label="收合">×</button>
        </header>
        <section class="conest-actions">
          <button data-scope="current" type="button">下載目前影片 / P</button>
          <button data-scope="all" type="button">下載全部多 P</button>
          <button data-scope="collection" type="button">下載合集（已載入項目）</button>
          <div class="conest-status"></div>
        </section>
        <section class="conest-tasks-section">
          <div class="conest-section-title">
            <span>任務列表</span>
            <div class="conest-task-tools"><span class="conest-task-count">0</span><button class="conest-clear-completed" type="button" hidden title="清除已完成與失敗紀錄" aria-label="清除已完成與失敗紀錄">清除</button><button class="conest-refresh" type="button" title="重新整理">↻</button></div>
          </div>
          <div class="conest-task-list"><div class="conest-task-empty">讀取中…</div></div>
        </section>
        <section class="conest-settings-panel" hidden>
          <div class="conest-settings-header">
            <strong>下載設定</strong>
            <button class="conest-settings-close" type="button" aria-label="關閉">×</button>
          </div>
          <div class="conest-settings-body"></div>
          <div class="conest-settings-footer">
            <button class="conest-settings-cancel" type="button">取消</button>
            <button class="conest-settings-save" type="button">套用</button>
          </div>
        </section>
      </aside>
    `;
    document.body.appendChild(root);

    const tab = root.querySelector('.conest-tab');
    const close = root.querySelector('.conest-close');
    installTabDrag(tab);

    const stored = await storageGet([PANEL_OPEN_KEY, TAB_Y_KEY]);
    if (typeof stored[TAB_Y_KEY] === 'number') {
      const top = Math.max(12, Math.min(window.innerHeight - tab.offsetHeight - 12, stored[TAB_Y_KEY]));
      tab.style.top = `${top}px`;
      tab.style.bottom = 'auto';
    }
    setPanelOpen(Boolean(stored[PANEL_OPEN_KEY]), false);

    tab.addEventListener('click', () => {
      if (tab.dataset.dragged === '1') return;
      setPanelOpen(!root.classList.contains('open'));
    });
    close.addEventListener('click', () => setPanelOpen(false));
    root.querySelector('.conest-refresh').addEventListener('click', refreshTasks);
    root.querySelector('.conest-clear-completed').addEventListener('click', async (event) => {
      const button = event.currentTarget;
      const ids = cachedTasks
        .filter(task => task.state === 'completed' || task.state === 'failed')
        .map(task => task.id);
      if (!ids.length) return;

      button.disabled = true;
      const results = await Promise.all(
        ids.map(id =>
          runtimeMessage({ type: 'conest-remove-task', id }).catch(() => null)
        )
      );
      button.disabled = false;
      if (contextInvalidated) return;

      const failed = results.filter(result => !result?.ok);
      if (failed.length) {
        setStatus(`有 ${failed.length} 筆紀錄無法移除`, true);
      } else {
        setStatus('已清除已完成與失敗紀錄');
      }
      await refreshTasks();
    });
    root.querySelector('.conest-task-list').addEventListener('click', async (event) => {
      const settingsButton = event.target.closest('.conest-task-settings');
      if (settingsButton) {
        const task = cachedTasks.find(item => item.id === settingsButton.dataset.taskId);
        if (!task) {
          setStatus('找不到任務資料', true);
          return;
        }
        renderTaskSettings(task);
        return;
      }

      const removeButton = event.target.closest('.conest-task-remove');
      if (removeButton) {
        const id = removeButton.dataset.taskId;
        removeButton.disabled = true;
        const response = await runtimeMessage({
          type: 'conest-remove-task',
          id
        }).catch(() => null);
        if (!response?.ok) {
          removeButton.disabled = false;
          setStatus(response?.data?.error || response?.error || '無法移除任務', true);
        } else {
          setStatus('已移除任務紀錄');
        }
        setTimeout(refreshTasks, 150);
        setTimeout(refreshTasks, 500);
        return;
      }

      const button = event.target.closest('.conest-task-start');
      if (!button) return;
      button.disabled = true;
      button.textContent = '啟動中…';
      const response = await runtimeMessage({
        type: 'conest-start-task',
        id: button.dataset.taskId
      }).catch(() => null);
      if (!response?.ok) {
        button.disabled = false;
        button.textContent = '下載';
        setStatus(response?.data?.error || response?.error || '無法啟動下載', true);
      }
      setTimeout(refreshTasks, 300);
      setTimeout(refreshTasks, 1000);
    });
    root.querySelector('.conest-settings-close').addEventListener('click', closeTaskSettings);
    root.querySelector('.conest-settings-cancel').addEventListener('click', closeTaskSettings);
    root.querySelector('.conest-settings-save').addEventListener('click', saveTaskSettings);
    root.querySelector('.conest-settings-panel').addEventListener('click', (event) => {
      const panel = event.currentTarget;
      if (!panel._select) return;

      const quality = event.target.closest('[data-setting-group]');
      if (quality) {
        const group = quality.dataset.settingGroup;
        const raw = quality.dataset.settingValue;
        const value = ['res', 'enc', 'abr'].includes(group) ? Number(raw) : raw;
        panel._select[group] = value;
        updateSettingPanelSelection(panel, group);
        return;
      }

      const toggle = event.target.closest('[data-setting-toggle]');
      if (toggle) {
        const group = toggle.dataset.settingToggle;
        const key = toggle.dataset.settingKey;

        if (group === 'thumb') {
          panel._select.thumb ??= [];
          const index = panel._select.thumb.indexOf(key);
          if (index === -1) panel._select.thumb.push(key);
          else panel._select.thumb.splice(index, 1);
        } else {
          panel._select[group] ??= {};
          panel._select[group][key] = !panel._select[group][key];
        }
        rerenderTaskSettings(panel);
        return;
      }

      const special = event.target.closest('[data-setting-special]');
      if (special?.dataset.settingSpecial === 'subtitles') {
        const languages = panel._options?.misc?.subtitles || [];
        if (panel._select.misc.subtitles) {
          panel._select.misc.subtitles = false;
        } else {
          panel._select.misc.subtitles = languages[0]?.id || false;
        }
        rerenderTaskSettings(panel);
        return;
      }

      if (special?.dataset.settingSpecial === 'history') {
        if (panel._select.danmaku.history) {
          panel._select.danmaku.history = false;
        } else {
          panel._select.danmaku.history = new Date().toISOString().slice(0, 10);
        }
        rerenderTaskSettings(panel);
      }
    });

    root.querySelector('.conest-settings-panel').addEventListener('change', (event) => {
      const panel = event.currentTarget;
      if (!panel._select) return;

      const subtitle = event.target.closest('[data-setting-select="subtitles"]');
      if (subtitle && panel._select.misc.subtitles) {
        panel._select.misc.subtitles = subtitle.value || false;
        rerenderTaskSettings(panel);
        return;
      }

      const date = event.target.closest('[data-setting-history-date]');
      if (date && panel._select.danmaku.history) {
        panel._select.danmaku.history = date.value || false;
        rerenderTaskSettings(panel);
      }
    });

    root.querySelectorAll('[data-scope]').forEach(button => {
      button.addEventListener('click', () => send(button.dataset.scope));
    });
  }

  function scheduleRouteSync() {
    if (contextInvalidated || location.href === lastUrl) return;
    lastUrl = location.href;
    clearTimeout(routeTimer);
    routeTimer = window.setTimeout(async () => {
      routeTimer = null;
      if (contextInvalidated) return;

      const root = document.getElementById(ROOT_ID);
      if (!supportedPage()) {
        clearInterval(taskTimer);
        taskTimer = null;
        root?.remove();
        return;
      }

      if (!root) {
        await build();
        return;
      }

      // Bilibili is a SPA. Keep the same panel instance instead of rebuilding it
      // on every video route, which prevents duplicated transition animations and
      // stale task state. Only the page URL used by the next download changes.
      if (root.classList.contains('open')) {
        void checkBridge();
        void refreshTasks();
      }
    }, 180);
  }

  const observer = new MutationObserver(scheduleRouteSync);
  observer.observe(document.documentElement, { subtree: true, childList: true });
  window.addEventListener('popstate', scheduleRouteSync);
  window.addEventListener('pageshow', () => {
    if (document.getElementById(ROOT_ID)?.classList.contains('open')) {
      void checkBridge();
      void refreshTasks();
    }
  });
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && document.getElementById(ROOT_ID)?.classList.contains('open')) {
      void checkBridge();
      void refreshTasks();
    }
  });

  void build();
})();
