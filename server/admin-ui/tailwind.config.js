/**
 * 设计 Token —— 源自 E:\Desktop\ui_prototype_design.html 高保真原型,作为 UI 改造的硬约束。
 *
 * 色彩(Color):
 *   brand   50 #f0f7ff / 100 #e0effe / 200 #bae0fd / 500 #0284c7 / 600 #0284c7 / 700 #0369a1
 *   slate   50 #f8fafc / 100 #f1f5f9 / 200 #e2e8f0 / 300 #cbd5e1 / 400 #94a3b8
 *           500 #64748b / 600 #475569 / 700 #334155 / 800 #1e293b / 900 #0f172a
 *   emerald 500 #10b981(成功/上升)    rose 500 #f43f5e(危险/下降)
 *   amber   400/500 #f59e0b(进行中)   purple 500 #8b5cf6(辅助)
 * 深色模式:html.dark 切换,背景 slate-900 / 卡片 slate-800 / 边框 slate-700 / 激活态 brand-500/10。
 *
 * 圆角(Radius): 卡片 rounded-2xl(16px) · 按钮/输入/下拉 rounded-xl(12px) · 分页 rounded-lg(8px) · Logo rounded-xl
 * 阴影(Shadow): 卡片 shadow-sm → hover:shadow-md · 主按钮 shadow-md shadow-brand-500/20 · Logo shadow-lg shadow-brand-500/30
 * 排版(Typo):   页标题 text-xl font-bold · KPI 标签 text-xs uppercase tracking-wider · KPI 值 text-2xl font-bold
 *               卡片标题 font-bold · 表头 text-xs font-semibold uppercase tracking-wider · 表体 text-sm
 * 间距(Spacing): 主区 p-6 space-y-6 · KPI 网格 gap-5 · 卡片内边距 p-5/p-6 · 导航项 px-3 py-3 · 单元格 p-4
 * 布局(Layout):  侧边栏 w-64 / 折叠 w-20 · 顶栏 h-16 px-6 · 主区 flex-1 overflow-y-auto
 * 交互:          主按钮 hover:bg-brand-700 active:scale-95 · 导航激活 bg-brand-50 text-brand-600 font-semibold
 *                图标 hover:scale-110 · 状态胶囊含同色圆点
 */
/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        brand: {
          50: '#f0f7ff',
          100: '#e0effe',
          200: '#bae0fd',
          500: '#0284c7',
          600: '#0284c7',
          700: '#0369a1',
        },
      },
      boxShadow: {
        'brand-sm': '0 1px 3px rgba(2, 132, 199, 0.12)',
        'brand-lg': '0 10px 15px -3px rgba(2, 132, 199, 0.30), 0 4px 6px -4px rgba(2, 132, 199, 0.30)',
      },
      fontFamily: {
        sans: ['"Segoe UI"', '"Microsoft YaHei UI"', '"Microsoft YaHei"', 'system-ui', 'sans-serif'],
      },
    },
  },
  plugins: [],
};
