import { useCallback, useEffect, useState } from 'react';

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

const App: React.FC = () => {
  const [token, setToken] = useState(getToken);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [loginErr, setLoginErr] = useState('');

  const [me, setMe] = useState('');
  const [users, setUsers] = useState<User[]>([]);
  const [peers, setPeers] = useState<Peer[]>([]);
  const [stats, setStats] = useState<Stats>({ users: 0, peersOnline: 0 });
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');

  const [newUser, setNewUser] = useState('');
  const [newPass, setNewPass] = useState('');
  const [resetUser, setResetUser] = useState<string | null>(null);
  const [resetPass, setResetPass] = useState('');

  const load = useCallback(async () => {
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
      setError('');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    if (token) void load();
  }, [token, load]);

  const doLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoginErr('');
    try {
      const r = await api<{ token: string; username: string }>('/api/auth/login', {
        method: 'POST',
        body: JSON.stringify({ username, password }),
      });
      localStorage.setItem(TOKEN_KEY, r.token);
      setToken(r.token);
      setMe(r.username);
      setPassword('');
      await load();
    } catch (err) {
      setLoginErr(err instanceof Error ? err.message : String(err));
    }
  };

  const logout = () => {
    localStorage.removeItem(TOKEN_KEY);
    setToken('');
    setMe('');
    setUsers([]);
    setPeers([]);
  };

  const createUser = async () => {
    setError('');
    try {
      await api('/api/admin/users', {
        method: 'POST',
        body: JSON.stringify({ username: newUser, password: newPass }),
      });
      setNewUser('');
      setNewPass('');
      setNotice(`账号 ${newUser} 已创建`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const deleteUser = async (name: string) => {
    if (!window.confirm(`确定删除账号 ${name}？`)) return;
    setError('');
    try {
      await api(`/api/admin/users/${encodeURIComponent(name)}`, { method: 'DELETE' });
      setNotice(`账号 ${name} 已删除`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const doResetPassword = async (name: string) => {
    setError('');
    try {
      await api(`/api/admin/users/${encodeURIComponent(name)}/password`, {
        method: 'POST',
        body: JSON.stringify({ password: resetPass }),
      });
      setResetUser(null);
      setResetPass('');
      setNotice(`账号 ${name} 密码已重置`);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  if (!token) {
    return (
      <div className="page login-page">
        <div className="brand">
          <div className="brand-logo">DCR</div>
          <h1>dcr-signal 管理后台</h1>
          <p>账号登录后管理用户与在线设备</p>
        </div>
        <form className="card login-card" onSubmit={doLogin}>
          <h2>登录</h2>
          {loginErr && <div className="err">{loginErr}</div>}
          <label>
            用户名
            <input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="admin"
              autoComplete="username"
              autoFocus
            />
          </label>
          <label>
            密码
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••"
              autoComplete="current-password"
            />
          </label>
          <button type="submit" className="primary">登录</button>
        </form>
      </div>
    );
  }

  return (
    <div className="page">
      <header className="topbar">
        <div className="brand-row">
          <div className="brand-logo small">DCR</div>
          <span className="topbar-title">dcr-signal 管理后台</span>
        </div>
        <div className="topbar-right">
          <span className="me">当前账号：{me}</span>
          <button className="ghost" onClick={logout}>退出登录</button>
        </div>
      </header>

      <div className="stat-row">
        <div className="stat-card">
          <div className="stat-num">{stats.users}</div>
          <div className="stat-label">账号总数</div>
        </div>
        <div className="stat-card">
          <div className="stat-num">{stats.peersOnline}</div>
          <div className="stat-label">在线设备</div>
        </div>
        <div className="stat-card">
          <div className="stat-num">{users.length}</div>
          <div className="stat-label">已登记用户</div>
        </div>
      </div>

      {notice && <div className="ok" onClick={() => setNotice('')}>{notice}</div>}
      {error && <div className="err" onClick={() => setError('')}>{error}</div>}

      <div className="card">
        <div className="card-head">
          <h2>用户管理</h2>
          <button className="refresh" onClick={() => void load()}>刷新</button>
        </div>
        <table>
          <thead>
            <tr>
              <th>用户名</th>
              <th>创建时间</th>
              <th className="actions-col">操作</th>
            </tr>
          </thead>
          <tbody>
            {users.map((u) => (
              <tr key={u.username}>
                <td>{u.username}</td>
                <td>{u.created_at}</td>
                <td className="actions">
                  {resetUser === u.username ? (
                    <span className="inline-reset">
                      <input
                        type="password"
                        value={resetPass}
                        placeholder="新密码"
                        onChange={(e) => setResetPass(e.target.value)}
                      />
                      <button className="primary sm" onClick={() => void doResetPassword(u.username)}>确认</button>
                      <button className="ghost sm" onClick={() => { setResetUser(null); setResetPass(''); }}>取消</button>
                    </span>
                  ) : (
                    <>
                      <button className="ghost sm" onClick={() => { setResetUser(u.username); setResetPass(''); }}>重置密码</button>
                      <button className="danger sm" onClick={() => void deleteUser(u.username)}>删除</button>
                    </>
                  )}
                </td>
              </tr>
            ))}
            {users.length === 0 && (
              <tr><td colSpan={3} className="empty">暂无账号，请在下方创建</td></tr>
            )}
          </tbody>
        </table>
        <div className="create-row">
          <input
            value={newUser}
            onChange={(e) => setNewUser(e.target.value)}
            placeholder="新用户名（3-32 位字母数字 _ -）"
          />
          <input
            type="password"
            value={newPass}
            onChange={(e) => setNewPass(e.target.value)}
            placeholder="初始密码（至少 6 位）"
          />
          <button className="primary" onClick={() => void createUser()}>创建账号</button>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <h2>在线设备</h2>
          <span className="hint">信令服务器当前登记的在线对端</span>
        </div>
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>局域网地址</th>
              <th>外部地址</th>
            </tr>
          </thead>
          <tbody>
            {peers.map((p) => (
              <tr key={p.id}>
                <td>{p.id}</td>
                <td>{p.lan}</td>
                <td>{p.external}</td>
              </tr>
            ))}
            {peers.length === 0 && (
              <tr><td colSpan={3} className="empty">暂无在线设备</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default App;