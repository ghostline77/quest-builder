import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
const target = process.env.GHOSTLINE_UI_BACKEND || "http://127.0.0.1:8790";
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    {
      name: "local-editor-origin",
      configureServer(server) {
        server.middlewares.use((request, response, next) => {
          const origin = request.headers.origin;
          if (origin) {
            try {
              if (new URL(origin).host !== request.headers.host) {
                response.statusCode = 403;
                response.end("Cross-origin editor request rejected");
                return;
              }
            } catch {
              response.statusCode = 403;
              response.end("Invalid editor origin");
              return;
            }
          }
          next();
        });
      },
    },
  ],
  server: {
    host: "127.0.0.1",
    port: Number(process.env.GHOSTLINE_UI_PORT) || 8890,
    strictPort: true,
    fs: { allow: ["../.."] },
    proxy: Object.fromEntries(
      ["/api", "/media", "/output", "/assets/inter.woff2", "/assets/icon-"].map(
        (path) => [
          path,
          {
            target,
            changeOrigin: true,
            configure(proxy) {
              proxy.on("proxyReq", (forward, request) => {
                if (request.headers.origin) forward.setHeader("origin", target);
              });
            },
          },
        ],
      ),
    ),
  },
  build: {
    target: "es2022",
    cssCodeSplit: false,
    assetsInlineLimit: 1_000_000,
    rollupOptions: {
      output: {
        entryFileNames: "assets/app.js",
        assetFileNames: (asset) =>
          asset.names?.some((name) => name.endsWith(".css"))
            ? "assets/style.css"
            : "assets/[name][extname]",
        inlineDynamicImports: true,
      },
    },
  },
});
