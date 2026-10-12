<template>
  <div class="info-page">
    <h1 class="w-full my-1.5">
      <i :class="[$fa.weight, 'fa-circle-info']"></i>
      <span>{{ $t('info.title') }}</span>
    </h1>

    <div class="flex flex-col w-full h-full mt-4 gap-5">
      <div class="flex items-end">
        <img class="h-16 mr-5" src="@/assets/img/icon.svg" alt="CONest" />
        <img class="h-10 mb-2" src="@/assets/img/logo.svg" alt="CONest" />
      </div>

      <button
        class="version-link"
        type="button"
        :title="`CONest v${app.version}`"
        @click="openReleaseVersion(`v${app.version}`)"
      >
        <span>CONest</span>
        <strong>v{{ app.version }}</strong>
      </button>

      <div class="actions flex gap-2">
        <button class="primary-color" :disabled="checkingUpdate" @click="checkUpdate">
          <i
            :class="[
              $fa.weight,
              checkingUpdate ? 'fa-spinner-third fa-spin' : 'fa-rotate',
            ]"
          ></i>
          <span>{{ $t('info.checkUpdate') }}</span>
        </button>
        <button @click="toggleChangelog">
          <i :class="[$fa.weight, 'fa-clock-rotate-left']"></i>
          <span>{{ $t('info.changelog') }}</span>
        </button>
      </div>

      <div v-if="showChangelog" class="changelog-panel">
        <div class="changelog-toolbar">
          <div>
            <strong>{{ $t('info.changelog') }}</strong>
            <div class="timeline-subtitle">{{ $t('info.timelineHint') }}</div>
          </div>

          <button
            class="timeline-refresh"
            type="button"
            :disabled="loadingReleases"
            :title="$t('info.refreshVersions')"
            @click="loadReleases(true)"
          >
            <i
              :class="[
                $fa.weight,
                loadingReleases ? 'fa-spinner-third fa-spin' : 'fa-rotate',
              ]"
            ></i>
          </button>
        </div>

        <div
          v-if="releaseLoadFailed"
          class="timeline-notice"
        >
          {{ $t('info.releaseListFallback') }}
        </div>

        <div class="version-timeline">
          <article
            v-for="entry in versionEntries"
            :key="entry.version"
            class="timeline-item"
            :class="{
              expanded: expandedVersions.has(entry.version),
              launch: entry.version === 'v0.0.0',
            }"
          >
            <div class="timeline-rail" aria-hidden="true">
              <span class="timeline-dot"></span>
              <span class="timeline-line"></span>
            </div>

            <div class="timeline-content">
              <div
                class="timeline-summary"
                :role="entry.version === 'v0.0.0' ? undefined : 'button'"
                :tabindex="entry.version === 'v0.0.0' ? undefined : 0"
                :aria-expanded="entry.version === 'v0.0.0' ? undefined : expandedVersions.has(entry.version)"
                @click="toggleVersion(entry.version)"
                @keydown.enter.prevent="toggleVersion(entry.version)"
                @keydown.space.prevent="toggleVersion(entry.version)"
              >
                <div class="timeline-main">
                  <div class="timeline-title-row">
                    <button
                      class="timeline-version-link"
                      type="button"
                      :title="$t('info.openRelease')"
                      @click.stop="openReleaseVersion(entry.version)"
                    >
                      {{ entry.version }}
                      <i :class="[$fa.weight, 'fa-arrow-up-right-from-square']"></i>
                    </button>

                    <span
                      v-if="entry.version === latestReleaseVersion"
                      class="version-badge latest"
                    >
                      {{ $t('info.latestVersion') }}
                    </span>
                    <span
                      v-if="entry.version === `v${app.version}`"
                      class="version-badge current"
                    >
                      {{ $t('info.currentVersion') }}
                    </span>
                  </div>

                  <div class="timeline-meta">
                    <span v-if="entry.version === 'v0.0.0'" class="timeline-launch">
                      v0.0.0上線
                    </span>
                    <span v-else class="timeline-counts">
                      <span class="timeline-count added">
                        {{ $t('info.addedCount', [entry.added.length]) }}
                      </span>
                      <span class="timeline-separator">·</span>
                      <span class="timeline-count fixed-count">
                        {{ $t('info.fixedCount', [entry.fixed.length]) }}
                      </span>
                    </span>
                    <span
                      v-if="entry.publishedAt"
                      class="timeline-date"
                    >
                      {{ formatDate(entry.publishedAt) }}
                    </span>
                  </div>
                </div>

                <i
                  v-if="entry.version !== 'v0.0.0'"
                  :class="[
                    $fa.weight,
                    'fa-chevron-down',
                    'timeline-chevron',
                    { open: expandedVersions.has(entry.version) },
                  ]"
                ></i>
              </div>

              <Transition name="timeline-expand">
                <div
                  v-if="entry.version !== 'v0.0.0' && expandedVersions.has(entry.version)"
                  class="timeline-details"
                >
                  <section v-if="entry.added.length" class="change-section">
                    <div class="change-heading">
                      <span class="change-marker added"></span>
                      <strong>{{ $t('info.added') }}</strong>
                      <span>{{ entry.added.length }}</span>
                    </div>
                    <ul>
                      <li v-for="item in entry.added" :key="`add-${item}`">
                        {{ item }}
                      </li>
                    </ul>
                  </section>

                  <section v-if="entry.fixed.length" class="change-section">
                    <div class="change-heading">
                      <span class="change-marker fixed-change"></span>
                      <strong>{{ $t('info.fixed') }}</strong>
                      <span>{{ entry.fixed.length }}</span>
                    </div>
                    <ul>
                      <li v-for="item in entry.fixed" :key="`fix-${item}`">
                        {{ item }}
                      </li>
                    </ul>
                  </section>

                  <div
                    v-if="!entry.added.length && !entry.fixed.length"
                    class="empty-version"
                  >
                    {{ $t('info.noChangelog') }}
                  </div>

                  <div class="timeline-actions">
                    <button
                      type="button"
                      @click="openReleaseVersion(entry.version)"
                    >
                      <i :class="[$fa.weight, 'fa-arrow-up-right-from-square']"></i>
                      <span>GitHub Release</span>
                    </button>

                    <button
                      class="install-version"
                      :class="{ 'primary-color': canInstall(entry) }"
                      type="button"
                      :disabled="!canInstall(entry) || Boolean(installingVersion)"
                      @click="installVersion(entry)"
                    >
                      <i
                        :class="[
                          $fa.weight,
                          installingVersion === entry.version
                            ? 'fa-spinner-third fa-spin'
                            : entry.version === `v${app.version}`
                              ? 'fa-circle-check'
                              : 'fa-circle-down',
                        ]"
                      ></i>
                      <span>
                        {{
                          installingVersion === entry.version
                            ? $t('info.installingVersion', [installProgress.toFixed(0)])
                            : entry.version === `v${app.version}`
                              ? $t('info.installedVersion')
                              : $t('info.installVersion', [entry.version])
                        }}
                      </span>
                    </button>
                  </div>
                </div>
              </Transition>
            </div>
          </article>
        </div>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { fetch } from '@tauri-apps/plugin-http';
