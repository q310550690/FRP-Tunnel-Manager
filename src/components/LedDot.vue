<script setup lang="ts">
import { computed } from "vue";
import type { ServiceState } from "@/types";

const props = withDefaults(
  defineProps<{ state: ServiceState; size?: number; pulse?: boolean }>(),
  { size: 8, pulse: true }
);

const color = computed(() => {
  switch (props.state) {
    case "running":
      return "var(--accent)";
    case "starting":
    case "stopping":
      return "var(--amber)";
    case "error":
      return "var(--red)";
    default:
      return "var(--text-faint)";
  }
});

/** 停止 / 未安装：平面的灰灯，不带光晕 */
const dim = computed(() => props.state === "stopped" || props.state === "not-installed");

const animated = computed(
  () =>
    props.pulse &&
    !dim.value &&
    (props.state === "running" || props.state === "starting" || props.state === "error")
);
</script>

<template>
  <span
    class="led"
    :class="{ animated, dim, blink: state === 'starting' || state === 'stopping' }"
    :style="{ width: size + 'px', height: size + 'px', '--led': color }"
  />
</template>

<style scoped>
/* 平面灯：纯色圆点 + 向外扩散的光晕，没有高光、没有内阴影、没有立体感 */
.led {
  display: inline-block;
  flex: none;
  border-radius: 50%;
  background: var(--led);
  box-shadow: 0 0 7px 1px color-mix(in srgb, var(--led) 62%, transparent);
}
.led.dim {
  box-shadow: none;
  opacity: 0.6;
}
.led.animated {
  animation: led-glow 2.4s ease-in-out infinite;
}
.led.blink {
  animation: led-blink 1s steps(2, start) infinite;
}
@keyframes led-glow {
  0%,
  100% {
    box-shadow: 0 0 5px 0 color-mix(in srgb, var(--led) 48%, transparent);
  }
  50% {
    box-shadow: 0 0 10px 2px color-mix(in srgb, var(--led) 72%, transparent);
  }
}
@keyframes led-blink {
  50% {
    opacity: 0.35;
  }
}
@media (prefers-reduced-motion: reduce) {
  .led.animated,
  .led.blink {
    animation: none;
  }
}
</style>
