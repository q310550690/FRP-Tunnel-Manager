import type { t } from "./index";

declare module "vue" {
  interface ComponentCustomProperties {
    $t: typeof t;
  }
}

export {};
