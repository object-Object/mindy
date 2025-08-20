import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
    plugins: [react()],
    optimizeDeps: {
        exclude: ["mindy-website"],
    },
    // https://github.com/vitejs/vite/discussions/7920#discussioncomment-4803178
    esbuild: {
        pure: mode === "production" ? ["console.log", "console.debug"] : [],
    },
}));
