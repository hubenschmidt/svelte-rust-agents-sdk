import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';

export default defineConfig({
  plugins: [solid()],
  server: {
    port: 3001,
    strictPort: true
  },
  optimizeDeps: {
    include: ['@dagrejs/dagre', '@dagrejs/graphlib']
  }
});
