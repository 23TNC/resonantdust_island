import { defineConfig } from 'vite'

export default defineConfig({
  server: {
    // Bind 0.0.0.0 so Chrome on Windows can reach the server running in WSL.
    // Load it at http://localhost:5173 all the same — WSL2 forwards to the
    // Windows loopback, and localhost is a secure context where a bare IP is
    // not. WebGPU is unavailable without one.
    host: true,
    port: 5173,
    strictPort: true,
  },
  // Keep the wasm as its own file. Inlining a multi-megabyte module as a
  // base64 data URI would balloon the bundle and defeat streaming compilation.
  build: {
    assetsInlineLimit: 0,
    target: 'es2022',
  },
})
