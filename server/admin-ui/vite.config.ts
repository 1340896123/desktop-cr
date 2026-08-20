import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  // 相对路径,便于服务端任意子路径托管
  base: './',
  server: {
    port: 5174,
    proxy: {
      // 开发模式下代理到本地 dcr-signal 管理端口
      '/api': 'http://localhost:21120',
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});