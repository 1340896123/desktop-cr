import React, { useEffect, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  EyeRegular,
  EyeOffRegular,
  KeyRegular,
  CopyRegular,
} from '@fluentui/react-icons';
import { fontFamily, spacing, radius, shadow } from '../theme/tokens';
import { getAppConfig, saveAppConfig, type AppConfig } from '../services/config';
import { startHost, stopHost, onHostStateChange, type HostState } from '../services/connection';
import { ToggleSwitch } from './SettingsPage';

const UU_BLUE = '#0066ff';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.section}px`,
    alignItems: 'center',
  },
  inner: {
    width: '100%',
    maxWidth: '768px',
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.section}px`,
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: '#111827',
    letterSpacing: '-0.02em',
    margin: 0,
  },
  card: {
    backgroundColor: '#ffffff',
    borderRadius: radius.card,
    boxShadow: shadow.card,
    border: '1px solid rgba(229, 231, 235, 0.8)',
    padding: '20px',
    display: 'flex',
    flexDirection: 'column',
    gap: '16px',
  },
  cardHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderBottom: '1px solid #F3F4F6',
    paddingBottom: '16px',
  },
  cardTitle: {
    fontFamily,
    fontSize: '16px',
    fontWeight: 700,
    color: '#1F2937',
  },
  allowRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  allowLabel: {
    fontFamily,
    fontSize: '12px',
    color: '#4B5563',
  },
  grid: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    gap: '16px',
    alignItems: 'center',
    paddingTop: '4px',
  },
  fieldLabel: {
    fontFamily,
    fontSize: '12px',
    color: '#9CA3AF',
    display: 'block',
    marginBottom: '4px',
  },
  hostId: {
    fontFamily: 'Consolas, "Courier New", monospace',
    fontSize: '24px',
    fontWeight: 900,
    letterSpacing: '0.08em',
    color: '#111827',
  },
  verifyRow: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    fontSize: '12px',
    color: '#6B7280',
  },
  select: {
    background: 'transparent',
    border: 'none',
    outline: 'none',
    fontFamily,
    fontSize: '12px',
    fontWeight: 500,
    color: '#374151',
    cursor: 'pointer',
  },
  codeBox: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    backgroundColor: '#F9FAFB',
    borderRadius: radius.control,
    padding: '8px 10px',
    border: '1px solid #E5E7EB',
    marginTop: '6px',
  },
  codeText: {
    fontFamily: 'Consolas, "Courier New", monospace',
    fontSize: '13px',
    fontWeight: 600,
    letterSpacing: '0.04em',
    color: '#1F2937',
  },
  codeActions: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    color: '#6B7280',
  },
  codeIconBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '24px',
    height: '24px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: '#6B7280',
    cursor: 'pointer',

    '&:hover': {
      color: '#111827',
    },
  },
  copyBtn: {
    marginLeft: '4px',
    backgroundColor: '#ffffff',
    border: '1px solid #E5E7EB',
    boxShadow: shadow.card,
    color: '#374151',
    padding: '4px 12px',
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 150ms ease',
    whiteSpace: 'nowrap',

    '&:hover': {
      backgroundColor: '#F9FAFB',
    },
  },
  note: {
    fontFamily,
    fontSize: '11px',
    color: '#9CA3AF',
    display: 'block',
    paddingTop: '2px',
  },
  hostStatus: {
    fontFamily,
    fontSize: '12px',
    color: '#4B5563',
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
  },
  hostStatusDot: {
    width: '8px',
    height: '8px',
    borderRadius: '50%',
  },
  cardDesc: {
    fontFamily,
    fontSize: '12px',
    color: '#9CA3AF',
    marginTop: '2px',
  },
  partnerInputRow: {
    display: 'flex',
    gap: '12px',
    paddingTop: '4px',
  },
  input: {
    flex: 1,
    minWidth: 0,
    height: '36px',
    padding: '0 12px',
    backgroundColor: '#ffffff',
    border: '1px solid #D1D5DB',
    borderRadius: radius.control,
    fontFamily,
    fontSize: '13px',
    color: '#111827',
    outline: 'none',
    transition: 'border 150ms ease',

    '&:focus': {
      border: `1px solid ${UU_BLUE}`,
    },
  },
  connectBtn: {
    padding: '0 32px',
    borderRadius: radius.control,
    border: 'none',
    backgroundColor: UU_BLUE,
    color: '#ffffff',
    fontFamily,
    fontSize: '13px',
    fontWeight: 500,
    cursor: 'pointer',
    transition: 'background-color 150ms ease',
    whiteSpace: 'nowrap',

    '&:hover': {
      backgroundColor: '#0052cc',
    },

    '&:disabled': {
      backgroundColor: '#E5E7EB',
      color: '#9CA3AF',
      cursor: 'not-allowed',
    },
  },
  errorText: {
    fontFamily,
    fontSize: '12px',
    color: '#DC2626',
    marginTop: '8px',
  },
});

/** 生成 8 位随机验证码（仅本地演示，无后端校验） */
function generateCode(): string {
  const chars = 'abcdefghijkmnpqrstuvwxyz23456789';
  let out = '';
  for (let i = 0; i < 8; i += 1) {
    out += chars[Math.floor(Math.random() * chars.length)];
  }
  return out;
}

interface RemoteAssistPageProps {
  /** 匹配到对端设备后交给 App 发起真实连接（peerId 为 config.peers 中的 id） */
  onConnectDevice?: (peerId: string, name: string) => Promise<void>;
  onShowToast?: (msg: string) => void;
}