import { openUrl } from '@tauri-apps/plugin-opener';

import { useAppStore, useComponentsStore } from '@/store';
import { AppLog } from '@/services/utils';
import changelog from '../../CHANGELOG.md?raw';
import i18n from '@/i18n';

type VersionEntry = {
  version: string;
  added: string[];
  fixed: string[];
  publishedAt?: string;
  remote: boolean;
};

type GitHubRelease = {
  tag_name: string;
  body?: string | null;
  published_at?: string | null;
  draft?: boolean;
};

type InstallProgress = {
  version: string;
  percent: number;
};

const components = useComponentsStore();
const app = useAppStore();

const checkingUpdate = ref(false);
const showChangelog = ref(false);
const loadingReleases = ref(false);
const releaseLoadFailed = ref(false);
const installingVersion = ref('');
const installProgress = ref(0);
const expandedVersions = ref(
  new Set<string>(app.version === '0.0.0' ? [] : [`v${app.version}`]),
);

let unlistenProgress: UnlistenFn | undefined;

function normalizeVersion(version: string) {
  return version.startsWith('v') ? version : `v${version}`;
}

function parseCategoryBody(body: string) {
  const added: string[] = [];
  const fixed: string[] = [];
  let category: 'added' | 'fixed' | null = null;

  for (const rawLine of body.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) continue;

    if (/^#{1,6}\s*新增\s*$/i.test(line)) {
      category = 'added';
      continue;
    }
    if (/^#{1,6}\s*修正\s*$/i.test(line)) {
      category = 'fixed';
      continue;
    }

    const bullet = line.match(/^[-*+]\s+(.+)$/)?.[1]?.trim();
    if (!bullet) continue;

    // CONest changelog has exactly two categories. Old unsectioned records are
    // mapped only as a compatibility fallback.
    if (category === 'added') added.push(bullet);
    else if (category === 'fixed') fixed.push(bullet);
    else if (/^(新增|加入|支援)/.test(bullet)) added.push(bullet);
    else fixed.push(bullet);
  }

  return { added, fixed };
}

