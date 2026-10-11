import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [
    react(),
    {
      name: "admin-dev-entry",
      apply: "serve",
      transformIndexHtml: {
        order: "pre",
        handler: (html) =>
          html.replace(
            "../src/admin/main.tsx",
            "/@fs/" + resolve("src/admin/main.tsx").replaceAll("\\", "/"),
          ),
      },
    },
  ],
  root: "admin",
  base: "/admin/",
  publicDir: false,
  server: {
    host: "127.0.0.1",
    port: 1422,
    proxy: {
      "/v1": {
        target: process.env.ADMIN_API_URL || "http://127.0.0.1:8089",
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: "../server/internal/api/adminassets/web",
    emptyOutDir: true,
  },
});
