import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  root: 'src',
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    // 多页面入口:主窗口 + 独立文件传输窗口(相对项目根目录)
    rollupOptions: {
      input: {
        main: 'src/index.html',
        transfer: 'src/transfer.html',
      },
    },
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
    // 允许 monkeycode 预览域名访问开发服务器
    allowedHosts: ['.monkeycode-ai.online'],
    // 如有后端代理需求可启用，例如：
    // proxy: {
    //   '/api': {
    //     target: 'http://localhost:3001',
    //     changeOrigin: true,
    //   },
    // },
  },
});
