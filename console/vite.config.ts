import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
export default defineConfig({
  plugins: [react()],
  base: '/assets/member-console/',
  build: {
    manifest: true,
    assetsInlineLimit: 0,
    sourcemap: false,
    rolldownOptions: {
      output: {
        entryFileNames: 'app.js',
        chunkFileNames: '[name].js',
        assetFileNames: '[name][extname]',
      },
    },
  },
});
