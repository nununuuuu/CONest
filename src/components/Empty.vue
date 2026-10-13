<template>
  <div class="workspace-empty" role="status">
    <span class="workspace-empty-mark" aria-hidden="true">＋</span>
    <p>{{ text }}</p>
  </div>
</template>
<script setup lang="ts">
defineProps<{ text: string }>();
</script>
