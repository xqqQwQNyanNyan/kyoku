import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
    // 开发服务和测试需要读取随应用内置的牌谱与许可。
    fs: {
      allow: ['.', '../fixtures/tenhou'],
    },
    // Rust 构建产物不应触发前端重载，避免开发时清空复盘状态。
    watch: { ignored: ['**/src-tauri/**'] },
  },
  clearScreen: false,
});
