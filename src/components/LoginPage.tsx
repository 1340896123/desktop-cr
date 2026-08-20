import React, { useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import { palette, fontFamily, radius, spacing, shadow } from '../theme/tokens';
import { loginAccount, type AccountSession } from '../services/auth';

const useStyles = makeStyles({
  root: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    height: '100vh',
    width: '100vw',
    backgroundColor: palette.background,
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
});

interface LoginPageProps {
  onLogin: (account: AccountSession) => void;
}

/** 账号登录页:登录 dcr-signal 服务通过验证后解锁应用 */
export const LoginPage: React.FC<LoginPageProps> = ({ onLogin }) => {
  const styles = useStyles();
  const [server, setServer] = useState('120.78.77.248:21120');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!username.trim() || !password) {
      setError('请输入用户名和密码');
      return;
    }
    setBusy(true);
    try {
      const account = await loginAccount(server.trim(), username.trim(), password);
      onLogin(account);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={styles.root}>
      <div className={styles.card}>
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
              placeholder="请输入密码"
              autoComplete="current-password"
            />
          </div>
          {error && <div className={styles.error}>{error}</div>}
          <button className={styles.submit} type="submit" disabled={busy}>
            {busy ? '登录中…' : '登 录'}
          </button>
        </form>

        <div className={styles.hint}>登录后解锁远程控制功能</div>
      </div>
    </div>
  );
};

export default LoginPage;