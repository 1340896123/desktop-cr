import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import {
  ArcElement,
  CategoryScale,
  Chart,
  DoughnutController,
  Filler,
  Legend,
  LineController,
  LineElement,
  LinearScale,
  PointElement,
  Tooltip,
} from 'chart.js';

Chart.register(
  LineController,
  LineElement,
  PointElement,
  LinearScale,
  CategoryScale,
  DoughnutController,
  ArcElement,
  Filler,
  Tooltip,
  Legend,
);

// ---------------------------------------------------------------------------
// 类型与 API
// ---------------------------------------------------------------------------

interface User {
  username: string;
  created_at: string;
}

interface Peer {
  id: string;
  lan: string;
  external: string;
}

interface Stats {
  users: number;
  peersOnline: number;
}

const TOKEN_KEY = 'dcr_admin_token';
const THEME_KEY = 'dcr_admin_theme';

function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? '';
}

async function api<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  const token = getToken();
  if (token) headers['Authorization'] = `Bearer ${token}`;
  const res = await fetch(path, { ...options, headers });
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    const msg = typeof body.error === 'string' ? body.error : `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return body as T;
}

// ---------------------------------------------------------------------------
// 图标(内联 SVG,离线可用,风格对齐原型 FontAwesome)
// ---------------------------------------------------------------------------

const ICON_PATHS: Record<string, string[]> = {
  cube: ['M12 2.5l8.5 4.75v9.5L12 21.5l-8.5-4.75v-9.5L12 2.5z', 'M12 12l8.5-4.75', 'M12 12v9.5', 'M3.5 7.25L12 12'],
  pie: ['M12 3.5a8.5 8.5 0 1 1-8.5 8.5h8.5V3.5z', 'M12 12l8.5-4.25'],
  users: [
    'M15.5 12a3.25 3.25 0 1 0 0-6.5 3.25 3.25 0 0 0 0 6.5z',
    'M20.5 18.5a6.5 6.5 0 0 0-10 0',
    'M8.5 9.75a2.75 2.75 0 1 1 0-5.5 2.75 2.75 0 0 1 0 5.5z',
    'M3.75 18.5a4.75 4.75 0 0 1 9.5 0',
  ],
  device: ['M3.5 5h17v12h-17z', 'M8.5 20h7', 'M12 17v3'],
  gear: [
    'M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7z',
    'M12 2.5v2.2',
    'M12 19.3v2.2',
    'M4.2 7.7l1.9 1.1',
    'M17.9 15.2l1.9 1.1',
    'M4.2 16.3l1.9-1.1',
    'M17.9 8.8l1.9-1.1',
  ],
  search: ['M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14z', 'M16.5 16.5L21 21'],
  bell: [
    'M12 3.5a5.5 5.5 0 0 0-5.5 5.5v3.2l-1.9 3.3h14.8L17.5 12.2V9A5.5 5.5 0 0 0 12 3.5z',
    'M10 19.5a2 2 0 0 0 4 0',
  ],
  sun: ['M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z', 'M12 2v2', 'M12 20v2', 'M4.9 4.9l1.4 1.4', 'M17.7 17.7l1.4 1.4', 'M2 12h2', 'M20 12h2', 'M4.9 19.1l1.4-1.4', 'M17.7 6.3l1.4-1.4'],
  moon: ['M20.5 14.5A8.5 8.5 0 1 1 9.5 3.5a7 7 0 0 0 11 11z'],
  plus: ['M12 5v14', 'M5 12h14'],
  'angle-left': ['M14.5 5.5L8 12l6.5 6.5'],
  'angle-right': ['M9.5 5.5L16 12l-6.5 6.5'],
  'chevron-right': ['M9 6l6 6-6 6'],
  download: ['M12 3.5v11', 'M7.5 10L12 14.5 16.5 10', 'M4.5 20.5h15'],
  eye: ['M3.5 12S7 5.5 12 5.5 20.5 12 20.5 12 17 18.5 12 18.5 3.5 12 3.5 12z', 'M12 14.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z'],
  trash: ['M4.5 7h15', 'M9 7V4.5h6V7', 'M6.5 7l1 13h9l1-13', 'M10 10.5v6', 'M14 10.5v6'],
  key: ['M14.5 14.5a4.5 4.5 0 1 1-2-3.7', 'M10.5 10.5l8 8', 'M16 13.5l2 2'],
  xmark: ['M6 6l12 12', 'M18 6L6 18'],
  'arrow-up': ['M12 19.5v-15', 'M6 10l6-6 6 6'],
  'arrow-down': ['M12 4.5v15', 'M6 14l6 6 6-6'],
  'check-circle': ['M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z', 'M8.5 12l2.5 2.5 4.5-5'],
  'x-circle': ['M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z', 'M9.5 9.5l5 5', 'M14.5 9.5l-5 5'],
  refresh: ['M20 12a8 8 0 1 1-2.34-5.66', 'M20 4.5V8h-3.5'],
  server: ['M4 5h16v5H4z', 'M4 14h16v5H4z', 'M7 7.5h.01', 'M7 16.5h.01'],
  shield: ['M12 3l7 2.5v5c0 4.5-3 8-7 9.5-4-1.5-7-5-7-9.5v-5L12 3z'],
  logout: ['M15 4h4v16h-4', 'M10 8l-4 4 4 4', 'M6 12h10'],
};

function Icon({ name, className = 'w-5 h-5' }: { name: string; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {(ICON_PATHS[name] ?? []).map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

// ---------------------------------------------------------------------------
// 状态胶囊(样式对齐原型:彩色圆点 + 浅底文字)
// ---------------------------------------------------------------------------

const PILL_STYLES: Record<string, { wrap: string; dot: string }> = {
  正常: { wrap: 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-400', dot: 'bg-emerald-500' },
  在线: { wrap: 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-400', dot: 'bg-emerald-500' },
  离线: { wrap: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-300', dot: 'bg-slate-400' },
  进行中: { wrap: 'bg-amber-50 text-amber-600 dark:bg-amber-900/30 dark:text-amber-400', dot: 'bg-amber-500' },
  已完成: { wrap: 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-400', dot: 'bg-emerald-500' },
  待处理: { wrap: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-300', dot: 'bg-slate-400' },
};

function StatusPill({ status }: { status: string }) {
  const s = PILL_STYLES[status] ?? PILL_STYLES['待处理'];
  return (
    <span className={`inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium ${s.wrap}`}>
      <span className={`w-1.5 h-1.5 rounded-full mr-1.5 ${s.dot}`} />
      {status}
    </span>
  );
}

// ---------------------------------------------------------------------------
// 通用弹窗 / Toast
// ---------------------------------------------------------------------------

function Modal({
  open,
  onClose,
  maxW = 'max-w-lg',
  children,
}: {
  open: boolean;
  onClose: () => void;
  maxW?: string;
  children: ReactNode;
}) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 backdrop-blur-sm" onClick={onClose}>
      <div
        className={`bg-white dark:bg-slate-800 rounded-2xl shadow-2xl border border-slate-100 dark:border-slate-700 w-full ${maxW} p-6 space-y-5`}
        onClick={(e) => e.stopPropagation()}
      >
        {children}
      </div>
    </div>
  );
}

type ToastType = 'success' | 'error';

interface ToastState {
  type: ToastType;
  message: string;
}

// ---------------------------------------------------------------------------
// 登录页(沿用原型品牌 Logo 瓦片 + 卡片样式)
// ---------------------------------------------------------------------------

function LoginPage({
  onLogin,
}: {
  onLogin: (token: string, username: string) => void;
}) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);

  const doLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr('');
    setBusy(true);
    try {
      const r = await api<{ token: string; username: string }>('/api/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username, password }),
      });
      localStorage.setItem(TOKEN_KEY, r.token);
      onLogin(r.token, r.username);
    } catch (error) {
      setErr(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex min-h-screen items-center justify-center bg-slate-50 dark:bg-slate-900 p-6">
      <div className="w-full max-w-sm space-y-6">
        <div className="text-center">
          <div className="mx-auto flex items-center justify-center w-12 h-12 rounded-xl bg-brand-600 text-white shadow-brand-lg">
            <Icon name="cube" className="w-6 h-6" />
          </div>
          <h1 className="mt-3 text-xl font-bold text-slate-800 dark:text-white">dcr-signal 管理后台</h1>
          <p className="mt-1 text-xs text-slate-400">账号登录后管理用户与在线设备</p>
        </div>

        <form
          onSubmit={doLogin}
          className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 shadow-sm p-6 space-y-4"
        >
          <h2 className="font-bold text-slate-800 dark:text-white">登录</h2>
          {err && (
            <div className="flex items-center gap-2 text-rose-600 dark:text-rose-400 text-sm">
              <Icon name="x-circle" className="w-4 h-4" />
              {err}
            </div>
          )}
          <div>
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-300 mb-1">用户名</label>
            <input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="admin"
              autoComplete="username"
              autoFocus
              className="w-full px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500"
            />
          </div>
          <div>
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-300 mb-1">密码</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••"
              autoComplete="current-password"
              className="w-full px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500"
            />
          </div>
          <button
            type="submit"
            disabled={busy}
            className="w-full py-2 bg-brand-600 hover:bg-brand-700 text-white rounded-xl font-medium text-sm shadow-md shadow-brand-500/20 transition active:scale-95 disabled:opacity-60"
          >
            {busy ? '登录中…' : '登录'}
          </button>
        </form>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 通用数据表格卡片(标题徽标 / 筛选 / 搜索 / 导出 / 分页)
// ---------------------------------------------------------------------------

interface Col<T> {
  header: string;
  cell: (row: T) => ReactNode;
}

function DataTable<T>({
  title,
  subtitle,
  badge,
  columns,
  rows,
  rowKey,
  status,
  statusOptions,
  searchFields,
  exportName,
  exportRow,
  actions,
  emptyText,
  externalQuery = '',
}: {
  title: string;
  subtitle?: string;
  badge: number;
  columns: Col<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  status?: (row: T) => string;
  statusOptions?: { value: string; label: string }[];
  searchFields?: (row: T) => string;
  exportName: string;
  exportRow: (row: T) => string[];
  actions?: (row: T) => ReactNode;
  emptyText: string;
  externalQuery?: string;
}) {
  const [statusFilter, setStatusFilter] = useState('ALL');
  const [page, setPage] = useState(1);
  const pageSize = 5;

  const q = externalQuery.trim().toLowerCase();
  const filtered = rows.filter((row) => {
    const matchesStatus = statusFilter === 'ALL' || (status ? status(row) === statusFilter : true);
    const haystack = (searchFields ? searchFields(row) : '').toLowerCase();
    const matchesSearch = q === '' || haystack.includes(q);
    return matchesStatus && matchesSearch;
  });

  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const cur = Math.min(page, pages);
  const start = (cur - 1) * pageSize;
  const end = Math.min(start + pageSize, filtered.length);
  const pageRows = filtered.slice(start, end);

  useEffect(() => {
    if (page > pages) setPage(pages);
  }, [page, pages]);

  const doExport = () => {
    const lines = [
      columns.map((c) => c.header),
      ...filtered.map((row) => exportRow(row)),
    ]
      .map((row) => row.map((v) => `"${String(v).replace(/"/g, '""')}"`).join(','))
      .join('\n');
    const blob = new Blob(['\ufeff' + lines], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${exportName}-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  return (
    <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm overflow-hidden">
      <div className="p-5 border-b border-slate-100 dark:border-slate-700 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center space-x-3">
          <h3 className="font-bold text-slate-800 dark:text-white">{title}</h3>
          <span className="px-2.5 py-0.5 text-xs font-semibold bg-brand-50 text-brand-600 dark:bg-brand-900/30 dark:text-brand-400 rounded-full">
            {badge} 条记录
          </span>
        </div>
        {subtitle && <p className="text-xs text-slate-400 md:hidden">{subtitle}</p>}

        <div className="flex flex-wrap items-center gap-3">
          {statusOptions && (
            <select
              value={statusFilter}
              onChange={(e) => {
                setStatusFilter(e.target.value);
                setPage(1);
              }}
              className="text-xs bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl px-3 py-2 focus:ring-2 focus:ring-brand-500"
            >
              {statusOptions.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          )}
          <button
            onClick={doExport}
            className="flex items-center space-x-1.5 px-3 py-2 text-xs font-medium border border-slate-200 dark:border-slate-600 rounded-xl hover:bg-slate-50 dark:hover:bg-slate-700 transition"
          >
            <Icon name="download" className="w-3.5 h-3.5" />
            <span>导出</span>
          </button>
        </div>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-slate-50/50 dark:bg-slate-700/30 text-xs font-semibold text-slate-400 uppercase tracking-wider border-b border-slate-100 dark:border-slate-700">
              {columns.map((c, i) => (
                <th key={c.header} className={`p-4 ${i === 0 ? 'pl-6' : ''} ${i === columns.length - 1 ? 'pr-6' : ''}`}>
                  {c.header}
                </th>
              ))}
              {actions && <th className="p-4 pr-6 text-right">操作</th>}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
            {pageRows.map((row) => (
              <tr key={rowKey(row)} className="hover:bg-slate-50/80 dark:hover:bg-slate-700/20 transition">
                {columns.map((c, i) => (
                  <td key={c.header} className={`p-4 ${i === 0 ? 'pl-6' : ''} ${i === columns.length - 1 ? 'pr-6' : ''}`}>
                    {c.cell(row)}
                  </td>
                ))}
                {actions && <td className="p-4 pr-6 text-right space-x-2 whitespace-nowrap">{actions(row)}</td>}
              </tr>
            ))}
            {pageRows.length === 0 && (
              <tr>
                <td colSpan={columns.length + (actions ? 1 : 0)} className="p-10 text-center text-slate-400 text-sm">
                  {emptyText}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="p-4 border-t border-slate-100 dark:border-slate-700 flex flex-col sm:flex-row items-center justify-between gap-3 text-xs text-slate-500">
        <span>
          显示 {filtered.length === 0 ? 0 : start + 1} 至 {end} 条,共 {filtered.length} 条记录
        </span>
        <div className="flex items-center space-x-1">
          <button
            onClick={() => setPage((p) => Math.max(1, p - 1))}
            disabled={cur <= 1}
            className="px-3 py-1.5 border border-slate-200 dark:border-slate-700 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-50 transition"
          >
            上一页
          </button>
          {Array.from({ length: pages }, (_, i) => i + 1).map((n) => (
            <button
              key={n}
              onClick={() => setPage(n)}
              className={`px-3 py-1.5 rounded-lg transition ${
                n === cur
                  ? 'bg-brand-600 text-white font-bold'
                  : 'border border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-700'
              }`}
            >
              {n}
            </button>
          ))}
          <button
            onClick={() => setPage((p) => Math.min(pages, p + 1))}
            disabled={cur >= pages}
            className="px-3 py-1.5 border border-slate-200 dark:border-slate-700 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-50 transition"
          >
            下一页
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// 概览仪表盘
// ---------------------------------------------------------------------------

interface HistorySample {
  t: string;
  users: number;
  online: number;
  lan: number;
}

function Dashboard({
  users,
  peers,
  stats,
  history,
  searchQuery,
  onViewUser,
  onResetUser,
  onDeleteUser,
}: {
  users: User[];
  peers: Peer[];
  stats: Stats;
  history: HistorySample[];
  searchQuery: string;
  onViewUser: (u: User) => void;
  onResetUser: (u: User) => void;
  onDeleteUser: (u: User) => void;
}) {
  const lineRef = useRef<HTMLCanvasElement>(null);
  const doughnutRef = useRef<HTMLCanvasElement>(null);
  const [range, setRange] = useState('15');

  const lanCount = peers.filter((p) => p.lan && p.lan !== '-').length;
  const onlineRate = stats.users > 0 ? Math.round((stats.peersOnline / stats.users) * 100) : 0;

  const last = history[history.length - 1];
  const prev = history[history.length - 2];
  const delta = (cur: number) => (prev && prev.users !== 0 ? ((cur - prev.users) / prev.users) * 100 : 0);

  const kpis = [
    {
      title: '账号总数',
      value: String(stats.users),
      change: `${delta(last?.users ?? 0) >= 0 ? '+' : ''}${delta(last?.users ?? 0).toFixed(1)}%`,
      isUp: delta(last?.users ?? 0) >= 0,
      bgColor: 'bg-brand-50 dark:bg-brand-900/30',
      textColor: 'text-brand-500',
      icon: 'users',
    },
    {
      title: '在线设备',
      value: String(stats.peersOnline),
      change: `${delta(last?.online ?? 0) >= 0 ? '+' : ''}${delta(last?.online ?? 0).toFixed(1)}%`,
      isUp: delta(last?.online ?? 0) >= 0,
      bgColor: 'bg-emerald-50 dark:bg-emerald-900/30',
      textColor: 'text-emerald-500',
      icon: 'device',
    },
    {
      title: '局域网可达',
      value: String(lanCount),
      change: `${delta(last?.lan ?? 0) >= 0 ? '+' : ''}${delta(last?.lan ?? 0).toFixed(1)}%`,
      isUp: delta(last?.lan ?? 0) >= 0,
      bgColor: 'bg-purple-50 dark:bg-purple-900/30',
      textColor: 'text-purple-500',
      icon: 'server',
    },
    {
      title: '账号在线率',
      value: `${onlineRate}%`,
      change: `${delta(last ? (last.online / Math.max(last.users, 1)) * 100 : 0) >= 0 ? '+' : ''}${delta(
        last ? (last.online / Math.max(last.users, 1)) * 100 : 0,
      ).toFixed(1)}%`,
      isUp: delta(last ? (last.online / Math.max(last.users, 1)) * 100 : 0) >= 0,
      bgColor: 'bg-rose-50 dark:bg-rose-900/30',
      textColor: 'text-rose-500',
      icon: 'shield',
    },
  ];

  useEffect(() => {
    const canvas = lineRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const sliced = range === 'all' ? history : history.slice(-Number(range));
    const chart = new Chart(ctx, {
      type: 'line',
      data: {
        labels: sliced.map((h) => h.t),
        datasets: [
          {
            label: '在线设备',
            data: sliced.map((h) => h.online),
            borderColor: '#0284c7',
            backgroundColor: 'rgba(2, 132, 199, 0.1)',
            fill: true,
            tension: 0.4,
          },
          {
            label: '账号数',
            data: sliced.map((h) => h.users),
            borderColor: '#10b981',
            backgroundColor: 'transparent',
            borderDash: [5, 5],
            tension: 0.4,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { position: 'top' } },
        scales: {
          y: { grid: { color: 'rgba(148, 163, 184, 0.12)' } },
          x: { grid: { display: false } },
        },
      },
    });
    return () => chart.destroy();
  }, [history, range]);

  useEffect(() => {
    const canvas = doughnutRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const chart = new Chart(ctx, {
      type: 'doughnut',
      data: {
        labels: ['局域网可达', '仅外部地址'],
        datasets: [
          {
            data: [lanCount, Math.max(0, peers.length - lanCount)],
            backgroundColor: ['#0284c7', '#8b5cf6'],
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        cutout: '62%',
        plugins: { legend: { position: 'bottom' } },
      },
    });
    return () => chart.destroy();
  }, [peers, lanCount]);

  return (
    <div className="space-y-6">
      {/* 1. KPI 数据卡片列表 */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-5">
        {kpis.map((kpi) => (
          <div
            key={kpi.title}
            className="p-5 bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm hover:shadow-md transition"
          >
            <div className="flex justify-between items-start">
              <div>
                <p className="text-xs font-medium text-slate-400 uppercase tracking-wider">{kpi.title}</p>
                <h3 className="text-2xl font-bold mt-2 text-slate-800 dark:text-white">{kpi.value}</h3>
              </div>
              <div className={`p-3 rounded-xl ${kpi.bgColor}`}>
                <Icon name={kpi.icon} className={`w-5 h-5 ${kpi.textColor}`} />
              </div>
            </div>
            <div className="flex items-center mt-4 text-xs">
              <span className={`font-semibold flex items-center ${kpi.isUp ? 'text-emerald-500' : 'text-rose-500'}`}>
                <Icon name={kpi.isUp ? 'arrow-up' : 'arrow-down'} className="w-3 h-3 mr-1" />
                {kpi.change}
              </span>
              <span className="text-slate-400 ml-2">相比上次刷新</span>
            </div>
          </div>
        ))}
      </div>

      {/* 2. 图表统计区 */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 p-6 bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm">
          <div className="flex justify-between items-center mb-4">
            <div>
              <h3 className="font-bold text-slate-800 dark:text-white">在线趋势</h3>
              <p className="text-xs text-slate-400">每次刷新采样的账号与在线设备统计</p>
            </div>
            <select
              value={range}
              onChange={(e) => setRange(e.target.value)}
              className="text-xs bg-slate-100 dark:bg-slate-700 border-none rounded-lg px-3 py-1.5 focus:ring-0"
            >
              <option value="7">最近7次</option>
              <option value="15">最近15次</option>
              <option value="all">全部采样</option>
            </select>
          </div>
          <div className="h-64 relative">
            <canvas ref={lineRef} />
          </div>
        </div>

        <div className="p-6 bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm">
          <div className="flex justify-between items-center mb-4">
            <div>
              <h3 className="font-bold text-slate-800 dark:text-white">网络分布</h3>
              <p className="text-xs text-slate-400">在线设备的网络可达性划分</p>
            </div>
          </div>
          <div className="h-64 relative flex items-center justify-center">
            {peers.length === 0 ? (
              <span className="text-sm text-slate-400">暂无在线设备</span>
            ) : (
              <canvas ref={doughnutRef} />
            )}
          </div>
        </div>
      </div>

      {/* 3. 用户数据列表 */}
      <DataTable
        title="用户账号列表"
        badge={users.length}
        externalQuery={searchQuery}
        exportName="用户列表"
        emptyText="暂无账号,点击右上角「新建账号」创建"
        columns={[
          {
            header: 'ID / 用户名',
            cell: (u) => (
              <div className="flex items-center space-x-3">
                <div className="w-8 h-8 rounded-lg bg-brand-50 dark:bg-brand-900/40 text-brand-600 dark:text-brand-400 flex items-center justify-center font-bold text-xs">
                  {u.username.slice(0, 1).toUpperCase()}
                </div>
                <div>
                  <div className="font-semibold text-slate-800 dark:text-slate-100">{u.username}</div>
                  <div className="text-xs text-slate-400">普通账号</div>
                </div>
              </div>
            ),
          },
          { header: '创建时间', cell: (u) => <span className="text-slate-500 dark:text-slate-400 text-xs">{u.created_at}</span> },
          { header: '状态', cell: () => <StatusPill status="正常" /> },
        ]}
        rows={users}
        rowKey={(u) => u.username}
        status={() => '正常'}
        statusOptions={[
          { value: 'ALL', label: '全部状态' },
          { value: '正常', label: '正常' },
        ]}
        searchFields={(u) => `${u.username} ${u.created_at}`}
        exportRow={(u) => [u.username, u.created_at, '正常']}
        actions={(u) => (
          <>
            <button onClick={() => onViewUser(u)} className="p-1.5 text-slate-400 hover:text-brand-600 transition" title="查看详情">
              <Icon name="eye" className="w-4 h-4" />
            </button>
            <button onClick={() => onResetUser(u)} className="p-1.5 text-slate-400 hover:text-brand-600 transition" title="重置密码">
              <Icon name="key" className="w-4 h-4" />
            </button>
            <button onClick={() => onDeleteUser(u)} className="p-1.5 text-slate-400 hover:text-rose-600 transition" title="删除">
              <Icon name="trash" className="w-4 h-4" />
            </button>
          </>
        )}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// 主应用
// ---------------------------------------------------------------------------

type TabId = 'dashboard' | 'users' | 'peers' | 'settings';

const NAV_ITEMS: { id: TabId; name: string; icon: string }[] = [
  { id: 'dashboard', name: '概览仪表盘', icon: 'pie' },
  { id: 'users', name: '用户管理', icon: 'users' },
  { id: 'peers', name: '在线设备', icon: 'device' },
  { id: 'settings', name: '系统设置', icon: 'gear' },
];

const App: React.FC = () => {
  const [token, setToken] = useState(getToken);
  const [me, setMe] = useState('');
  const [users, setUsers] = useState<User[]>([]);
  const [peers, setPeers] = useState<Peer[]>([]);
  const [stats, setStats] = useState<Stats>({ users: 0, peersOnline: 0 });
  const [history, setHistory] = useState<HistorySample[]>([]);

  const [tab, setTab] = useState<TabId>('dashboard');
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [darkMode, setDarkMode] = useState(() => localStorage.getItem(THEME_KEY) === 'dark');
  const [searchQuery, setSearchQuery] = useState('');
  const [notifOpen, setNotifOpen] = useState(false);
  const [notices, setNotices] = useState<{ time: string; msg: string }[]>([]);

  const [toast, setToast] = useState<ToastState | null>(null);
  const toastTimer = useRef<number | null>(null);

  const [createOpen, setCreateOpen] = useState(false);
  const [newUser, setNewUser] = useState('');
  const [newPass, setNewPass] = useState('');
  const [detail, setDetail] = useState<{ kind: 'user' | 'peer'; user?: User; peer?: Peer } | null>(null);
  const [resetTarget, setResetTarget] = useState<User | null>(null);
  const [resetPass, setResetPass] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<User | null>(null);

  const nowTime = () => new Date().toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });

  const showToast = useCallback((type: ToastType, msg: string) => {
    setToast({ type, message: msg });
    setNotices((n) => [{ time: nowTime(), msg }, ...n].slice(0, 8));
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3000);
  }, []);

  useEffect(() => {
    document.documentElement.classList.toggle('dark', darkMode);
    localStorage.setItem(THEME_KEY, darkMode ? 'dark' : 'light');
  }, [darkMode]);

  const load = useCallback(
    async (silent = false) => {
      try {
        const [meName, userList, peerList, statsData] = await Promise.all([
          api<{ username: string }>('/api/auth/me'),
          api<User[]>('/api/admin/users'),
          api<Peer[]>('/api/admin/peers'),
          api<Stats>('/api/admin/stats'),
        ]);
        setMe(meName.username);
        setUsers(userList);
        setPeers(peerList);
        setStats(statsData);
        setHistory((h) =>
          [
            ...h,
            {
              t: nowTime(),
              users: statsData.users,
              online: statsData.peersOnline,
              lan: peerList.filter((p) => p.lan && p.lan !== '-').length,
            },
          ].slice(-24),
        );
      } catch (e) {
        if (!silent) showToast('error', e instanceof Error ? e.message : String(e));
      }
    },
    [showToast],
  );

  useEffect(() => {
    if (!token) return;
    void load();
    const timer = window.setInterval(() => void load(true), 15000);
    return () => window.clearInterval(timer);
  }, [token, load]);

  const logout = () => {
    localStorage.removeItem(TOKEN_KEY);
    setToken('');
    setMe('');
    setUsers([]);
    setPeers([]);
    setHistory([]);
    setNotices([]);
  };

  const createUser = async () => {
    try {
      await api('/api/admin/users', {
        method: 'POST',
        body: JSON.stringify({ username: newUser, password: newPass }),
      });
      setNewUser('');
      setNewPass('');
      setCreateOpen(false);
      showToast('success', `账号 ${newUser} 已创建`);
      void load();
    } catch (e) {
      showToast('error', e instanceof Error ? e.message : String(e));
    }
  };

  const deleteUser = async () => {
    if (!deleteTarget) return;
    const name = deleteTarget.username;
    try {
      await api(`/api/admin/users/${encodeURIComponent(name)}`, { method: 'DELETE' });
      setDeleteTarget(null);
      showToast('success', `账号 ${name} 已删除`);
      void load();
    } catch (e) {
      showToast('error', e instanceof Error ? e.message : String(e));
    }
  };

  const doResetPassword = async () => {
    if (!resetTarget) return;
    const name = resetTarget.username;
    try {
      await api(`/api/admin/users/${encodeURIComponent(name)}/password`, {
        method: 'POST',
        body: JSON.stringify({ password: resetPass }),
      });
      setResetTarget(null);
      setResetPass('');
      showToast('success', `账号 ${name} 密码已重置`);
    } catch (e) {
      showToast('error', e instanceof Error ? e.message : String(e));
    }
  };

  if (!token) {
    return (
      <LoginPage
        onLogin={(t, name) => {
          setToken(t);
          setMe(name);
        }}
      />
    );
  }

  const tabName = NAV_ITEMS.find((n) => n.id === tab)?.name ?? '控制台';

  const userColumns: Col<User>[] = [
    {
      header: 'ID / 用户名',
      cell: (u) => (
        <div className="flex items-center space-x-3">
          <div className="w-8 h-8 rounded-lg bg-brand-50 dark:bg-brand-900/40 text-brand-600 dark:text-brand-400 flex items-center justify-center font-bold text-xs">
            {u.username.slice(0, 1).toUpperCase()}
          </div>
          <div>
            <div className="font-semibold text-slate-800 dark:text-slate-100">{u.username}</div>
            <div className="text-xs text-slate-400">普通账号</div>
          </div>
        </div>
      ),
    },
    { header: '创建时间', cell: (u) => <span className="text-slate-500 dark:text-slate-400 text-xs">{u.created_at}</span> },
    { header: '状态', cell: () => <StatusPill status="正常" /> },
  ];

  const peerColumns: Col<Peer>[] = [
    {
      header: 'ID / 设备',
      cell: (p) => (
        <div className="flex items-center space-x-3">
          <div className="w-8 h-8 rounded-lg bg-brand-50 dark:bg-brand-900/40 text-brand-600 dark:text-brand-400 flex items-center justify-center font-bold text-xs">
            {p.id.slice(0, 1).toUpperCase()}
          </div>
          <div>
            <div className="font-semibold text-slate-800 dark:text-slate-100">{p.id}</div>
            <div className="text-xs text-slate-400">远程设备</div>
          </div>
        </div>
      ),
    },
    { header: '局域网地址', cell: (p) => <span className="text-slate-500 dark:text-slate-400 text-xs">{p.lan || '-'}</span> },
    { header: '外部地址', cell: (p) => <span className="text-slate-500 dark:text-slate-400 text-xs">{p.external || '-'}</span> },
    { header: '状态', cell: () => <StatusPill status="在线" /> },
  ];

  return (
    <div className="flex h-screen overflow-hidden">
      {/* 侧边导航栏 */}
      <aside
        className={`flex flex-col flex-shrink-0 transition-all duration-300 bg-white dark:bg-slate-800 border-r border-slate-200 dark:border-slate-700 z-20 ${
          sidebarOpen ? 'w-64' : 'w-20'
        }`}
      >
        <div className="flex items-center justify-between h-16 px-4 border-b border-slate-200 dark:border-slate-700">
          <div className="flex items-center space-x-3 overflow-hidden">
            <div className="flex items-center justify-center w-10 h-10 rounded-xl bg-brand-600 text-white shadow-brand-lg flex-shrink-0">
              <Icon name="cube" className="w-5 h-5" />
            </div>
            {sidebarOpen && (
              <span className="font-bold text-lg whitespace-nowrap text-slate-800 dark:text-white">dcr-signal</span>
            )}
          </div>
          <button
            onClick={() => setSidebarOpen((v) => !v)}
            className="p-1.5 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700 transition"
          >
            <Icon name={sidebarOpen ? 'angle-left' : 'angle-right'} className="w-4 h-4" />
          </button>
        </div>

        <nav className="flex-1 px-3 py-4 space-y-1 overflow-y-auto">
          {NAV_ITEMS.map((item) => {
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setTab(item.id)}
                className={`flex items-center w-full px-3 py-3 rounded-xl transition duration-150 group ${
                  active
                    ? 'bg-brand-50 text-brand-600 dark:bg-brand-500/10 dark:text-brand-400 font-semibold'
                    : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700/50 hover:text-slate-900 dark:hover:text-slate-200'
                }`}
              >
                <Icon name={item.icon} className="w-5 h-5 text-center flex-shrink-0 transition group-hover:scale-110" />
                {sidebarOpen && <span className="ml-3 text-sm whitespace-nowrap">{item.name}</span>}
                {sidebarOpen && item.id === 'users' && users.length > 0 && (
                  <span className="ml-auto px-2 py-0.5 text-xs font-medium rounded-full bg-red-100 text-red-600 dark:bg-red-900/40 dark:text-red-400">
                    {users.length}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="p-3 border-t border-slate-200 dark:border-slate-700">
          <div className="flex items-center p-2 rounded-xl bg-slate-50 dark:bg-slate-700/50">
            <div className="w-9 h-9 rounded-full bg-brand-100 dark:bg-brand-900/40 text-brand-700 dark:text-brand-400 flex items-center justify-center font-semibold text-sm flex-shrink-0 border border-slate-200 dark:border-slate-600">
              {me.slice(0, 1).toUpperCase()}
            </div>
            {sidebarOpen && (
              <div className="ml-3 overflow-hidden">
                <p className="text-sm font-semibold text-slate-800 dark:text-slate-200 truncate">{me}</p>
                <p className="text-xs text-slate-500 dark:text-slate-400 truncate">管理员</p>
              </div>
            )}
            <button
              onClick={logout}
              title="退出登录"
              className="ml-auto p-1.5 rounded-lg text-slate-400 hover:text-rose-600 hover:bg-slate-100 dark:hover:bg-slate-600 transition"
            >
              <Icon name="logout" className="w-4 h-4" />
            </button>
          </div>
        </div>
      </aside>

      {/* 主内容区域 */}
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        {/* 顶栏 Header */}
        <header className="flex items-center justify-between h-16 px-6 bg-white dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700 z-10">
          <div className="flex items-center space-x-4">
            <h1 className="text-xl font-bold text-slate-800 dark:text-white">{tabName}</h1>
            <div className="hidden sm:flex items-center text-xs text-slate-400 space-x-2">
              <span>主页</span>
              <Icon name="chevron-right" className="w-2.5 h-2.5" />
              <span className="text-slate-600 dark:text-slate-300">{tabName}</span>
            </div>
          </div>

          <div className="flex items-center space-x-3">
            <div className="relative hidden md:block">
              <Icon name="search" className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 w-3.5 h-3.5" />
              <input
                type="text"
                placeholder="搜索关键词..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-60 py-2 pl-9 pr-4 text-sm bg-slate-100 dark:bg-slate-700 text-slate-800 dark:text-slate-100 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500 transition"
              />
            </div>

            <button
              onClick={() => setDarkMode((v) => !v)}
              className="p-2.5 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-xl transition"
              title={darkMode ? '切换到浅色模式' : '切换到深色模式'}
            >
              <Icon name={darkMode ? 'sun' : 'moon'} className={`w-5 h-5 ${darkMode ? 'text-amber-400' : ''}`} />
            </button>

            <div className="relative">
              <button
                onClick={() => setNotifOpen((v) => !v)}
                className="relative p-2.5 text-slate-500 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-xl transition"
              >
                <Icon name="bell" className="w-5 h-5" />
                {notices.length > 0 && (
                  <span className="absolute top-2 right-2 w-2 h-2 bg-red-500 rounded-full ring-2 ring-white dark:ring-slate-800" />
                )}
              </button>

              {notifOpen && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setNotifOpen(false)} />
                  <div className="absolute right-0 mt-2 w-80 bg-white dark:bg-slate-800 rounded-2xl shadow-xl border border-slate-100 dark:border-slate-700 py-2 z-50">
                    <div className="px-4 py-2 border-b border-slate-100 dark:border-slate-700 flex justify-between items-center">
                      <span className="font-bold text-sm text-slate-800 dark:text-white">系统通知</span>
                      <button
                        onClick={() => setNotices([])}
                        className="text-xs text-brand-600 dark:text-brand-400 cursor-pointer"
                      >
                        全部已读
                      </button>
                    </div>
                    <div className="max-h-60 overflow-y-auto divide-y divide-slate-100 dark:divide-slate-700/50">
                      {notices.length === 0 ? (
                        <div className="p-4 text-center text-xs text-slate-400">暂无通知</div>
                      ) : (
                        notices.map((n, i) => (
                          <div key={i} className="p-3 hover:bg-slate-50 dark:hover:bg-slate-700/30 transition cursor-pointer">
                            <p className="text-xs font-semibold text-slate-800 dark:text-slate-200">{n.msg}</p>
                            <p className="text-[11px] text-slate-400 mt-1">{n.time}</p>
                          </div>
                        ))
                      )}
                    </div>
                  </div>
                </>
              )}
            </div>

            <button
              onClick={() => setCreateOpen(true)}
              className="flex items-center space-x-2 px-4 py-2 bg-brand-600 hover:bg-brand-700 text-white rounded-xl font-medium text-sm shadow-md shadow-brand-500/20 transition active:scale-95"
            >
              <Icon name="plus" className="w-4 h-4" />
              <span className="hidden sm:inline">新建账号</span>
            </button>
          </div>
        </header>

        {/* 视图主体区 */}
        <main className="flex-1 overflow-y-auto p-6 space-y-6">
          {tab === 'dashboard' && (
            <Dashboard
              users={users}
              peers={peers}
              stats={stats}
              history={history}
              searchQuery={searchQuery}
              onViewUser={(u) => setDetail({ kind: 'user', user: u })}
              onResetUser={(u) => {
                setResetTarget(u);
                setResetPass('');
              }}
              onDeleteUser={(u) => setDeleteTarget(u)}
            />
          )}

          {tab === 'users' && (
            <DataTable
              title="用户管理"
              badge={users.length}
              externalQuery={searchQuery}
              exportName="用户列表"
              emptyText="暂无账号,点击右上角「新建账号」创建"
              columns={userColumns}
              rows={users}
              rowKey={(u) => u.username}
              status={() => '正常'}
              statusOptions={[
                { value: 'ALL', label: '全部状态' },
                { value: '正常', label: '正常' },
              ]}
              searchFields={(u) => `${u.username} ${u.created_at}`}
              exportRow={(u) => [u.username, u.created_at, '正常']}
              actions={(u) => (
                <>
                  <button onClick={() => setDetail({ kind: 'user', user: u })} className="p-1.5 text-slate-400 hover:text-brand-600 transition" title="查看详情">
                    <Icon name="eye" className="w-4 h-4" />
                  </button>
                  <button
                    onClick={() => {
                      setResetTarget(u);
                      setResetPass('');
                    }}
                    className="p-1.5 text-slate-400 hover:text-brand-600 transition"
                    title="重置密码"
                  >
                    <Icon name="key" className="w-4 h-4" />
                  </button>
                  <button onClick={() => setDeleteTarget(u)} className="p-1.5 text-slate-400 hover:text-rose-600 transition" title="删除">
                    <Icon name="trash" className="w-4 h-4" />
                  </button>
                </>
              )}
            />
          )}

          {tab === 'peers' && (
            <DataTable
              title="在线设备"
              badge={peers.length}
              externalQuery={searchQuery}
              exportName="在线设备"
              emptyText="暂无在线设备"
              columns={peerColumns}
              rows={peers}
              rowKey={(p) => p.id}
              status={() => '在线'}
              statusOptions={[
                { value: 'ALL', label: '全部状态' },
                { value: '在线', label: '在线' },
              ]}
              searchFields={(p) => `${p.id} ${p.lan} ${p.external}`}
              exportRow={(p) => [p.id, p.lan, p.external, '在线']}
              actions={(p) => (
                <button onClick={() => setDetail({ kind: 'peer', peer: p })} className="p-1.5 text-slate-400 hover:text-brand-600 transition" title="查看详情">
                  <Icon name="eye" className="w-4 h-4" />
                </button>
              )}
            />
          )}

          {tab === 'settings' && (
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <div className="p-6 bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm">
                <div className="flex items-center space-x-3 mb-4">
                  <div className="p-3 rounded-xl bg-emerald-50 dark:bg-emerald-900/30 text-emerald-500">
                    <Icon name="server" className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-bold text-slate-800 dark:text-white">信令服务状态</h3>
                    <p className="text-xs text-slate-400">dcr-signal 信令 + 管理服务</p>
                  </div>
                  <span className="ml-auto">
                    <StatusPill status="在线" />
                  </span>
                </div>
                <div className="space-y-3 text-sm">
                  <div className="flex justify-between">
                    <span className="text-slate-400">服务端口</span>
                    <span className="font-mono font-semibold text-slate-700 dark:text-slate-200">21116 (信令) / 21120 (管理)</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">当前登录</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">{me}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">账号总数</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">{stats.users}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">在线设备</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">{stats.peersOnline}</span>
                  </div>
                </div>
              </div>

              <div className="p-6 bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700/60 shadow-sm">
                <div className="flex items-center space-x-3 mb-4">
                  <div className="p-3 rounded-xl bg-brand-50 dark:bg-brand-900/30 text-brand-500">
                    <Icon name="shield" className="w-5 h-5" />
                  </div>
                  <div>
                    <h3 className="font-bold text-slate-800 dark:text-white">安全与访问</h3>
                    <p className="text-xs text-slate-400">由服务端启动参数控制</p>
                  </div>
                </div>
                <div className="space-y-3 text-sm">
                  <div className="flex justify-between">
                    <span className="text-slate-400">自助注册</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">--no-register 可关闭</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">账号认证</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">JWT 令牌</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">管理接口</span>
                    <span className="font-mono font-semibold text-slate-700 dark:text-slate-200">/api/admin/*</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">数据存储</span>
                    <span className="font-semibold text-slate-700 dark:text-slate-200">JSON 用户库</span>
                  </div>
                </div>
              </div>
            </div>
          )}
        </main>
      </div>

      {/* 新建账号 模态对话框 */}
      <Modal open={createOpen} onClose={() => setCreateOpen(false)}>
        <div className="flex justify-between items-center border-b border-slate-100 dark:border-slate-700 pb-3">
          <h3 className="font-bold text-lg text-slate-800 dark:text-white">新建账号</h3>
          <button onClick={() => setCreateOpen(false)} className="text-slate-400 hover:text-slate-600">
            <Icon name="xmark" className="w-5 h-5" />
          </button>
        </div>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-300 mb-1">用户名</label>
            <input
              value={newUser}
              onChange={(e) => setNewUser(e.target.value)}
              placeholder="3-32 位字母数字 _ -"
              className="w-full px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500"
            />
          </div>
          <div>
            <label className="block text-xs font-semibold text-slate-600 dark:text-slate-300 mb-1">初始密码</label>
            <input
              type="password"
              value={newPass}
              onChange={(e) => setNewPass(e.target.value)}
              placeholder="至少 6 位"
              className="w-full px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500"
            />
          </div>
        </div>
        <div className="flex justify-end space-x-3 pt-3 border-t border-slate-100 dark:border-slate-700">
          <button
            onClick={() => setCreateOpen(false)}
            className="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-xl transition"
          >
            取消
          </button>
          <button
            onClick={() => void createUser()}
            className="px-4 py-2 text-sm font-medium bg-brand-600 hover:bg-brand-700 text-white rounded-xl shadow-md shadow-brand-500/20 transition active:scale-95"
          >
            保存添加
          </button>
        </div>
      </Modal>

      {/* 重置密码 模态对话框 */}
      <Modal open={resetTarget !== null} onClose={() => setResetTarget(null)} maxW="max-w-md">
        <div className="flex justify-between items-center border-b border-slate-100 dark:border-slate-700 pb-3">
          <h3 className="font-bold text-lg text-slate-800 dark:text-white">重置密码</h3>
          <button onClick={() => setResetTarget(null)} className="text-slate-400 hover:text-slate-600">
            <Icon name="xmark" className="w-5 h-5" />
          </button>
        </div>
        <div>
          <label className="block text-xs font-semibold text-slate-600 dark:text-slate-300 mb-1">账号 {resetTarget?.username} 的新密码</label>
          <input
            type="password"
            value={resetPass}
            onChange={(e) => setResetPass(e.target.value)}
            placeholder="至少 6 位"
            className="w-full px-3 py-2 text-sm bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-xl focus:outline-none focus:ring-2 focus:ring-brand-500"
          />
        </div>
        <div className="flex justify-end space-x-3 pt-3 border-t border-slate-100 dark:border-slate-700">
          <button
            onClick={() => setResetTarget(null)}
            className="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-xl transition"
          >
            取消
          </button>
          <button
            onClick={() => void doResetPassword()}
            className="px-4 py-2 text-sm font-medium bg-brand-600 hover:bg-brand-700 text-white rounded-xl shadow-md shadow-brand-500/20 transition active:scale-95"
          >
            确认重置
          </button>
        </div>
      </Modal>

      {/* 详情查看 模态对话框 */}
      <Modal open={detail !== null} onClose={() => setDetail(null)} maxW="max-w-md">
        <div className="flex justify-between items-center border-b border-slate-100 dark:border-slate-700 pb-3">
          <h3 className="font-bold text-lg text-slate-800 dark:text-white">
            {detail?.kind === 'peer' ? '设备详情' : '账号详情'}
          </h3>
          <button onClick={() => setDetail(null)} className="text-slate-400 hover:text-slate-600">
            <Icon name="xmark" className="w-5 h-5" />
          </button>
        </div>
        {detail?.kind === 'user' && detail.user && (
          <div className="space-y-3 text-sm">
            <div className="flex justify-between">
              <span className="text-slate-400">用户名:</span>
              <span className="font-mono font-bold text-slate-700 dark:text-slate-200">{detail.user.username}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">创建时间:</span>
              <span className="font-semibold text-slate-700 dark:text-slate-200">{detail.user.created_at}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">当前状态:</span>
              <StatusPill status="正常" />
            </div>
          </div>
        )}
        {detail?.kind === 'peer' && detail.peer && (
          <div className="space-y-3 text-sm">
            <div className="flex justify-between">
              <span className="text-slate-400">设备 ID:</span>
              <span className="font-mono font-bold text-slate-700 dark:text-slate-200">{detail.peer.id}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">局域网地址:</span>
              <span className="font-mono font-semibold text-slate-700 dark:text-slate-200">{detail.peer.lan || '-'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">外部地址:</span>
              <span className="font-mono font-semibold text-slate-700 dark:text-slate-200">{detail.peer.external || '-'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">当前状态:</span>
              <StatusPill status="在线" />
            </div>
          </div>
        )}
        <div className="pt-3 border-t border-slate-100 dark:border-slate-700 text-right">
          <button
            onClick={() => setDetail(null)}
            className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-xl text-sm font-medium"
          >
            关闭
          </button>
        </div>
      </Modal>

      {/* 删除确认 模态对话框 */}
      <Modal open={deleteTarget !== null} onClose={() => setDeleteTarget(null)} maxW="max-w-sm">
        <div className="flex justify-between items-center border-b border-slate-100 dark:border-slate-700 pb-3">
          <h3 className="font-bold text-lg text-slate-800 dark:text-white">删除账号</h3>
          <button onClick={() => setDeleteTarget(null)} className="text-slate-400 hover:text-slate-600">
            <Icon name="xmark" className="w-5 h-5" />
          </button>
        </div>
        <p className="text-sm text-slate-500 dark:text-slate-400">
          确定要删除账号 <span className="font-bold text-slate-800 dark:text-slate-100">{deleteTarget?.username}</span> 吗?此操作不可恢复。
        </p>
        <div className="flex justify-end space-x-3 pt-3 border-t border-slate-100 dark:border-slate-700">
          <button
            onClick={() => setDeleteTarget(null)}
            className="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-xl transition"
          >
            取消
          </button>
          <button
            onClick={() => void deleteUser()}
            className="px-4 py-2 text-sm font-medium bg-rose-600 hover:bg-rose-700 text-white rounded-xl shadow-md shadow-rose-500/20 transition active:scale-95"
          >
            删除
          </button>
        </div>
      </Modal>

      {/* Toast 消息提示 */}
      {toast && (
        <div className="fixed bottom-6 right-6 z-50 flex items-center space-x-2 px-4 py-3 bg-slate-800 dark:bg-slate-700 text-white rounded-xl shadow-xl text-sm">
          <Icon
            name={toast.type === 'success' ? 'check-circle' : 'x-circle'}
            className={`w-4 h-4 ${toast.type === 'success' ? 'text-emerald-400' : 'text-rose-400'}`}
          />
          <span>{toast.message}</span>
        </div>
      )}
    </div>
  );
};

export default App;