function parseLocalChangelog() {
  const matches = [...changelog.matchAll(/^##\s+(.+)$/gm)];
  if (!matches.length) {
    const categories = parseCategoryBody(changelog);
    return [
      {
        version: `v${app.version}`,
        ...categories,
        remote: false,
      },
    ] satisfies VersionEntry[];
  }

  return matches.map((match, index) => {
    const bodyStart = (match.index ?? 0) + match[0].length;
    const end = matches[index + 1]?.index ?? changelog.length;
    return {
      version: normalizeVersion(match[1].trim()),
      ...parseCategoryBody(changelog.slice(bodyStart, end)),
      remote: false,
    } satisfies VersionEntry;
  });
}

const localEntries = parseLocalChangelog();
const remoteEntries = ref<VersionEntry[]>([]);

const versionEntries = computed(() => {
  const merged = new Map<string, VersionEntry>();

  for (const entry of remoteEntries.value) {
    merged.set(entry.version, entry);
  }
  for (const entry of localEntries) {
    const remote = merged.get(entry.version);
    merged.set(entry.version, {
      ...entry,
      ...(remote ?? {}),
      added: remote?.added.length ? remote.added : entry.added,
      fixed: remote?.fixed.length ? remote.fixed : entry.fixed,
    });
  }

  return [...merged.values()].sort((a, b) => {
    const aTime = a.publishedAt ? Date.parse(a.publishedAt) : 0;
    const bTime = b.publishedAt ? Date.parse(b.publishedAt) : 0;
    if (aTime !== bTime) return bTime - aTime;
    return b.version.localeCompare(a.version, undefined, { numeric: true });
  });
});

const latestReleaseVersion = computed(
  () => remoteEntries.value[0]?.version ?? '',
);

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(new Date(value));
}

function openReleaseVersion(version: string) {
  openUrl(
    `https://github.com/nununuuuu/CONest/releases/tag/${normalizeVersion(version)}`,
  );
}

function toggleVersion(version: string) {
  if (version === 'v0.0.0') return;
  const next = new Set(expandedVersions.value);
  if (next.has(version)) next.delete(version);
  else next.add(version);
  expandedVersions.value = next;
}

function toggleChangelog() {
  showChangelog.value = !showChangelog.value;
  if (showChangelog.value && !remoteEntries.value.length) {
    void loadReleases();
  }
}

async function loadReleases(force = false) {
  if (loadingReleases.value) return;
  if (remoteEntries.value.length && !force) return;

  loadingReleases.value = true;
  releaseLoadFailed.value = false;

  try {
    const response = await fetch(
      'https://api.github.com/repos/nununuuuu/CONest/releases?per_page=30',
      {
        headers: {
          Accept: 'application/vnd.github+json',
        },
      },
    );

    if (!response.ok) {
      throw new Error(`GitHub Releases HTTP ${response.status}`);
    }

    const releases = (await response.json()) as GitHubRelease[];
    remoteEntries.value = releases
      .filter(
        (release) =>
          !release.draft && /^v?\d+\.\d+\.\d+/.test(release.tag_name),
      )
      .map((release) => ({
        version: normalizeVersion(release.tag_name),
        ...parseCategoryBody(release.body?.trim() || ''),
        publishedAt: release.published_at ?? undefined,
        remote: true,
      }));
  } catch (error) {
    releaseLoadFailed.value = true;
    console.warn('Failed to load CONest releases', error);
  } finally {
    loadingReleases.value = false;
  }
}

function canInstall(entry: VersionEntry) {
  return entry.version !== `v${app.version}` && entry.remote;
}

async function installVersion(entry: VersionEntry) {
  if (!canInstall(entry) || installingVersion.value) return;

  installingVersion.value = entry.version;
  installProgress.value = 0;

  try {
    await invoke('install_release_version', {
      version: entry.version,
    });
  } catch (error) {
    AppLog(
      i18n.global.t('info.installVersionFailed', [String(error)]),
      'error',
    );
    installingVersion.value = '';
  }
}

async function checkUpdate() {
  if (checkingUpdate.value) return;
  checkingUpdate.value = true;
  try {
    await components.c.updater?.check(true);
  } finally {
    checkingUpdate.value = false;
  }
}

onMounted(async () => {
  unlistenProgress = await listen<InstallProgress>(
    'version-install-progress',
    (event) => {
      if (normalizeVersion(event.payload.version) !== installingVersion.value) {
        return;
      }
      installProgress.value = event.payload.percent ?? 0;
    },
  );
});

onBeforeUnmount(() => {
  unlistenProgress?.();
});
</script>

<style scoped>
.info-page {
  width: 100%;
  height: 100%;
  overflow-y: auto;
}

.version-link {
  width: fit-content;
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--content-color);
}

