<script setup lang="ts">
import { computed } from "vue";
import { MorphIcon } from "morphicons/vue";
import type { IconNode } from "morphicons";
import { ICONS, type IconName } from "@/icons";

const props = withDefaults(
  defineProps<{
    name: IconName;
    size?: number;
    strokeWidth?: number;
  }>(),
  { size: 14, strokeWidth: 1.7 }
);

const icon = computed(() => ICONS[props.name] as unknown as IconNode);
</script>

<template>
  <MorphIcon
    :icon="icon"
    :size="size"
    :stroke-width="strokeWidth"
    reduced-motion="user"
    class="app-icon"
  />
</template>

<style scoped>
.app-icon {
  display: block;
  flex: none;
}
</style>