/**
 * 远程协助：卡1「本设备」（允许他人远程协助开关 + 设备ID + 演示验证码），
 * 卡2「远控伙伴设备」（输入设备ID匹配对端列表并连接）。
 * 真实接线：开关持久化 hostEnabled 并启停被控端，连接匹配 config.peers 后调用 connectToDevice。
 */
export const RemoteAssistPage: React.FC<RemoteAssistPageProps> = ({ onConnectDevice, onShowToast }) => {
  const styles = useStyles();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [hostState, setHostState] = useState<HostState>({ running: false, port: 0 });
  const [hostError, setHostError] = useState<string | null>(null);
  const [showCode, setShowCode] = useState(false);
  const [code] = useState(generateCode);
  const [partnerId, setPartnerId] = useState('');

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getAppConfig().then(setConfig);
    void onHostStateChange((state) => setHostState(state)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const toggleHostEnabled = async () => {
    if (!config) return;
    const next: AppConfig = { ...config, hostEnabled: !config.hostEnabled };
    setConfig(next);
    setHostError(null);
    try {
      await saveAppConfig(next);
      if (next.hostEnabled) {
        await startHost(config.hostPort);
        onShowToast?.(`已开启被控端，端口 ${config.hostPort}`);
      } else {
        await stopHost();
        onShowToast?.('已关闭被控端');
      }
    } catch (error) {
      setHostError(String(error));
      onShowToast?.(`操作失败: ${String(error)}`);
    }
  };

  const copyCode = async () => {
    try {
      await navigator.clipboard.writeText(code);
      onShowToast?.('验证码及分享信息已复制到剪贴板');
    } catch {
      onShowToast?.('复制失败，请手动复制');
    }
  };

  const connectPartner = async () => {
    const input = partnerId.trim();
    if (!input) {
      onShowToast?.('请输入伙伴设备ID');
      return;
    }
    const peers = config?.peers ?? [];
    const match = peers.find(
      (p) => p.name === input || p.id === input || p.addr === input || p.addr.startsWith(`${input}:`),
    );
    if (!match) {
      onShowToast?.('未找到该设备，请先在设置-网络中添加对端设备');
      return;
    }
    try {
      await onConnectDevice?.(match.id, match.name);
    } catch (error) {
      onShowToast?.(`连接失败: ${String(error)}`);
    }
  };

  return (
    <div className={styles.page}>
      <div className={styles.inner}>
        <h1 className={styles.title}>远程协助</h1>

        {/* 卡1：本设备 */}
        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <span className={styles.cardTitle}>本设备</span>
            <div className={styles.allowRow}>
              <span className={styles.allowLabel}>允许他人远程协助</span>
              <ToggleSwitch on={config?.hostEnabled ?? false} onChange={() => void toggleHostEnabled()} />
            </div>
          </div>

          <div className={styles.grid}>
            <div>
              <span className={styles.fieldLabel}>本设备ID</span>
              <div className={styles.hostId}>{config?.hostId ?? '—'}</div>
            </div>

            <div>
              <div className={styles.verifyRow}>
                <span>验证方式:</span>
                <select className={styles.select} defaultValue="仅使用临时验证码">
                  <option>仅使用临时验证码</option>
                  <option>使用固定验证码</option>
                </select>
              </div>
              <div className={styles.codeBox}>
                <span className={styles.codeText}>{showCode ? code : '••••••••'}</span>
                <div className={styles.codeActions}>
                  <button
                    type="button"
                    className={styles.codeIconBtn}
                    onClick={() => setShowCode((prev) => !prev)}
                    aria-label={showCode ? '隐藏验证码' : '显示验证码'}
                  >
                    {showCode ? <EyeOffRegular fontSize={14} /> : <EyeRegular fontSize={14} />}
                  </button>
                  <button type="button" className={styles.codeIconBtn} onClick={() => onShowToast?.('验证码已设置')} aria-label="设置">
                    <KeyRegular fontSize={14} />
                  </button>
                  <button type="button" className={styles.copyBtn} onClick={() => void copyCode()}>
                    <CopyRegular fontSize={12} style={{ verticalAlign: '-2px', marginRight: 4 }} />
                    复制并分享
                  </button>
                </div>
              </div>
              <span className={styles.note}>验证码为本地演示生成，实际连接以设备ID+端口为准</span>
            </div>
          </div>

          <div className={styles.hostStatus}>
            <span
              className={styles.hostStatusDot}
              style={{ backgroundColor: hostState.running ? '#10B981' : '#D1D5DB' }}
            />
            {hostState.running
              ? `被控端运行中 · 端口 ${hostState.port || (config?.hostPort ?? 21118)}`
              : '被控端未运行'}
          </div>
          {hostError && <div className={styles.errorText}>{hostError}</div>}
        </div>

        {/* 卡2：远控伙伴设备 */}
        <div className={styles.card}>
          <div className={styles.cardHeader}>
            <div>
              <span className={styles.cardTitle}>远控伙伴设备</span>
              <p className={styles.cardDesc}>通过其他设备的【设备ID】及【设备验证码】启动远程控制</p>
            </div>
          </div>

          <div>
            <span className={styles.fieldLabel}>伙伴的设备ID</span>
            <div className={styles.partnerInputRow}>
              <input
                className={styles.input}
                placeholder="请输入设备ID"
                value={partnerId}
                onChange={(e) => setPartnerId(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void connectPartner();
                }}
                spellCheck={false}
              />
              <button type="button" className={styles.connectBtn} disabled={!partnerId.trim()} onClick={() => void connectPartner()}>
                连接
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default RemoteAssistPage;