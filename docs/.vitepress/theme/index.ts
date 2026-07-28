import DefaultTheme from "vitepress/theme";
import type { EnhanceAppContext } from "vitepress";
import "./custom.css";
import SmartDownload from "./components/SmartDownload.vue";

export default {
  extends: DefaultTheme,
  enhanceApp({ app }: EnhanceAppContext) {
    app.component("SmartDownload", SmartDownload);
  },
};
