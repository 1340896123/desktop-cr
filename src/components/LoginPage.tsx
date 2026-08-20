import React, { useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import { palette, fontFamily, radius, spacing, shadow, titleBarHeight, zIndex } from '../theme/tokens';
import { loginAccount, registerAccount, type AccountSession } from '../services/auth';
import { onWindowMaximizedChange } from '../services/window';
import { WindowControls } from './shared/WindowControls';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    width: '100vw',
    position: 'relative',
    backgroundColor: palette.background,
  },
  topBar: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    height: `${titleBarHeight}px`,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'flex-end',
    backgroundColor: '#ffffff',
    borderBottom: '1px solid #f3f4f6',
    zIndex: zIndex.titleBar,
  },
  card: {
    width: '360px',
    backgroundColor: palette.backgroundElevated,
    borderRadius: radius.card,
    boxShadow: shadow.popover,
    border: `1px solid ${palette.borderLight}`,
    padding: `${spacing.xxl}px ${spacing.xxl}px ${spacing.xl}px`,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'stretch',
  },
  tabBar: {
    display: 'flex',
    borderBottom: `1px solid ${palette.borderLight}`,
    marginBottom: `${spacing.lg}px`,
  },
  tab: {
    flex: 1,
    height: '36px',
    background: 'transparent',
    border: 'none',
    borderBottom: '2px solid transparent',
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textMuted,
    cursor: 'pointer',
    transition: 'color 150ms ease, border-color 150ms ease',
    marginBottom: '-1px',

    '&:hover': {
      color: palette.textPrimary,
    },
  },
  tabActive: {
    color: palette.primary,
    borderBottomColor: palette.primary,
  },
  brand: {
    display: 'flex',
    alignItems: 'center',
    gap: `${spacing.sm}px`,
    marginBottom: `${spacing.lg}px`,
  },
  logo: {
    width: '40px',
    height: '40px',
    borderRadius: '12px',
    backgroundColor: palette.primary,
    color: palette.textOnPrimary,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontWeight: 700,
    fontSize: '15px',
    letterSpacing: '0.02em',
    boxShadow: '0 4px 10px rgba(47, 126, 247, 0.35)',
    flexShrink: 0,
  },
  brandText: {
    display: 'flex',
    flexDirection: 'column',
  },
  title: {
    fontFamily,
    fontSize: '17px',
    fontWeight: 700,
    color: palette.textPrimary,
    margin: 0,
  },
  subtitle: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    marginTop: '2px',
  },
  form: {
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.sm}px`,
  },
  field: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  label: {
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
  },
  input: {
    height: '36px',
    padding: '0 12px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '13px',
    color: palette.textPrimary,
    outline: 'none',
    transition: 'border-color 150ms ease',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  submit: {
    height: '38px',
    border: 'none',
    borderRadius: radius.control,
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textOnPrimary,
    backgroundColor: palette.primary,
    cursor: 'pointer',
    marginTop: `${spacing.xs}px`,
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: palette.primaryHover,
    },

    '&:disabled': {
      opacity: 0.6,
      cursor: 'default',
    },
  },
  error: {
    fontFamily,
    fontSize: '12px',
    color: palette.destructive,
    marginTop: `${spacing.xs}px`,
    minHeight: '18px',
  },
  hint: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    textAlign: 'center',
    marginTop: `${spacing.sm}px`,
  },
  linkBtn: {
    fontFamily,
    fontSize: '12px',
    fontWeight: 600,
    color: palette.primary,
    background: 'none',
    border: 'none',
    padding: '0 2px',
    cursor: 'pointer',

    '&:hover': {
      color: palette.primaryHover,
      textDecoration: 'underline',
    },
  },
});

interface LoginPageProps {
  onLogin: (account: AccountSession) => void;
}

/** 账号登录/注册页:登录或自助注册 dcr-signal 服务通过验证后解锁应用 */
export const LoginPage: React.FC<LoginPageProps> = ({ onLogin }) => {
  const styles = useStyles();
  const [mode, setMode] = useState<'login' | 'register'>('login');
  const [server, setServer] = useState('120.78.77.248:21120');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [maximized, setMaximized] = useState(false);

  // 订阅窗口最大化状态，用于取消圆角边框（与主窗口行为一致）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onWindowMaximizedChange(setMaximized).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!server.trim()) {
      setError('请输入服务器地址');
      return;
    }
    if (!username.trim()) {
      setError('请输入用户名');
      return;
    }
    if (mode === 'register') {
      if (password.length < 6) {
        setError('密码长度至少 6 位');
        return;
      }
      if (password !== confirmPassword) {
        setError('两次输入的密码不一致');
        return;
      }
    } else if (!password) {
      setError('请输入密码');
      return;
    }
    setBusy(true);
    try {
      const account =
        mode === 'register'
          ? await registerAccount(server.trim(), username.trim(), password)
          : await loginAccount(server.trim(), username.trim(), password);
      onLogin(account);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const switchMode = (next: 'login' | 'register') => {
    if (next === mode) return;
    setMode(next);
    setError(null);
    setPassword('');
    setConfirmPassword('');
  };

  return (
    <div
      className={styles.root}
      style={{
        borderRadius: maximized ? 0 : 8,
        border: maximized ? 'none' : `1px solid ${palette.borderLight}`,
      }}
    >
      <div className={styles.topBar} data-tauri-drag-region="deep">
        <WindowControls />
      </div>
      <div className={styles.card}>
        <div className={styles.tabBar}>
          <button
            type="button"
            className={`${styles.tab} ${mode === 'login' ? styles.tabActive : ''}`}
            onClick={() => switchMode('login')}
          >
            登 录
          </button>
          <button
            type="button"
            className={`${styles.tab} ${mode === 'register' ? styles.tabActive : ''}`}
            onClick={() => switchMode('register')}
          >
            注 册
          </button>
        </div>

        <div className={styles.brand}>
          <div className={styles.logo}>UU</div>
          <div className={styles.brandText}>
            <h1 className={styles.title}>网易UU远程</h1>
            <span className={styles.subtitle}>远程桌面客户端</span>
          </div>
        </div>

        <form className={styles.form} onSubmit={submit}>
          <div className={styles.field}>
            <label className={styles.label} htmlFor="login-server">服务器地址</label>
            <input
              id="login-server"
              className={styles.input}
              value={server}
              onChange={(e) => setServer(e.target.value)}
              placeholder="host:port 或 http://host:port"
              spellCheck={false}
            />
          </div>
          <div className={styles.field}>
            <label className={styles.label} htmlFor="login-username">用户名</label>
            <input
              id="login-username"
              className={styles.input}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="请输入账号"
              autoComplete="username"
              spellCheck={false}
            />
          </div>
          <div className={styles.field}>
            <label className={styles.label} htmlFor="login-password">密码</label>
            <input
              id="login-password"
              className={styles.input}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={mode === 'register' ? '至少 6 位' : '请输入密码'}
              autoComplete={mode === 'register' ? 'new-password' : 'current-password'}
            />
          </div>
          {mode === 'register' && (
            <div className={styles.field}>
              <label className={styles.label} htmlFor="login-confirm">确认密码</label>
              <input
                id="login-confirm"
                className={styles.input}
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                placeholder="请再次输入密码"
                autoComplete="new-password"
              />
            </div>
          )}
          {error && <div className={styles.error}>{error}</div>}
          <button className={styles.submit} type="submit" disabled={busy}>
            {busy ? (mode === 'register' ? '注册中…' : '登录中…') : mode === 'register' ? '注 册' : '登 录'}
          </button>
        </form>

        <div className={styles.hint}>
          {mode === 'login' ? (
            <>
              还没有账号?
              <button
                type="button"
                className={styles.linkBtn}
                onClick={() => switchMode('register')}
              >
                去注册
              </button>
            </>
          ) : (
            <>
              已有账号?
              <button
                type="button"
                className={styles.linkBtn}
                onClick={() => switchMode('login')}
              >
                去登录
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export default LoginPage;