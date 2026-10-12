<template>
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-earth-america']"></i>
      <span>{{ $t('settings.language.name') }}</span>
    </h3>
    <Dropdown v-model="settings.language" :drop="locales" />
  </section>
  <section>
    <h3>
      <i class="fa-solid fa-circle-half-stroke"></i>
      <span>{{ $t('settings.theme.name') }}</span>
    </h3>
    <Dropdown
      v-model="settings.theme"
      :drop="
        ['dark', 'light', 'auto'].map((id) => ({
          id,
          name: $t('settings.theme.' + id),
        }))
      "
    />
  </section>
  <section>
    <h3>
      <i class="fa-solid fa-window-maximize"></i>
      <span>{{ $t('settings.window_effect.name') }}</span>
    </h3>
    <Dropdown
      v-model="settings.window_effect"
      :drop="
        ['auto', 'mica', 'acrylic', 'sidebar', 'none'].map((id) => ({
          id,
          name: $t('settings.window_effect.' + id),
        }))
      "
    />
  </section>
  <hr />
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-clipboard-check']"></i>
      <span>{{ $t('settings.clipboard.name') }}</span>
    </h3>
    <Switch v-model="settings.clipboard" />
    <span class="desc">{{ $t('settings.clipboard.desc') }}</span>
  </section>
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-hand-pointer']"></i>
      <span>{{ $t('settings.drag_search.name') }}</span>
    </h3>
    <Switch v-model="settings.drag_search" />
    <span class="desc">{{ $t('settings.drag_search.desc') }}</span>
  </section>
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-bell']"></i>
      <span>{{ $t('settings.notify.name') }}</span>
    </h3>
    <Switch v-model="settings.notify" />
    <span class="desc">{{ $t('settings.notify.desc') }}</span>
  </section>
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-arrow-down-from-arc']"></i>
      <span>{{ $t('settings.auto_download.name') }}</span>
    </h3>
    <Switch v-model="settings.auto_download" />
    <span class="desc">{{ $t('settings.auto_download.desc') }}</span>
  </section>
  <hr />
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-power-off']"></i>
      <span>{{ $t('settings.auto_start.name') }}</span>
    </h3>
    <Switch v-model="settings.auto_start" />
    <span class="desc">{{ $t('settings.auto_start.desc') }}</span>
  </section>
  <section v-if="settings.auto_start">
    <h3>
      <i :class="[$fa.weight, 'fa-window-restore']"></i>
      <span>{{ $t('settings.startup_mode.name') }}</span>
    </h3>
    <Dropdown
      v-model="settings.startup_mode"
      :drop="
        ['background', 'window'].map((id) => ({
          id,
          name: $t('settings.startup_mode.' + id),
        }))
      "
    />
  </section>
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-xmark']"></i>
      <span>{{ $t('settings.close_behavior.name') }}</span>
    </h3>
    <Dropdown
      v-model="settings.close_behavior"
      :drop="
        ['exit', 'minimize'].map((id) => ({
          id,
          name: $t('settings.close_behavior.' + id),
        }))
      "
    />
    <span class="desc">{{ $t('settings.close_behavior.desc') }}</span>
  </section>
  <hr />
  <section>
    <h3>
      <i :class="[$fa.weight, 'fa-up-from-line']"></i>
      <span>{{ $t('settings.auto_check_update.name') }}</span>
    </h3>
    <Switch v-model="settings.auto_check_update" />
  </section>
</template>

<script lang="ts" setup>
import { locales } from '@/i18n';
import { useSettingsStore } from '@/store';
import { Dropdown, Switch } from '@/components';

const settings = useSettingsStore();
</script>