.version-link strong {
  color: var(--primary-color);
  font-weight: 600;
  text-shadow:
    0 0 4px color-mix(in srgb, var(--primary-color) 90%, transparent),
    0 0 10px color-mix(in srgb, var(--primary-color) 65%, transparent),
    0 0 18px color-mix(in srgb, var(--primary-color) 38%, transparent);
}

.version-link:hover {
  background: transparent;
  filter: brightness(1.08);
}

.changelog-panel {
  width: 100%;
  max-width: 46rem;
  border: 1px solid var(--split-color);
  border-radius: 0.9rem;
  background: var(--block-color);
  overflow: hidden;
}

.changelog-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  padding: 0.85rem 1rem;
  border-bottom: 1px solid var(--split-color);
}

.timeline-subtitle {
  margin-top: 0.15rem;
  color: var(--desc-color);
  font-size: 0.72rem;
  font-weight: 400;
}

.timeline-refresh {
  width: 2.15rem;
  height: 2.15rem;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  padding: 0;
  border-radius: 0.55rem;
  background: var(--solid-block-color);
  color: var(--desc-color);
}

.timeline-refresh:hover {
  color: var(--primary-color);
}

.timeline-notice {
  margin: 0.75rem 1rem 0;
  padding: 0.6rem 0.75rem;
  border: 1px solid var(--split-color);
  border-radius: 0.6rem;
  color: var(--desc-color);
  font-size: 0.76rem;
  background: var(--solid-block-color);
}

.version-timeline {
  max-height: 31rem;
  overflow-y: auto;
  padding: 0.8rem 1rem 1rem;
}

.timeline-item {
  position: relative;
  display: grid;
  grid-template-columns: 1.25rem minmax(0, 1fr);
}

.timeline-rail {
  position: relative;
  display: flex;
  justify-content: center;
}

.timeline-dot {
  position: relative;
  z-index: 2;
  width: 0.62rem;
  height: 0.62rem;
  margin-top: 1.05rem;
  border: 2px solid var(--block-color);
  border-radius: 999px;
  background: var(--desc-color);
  box-shadow: 0 0 0 1px var(--split-color);
}

.timeline-item.expanded .timeline-dot {
  background: var(--primary-color);
  box-shadow:
    0 0 0 1px var(--primary-color),
    0 0 0.75rem color-mix(in srgb, var(--primary-color) 55%, transparent);
}

.timeline-line {
  position: absolute;
  top: 1.6rem;
  bottom: -0.2rem;
  width: 1px;
  background: var(--split-color);
}

.timeline-item:last-child .timeline-line {
  display: none;
}

.timeline-content {
  min-width: 0;
  padding: 0 0 0.5rem 0.35rem;
}

