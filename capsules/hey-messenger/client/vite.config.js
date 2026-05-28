import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Built as a runtime browser capsule mounted at /apps/hey-messenger/
// (and /elastos/apps/hey-messenger/ under YunoHost subpath installs).
// base: "./" emits relative asset URLs in index.html so the same dist
// works under every mount path without rebuild — matches hey-social.
export default defineConfig({
  base: "./",
  plugins: [react()],
  server: {
    port: 5174,
    proxy: {
      "/api/provider": {
        target: "http://127.0.0.1:3000",
        changeOrigin: true,
      },
    },
  },
});
