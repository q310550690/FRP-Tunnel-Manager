import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

import "element-plus/theme-chalk/dark/css-vars.css";
import "./styles/tokens.css";

createApp(App).use(createPinia()).mount("#app");