.timeline-summary {
  display: flex;
  align-items: center;
  height: auto;
  overflow: visible;
  gap: 0.7rem;
  min-height: 3.2rem;
  padding: 0.65rem 0.75rem;
  border: 1px solid transparent;
  border-radius: 0.7rem;
  cursor: pointer;
  transition:
    background 0.14s ease,
    border-color 0.14s ease;
}

.timeline-summary:hover,
.timeline-item.expanded .timeline-summary {
  border-color: var(--split-color);
  background:
    color-mix(in srgb, var(--primary-color) 5%, var(--solid-block-color));
}

.timeline-main {
  min-width: 0;
  flex: 1;
}

.timeline-title-row,
.timeline-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
}

.timeline-title-row {
  gap: 0.4rem;
}

.timeline-meta {
  gap: 0.55rem;
  margin-top: 0.28rem;
  color: var(--desc-color);
  font-size: 0.75rem;
  flex-wrap: nowrap;
  line-height: 1.2rem;
}

.timeline-item.launch .timeline-summary {
  cursor: default;
}

.timeline-launch {
  color: var(--primary-color);
  font-weight: 650;
  white-space: nowrap;
}

.timeline-counts {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  min-width: 0;
  white-space: nowrap;
}

.timeline-version-link {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--content-color);
  font-size: 1rem;
  font-weight: 700;
}

.timeline-version-link:hover {
  color: var(--primary-color);
  background: transparent;
  text-decoration: underline;
}

.timeline-version-link i {
  font-size: 0.65rem;
  color: var(--desc-color);
}

.timeline-count.added {
  color: var(--primary-color);
}

.timeline-count.fixed-count {
  color: var(--content-color);
}

.timeline-separator {
  opacity: 0.55;
}

.timeline-date {
  margin-left: auto;
  flex: 0 0 auto;
  white-space: nowrap;
}

.timeline-chevron {
  flex: 0 0 auto;
  color: var(--desc-color);
  transition: transform 0.16s ease;
}

.timeline-chevron.open {
  transform: rotate(180deg);
  color: var(--primary-color);
}

.version-badge {
  display: inline-flex;
  align-items: center;
  padding: 0.08rem 0.35rem;
  border-radius: 999px;
  font-size: 0.63rem;
  font-weight: 650;
  white-space: nowrap;
}

.version-badge.current {
  color: var(--content-color);
  background: var(--solid-block-color);
  border: 1px solid var(--split-color);
}

.version-badge.latest {
  color: var(--primary-color);
  background: color-mix(in srgb, var(--primary-color) 12%, transparent);
}

.timeline-details {
  margin: 0 0.2rem 0.65rem;
  padding: 0.75rem 0.9rem 0.85rem;
  border: 1px solid var(--split-color);
  border-radius: 0.7rem;
  background: var(--solid-block-color);
}

.change-section + .change-section {
  margin-top: 0.9rem;
  padding-top: 0.8rem;
  border-top: 1px solid var(--split-color);
}

.change-heading {
  display: flex;
  align-items: center;
  gap: 0.42rem;
  margin-bottom: 0.42rem;
  font-size: 0.82rem;
}

.change-heading > span:last-child {
  min-width: 1.15rem;
  height: 1.15rem;
  display: inline-grid;
  place-items: center;
  border-radius: 999px;
  color: var(--desc-color);
  background: var(--block-color);
  font-size: 0.66rem;
}

.change-marker {
  width: 0.45rem;
  height: 0.45rem;
  border-radius: 999px;
}

.change-marker.added {
  background: var(--primary-color);
}

.change-marker.fixed-change {
  background: var(--desc-color);
}

.change-section ul {
  margin: 0;
  padding-left: 1.15rem;
}

.change-section li {
  margin: 0.2rem 0;
  color: var(--content-color);
  font-size: 0.82rem;
  line-height: 1.4rem;
}

.timeline-actions {
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 0.55rem;
  margin-top: 0.9rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--split-color);
}

.timeline-actions button {
  width: fit-content;
}

.empty-version {
  color: var(--desc-color);
  font-size: 0.8rem;
}

.timeline-expand-enter-active,
.timeline-expand-leave-active {
  transition:
    opacity 0.16s ease,
    transform 0.16s ease;
}

.timeline-expand-enter-from,
.timeline-expand-leave-to {
  opacity: 0;
  transform: translateY(-0.3rem);
}
</style>
