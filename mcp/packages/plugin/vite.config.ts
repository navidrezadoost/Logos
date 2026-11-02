import { defineConfig } from "vite";
import livePreview from "vite-live-preview";

let WS_URI = process.env.WS_URI || "ws://127.0.0.1:4402";
const pluginAllowedOrigin = process.env.LOGOS_MCP_PLUGIN_ALLOWED_ORIGIN;

if (pluginAllowedOrigin) {
    const parsedOrigin = new URL(pluginAllowedOrigin);
    if (
        parsedOrigin.origin !== pluginAllowedOrigin ||
        !["http:", "https:"].includes(parsedOrigin.protocol)
    ) {
        throw new Error("LOGOS_MCP_PLUGIN_ALLOWED_ORIGIN must be one exact http(s) origin");
    }
}

console.log("Will define LOGOS_MCP_WEBSOCKET_URL as:", JSON.stringify(WS_URI));

export default defineConfig({
    base: "./",
    plugins: [
        livePreview({
            reload: true,
            config: {
                build: {
                    sourcemap: true,
                },
            },
        }),
    ],
    build: {
        rollupOptions: {
            input: {
                plugin: "src/plugin.ts",
                index: "./index.html",
            },
            output: {
                entryFileNames: "[name].js",
            },
        },
    },
    preview: {
        host: "127.0.0.1",
        port: 4400,
        cors: pluginAllowedOrigin ? { origin: pluginAllowedOrigin } : false,
        allowedHosts: [],
    },
    define: {
        LOGOS_MCP_WEBSOCKET_URL: JSON.stringify(WS_URI),
        LOGOS_MCP_LOCAL_TOKEN: JSON.stringify(process.env.LOGOS_MCP_LOCAL_TOKEN ?? ""),
    },
});
