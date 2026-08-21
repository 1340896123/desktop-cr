import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowLeftRegular,
  ArrowUpRegular,
  ArrowSyncRegular,
  ArrowRightFilled,
  ArrowLeftFilled,
  FolderRegular,
  DocumentRegular,
  DocumentImageRegular,
  CheckmarkCircleFilled,
  DeleteRegular,
  PauseRegular,
  PlayRegular,
  DismissRegular,
} from '@fluentui/react-icons';
import { fontFamily, palette, radius, shadow, spacing } from '../theme/tokens';
import {
  listDirectory,
  getIncomingDir,
  sendFile,
  requestRemoteDir,
  requestFilePull,
  onFileProgress,
  onRemoteDirectory,
  type FileEntry,
} from '../services/fileTransfer';
import {
  getConnectionState,
  onConnectionStateChange,
} from '../services/connection';

const useStyles = makeStyles({
  page: {
    flex: 1,
    height: '100%',
    overflowY: 'hidden',
    padding: `${spacing.md}px ${spacing.lg}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.sm}px`,
    backgroundColor: palette.background,

    '@media (max-width: 560px)': {
      padding: `${spacing.sm}px ${spacing.md}px`,
    },
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    flexShrink: 0,

    '@media (max-width: 560px)': {
      flexDirection: 'column',
      alignItems: 'flex-start',
      gap: '6px',
    },
  },
  title: {
    fontFamily,
    fontSize: '20px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    margin: 0,
  },
  connectHint: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    background: palette.muted,
    borderRadius: radius.pill,
    padding: '5px 12px',
  },
  connectHintActive: {
    color: palette.online,
    background: `rgba(52, 199, 89, 0.12)`,
  },
  transferPanel: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    gap: '12px',
    flex: 1,
    minHeight: 0,

    // 窄窗口:上下堆叠两个面板,容器整体滚动
    '@media (max-width: 920px)': {
      gridTemplateColumns: '1fr',
      overflowY: 'auto',
      alignContent: 'start',
    },
  },
  pane: {
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.card,
    overflow: 'hidden',
    minWidth: 0,
    boxShadow: shadow.card,

    '@media (max-width: 920px)': {
      height: '48vh',
      minHeight: '320px',
    },
  },
  paneHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 12px',
    backgroundColor: palette.background,
    borderBottom: `1px solid ${palette.borderLight}`,
    flexShrink: 0,
  },
  paneTitle: {
    fontFamily,
    fontSize: '13px',
    fontWeight: 700,
    color: palette.textPrimary,
  },
  tag: {
    fontFamily,
    fontSize: '10px',
    lineHeight: '16px',
    padding: '0 7px',
    borderRadius: radius.pill,
    fontWeight: 600,
  },
  tagLocal: {
    backgroundColor: palette.borderLight,
    color: palette.textSecondary,
  },
  tagRemote: {
    backgroundColor: `rgba(52, 199, 89, 0.12)`,
    color: palette.online,
  },
  countText: {
    fontFamily,
    fontSize: '11px',
    color: palette.textMuted,
    marginLeft: 'auto',
    whiteSpace: 'nowrap',
  },
  headSendBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '4px',
    padding: '5px 10px',
    borderRadius: radius.control,
    border: 'none',
    backgroundColor: palette.borderLight,
    color: palette.textSecondary,
    fontFamily,
    fontSize: '12px',
    cursor: 'not-allowed',
    whiteSpace: 'nowrap',
  },
  headSendBtnActive: {
    backgroundColor: palette.primary,
    color: palette.textOnPrimary,
    cursor: 'pointer',

    '&:hover': {
      backgroundColor: palette.primaryHover,
    },
  },
  headGroup: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    marginLeft: 'auto',
  },
  pathBar: {
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
    padding: '6px 8px',
    borderBottom: `1px solid ${palette.borderLight}`,
    flexShrink: 0,
  },
  pathIconBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '26px',
    height: '26px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textMuted,
    cursor: 'pointer',
    transition: 'background-color 150ms ease, color 150ms ease',
    flexShrink: 0,

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },

    '&:disabled': {
      color: palette.border,
      cursor: 'not-allowed',
      opacity: 0.5,
    },
  },
  pathInput: {
    flex: 1,
    minWidth: 0,
    height: '26px',
    padding: '0 10px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    outline: 'none',
    transition: 'border-color 150ms ease',

    '&:focus': {
      border: `1px solid ${palette.primary}`,
    },
  },
  fileTableWrap: {
    flex: 1,
    overflowY: 'auto',
    minHeight: 0,
  },
  fileTable: {
    width: '100%',
    borderCollapse: 'collapse',
    fontFamily,
    fontSize: '12px',
    color: palette.textPrimary,
  },
  th: {
    padding: '6px 8px',
    textAlign: 'left',
    fontWeight: 400,
    color: palette.textSecondary,
    backgroundColor: palette.background,
    borderBottom: `1px solid ${palette.borderLight}`,
    position: 'sticky',
    top: 0,
    whiteSpace: 'nowrap',
  },
  thCheck: {
    width: '32px',
  },
  td: {
    padding: '6px 8px',
    borderBottom: `1px solid ${palette.borderLight}`,
    whiteSpace: 'nowrap',
  },
  tr: {
    cursor: 'pointer',
    transition: 'background-color 120ms ease',

    '&:hover': {
      backgroundColor: palette.primarySoft,
    },
  },
  trSelected: {
    backgroundColor: palette.primarySoft,

    '&:hover': {
      backgroundColor: palette.primarySoft,
    },
  },
  fileName: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    fontWeight: 500,
    color: palette.textPrimary,
    maxWidth: '220px',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  fileIcon: {
    display: 'flex',
    flexShrink: 0,
  },
  dateCol: {
    color: palette.textMuted,
  },
  typeCol: {
    color: palette.textSecondary,
  },
  hideSm: {
    '@media (max-width: 760px)': {
      display: 'none',
    },
  },
  sizeCol: {
    color: palette.textSecondary,
    textAlign: 'right',
  },
  taskSection: {
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.card,
    padding: '12px',
    boxShadow: shadow.card,
    flexShrink: 0,
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  taskHeader: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    borderBottom: `1px solid ${palette.borderLight}`,
    paddingBottom: '8px',
  },
  taskTitle: {
    fontFamily,
    fontSize: '13px',
    fontWeight: 700,
    color: palette.textPrimary,
  },
  taskCount: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
    fontWeight: 400,
    marginLeft: '6px',
  },
  batchOps: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    color: palette.textSecondary,
    fontSize: '11px',
  },
  batchBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '4px',
    border: 'none',
    background: 'transparent',
    padding: 0,
    color: palette.textSecondary,
    fontFamily,
    fontSize: '11px',
    cursor: 'pointer',
    transition: 'color 150ms ease',

    '&:hover': {
      color: palette.textPrimary,
    },
  },
  batchBtnDanger: {
    '&:hover': {
      color: palette.destructive,
    },
  },
  taskList: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
    maxHeight: '200px',
    overflowY: 'auto',
  },
  taskRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    backgroundColor: palette.background,
    padding: '8px',
    borderRadius: '8px',
    border: `1px solid ${palette.borderLight}`,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,

    // 窄窗口:允许换行,避免固定宽度列横向溢出
    '@media (max-width: 920px)': {
      flexWrap: 'wrap',
      rowGap: '4px',
    },
  },
  taskNameCol: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    width: '25%',
    minWidth: 0,
  },
  taskStatusCol: {
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
    minWidth: '150px',
    flexShrink: 0,
  },
  taskStatusSent: {
    color: palette.online,
    fontWeight: 500,
  },
  taskStatusPaused: {
    color: palette.textSecondary,
  },
  taskStatusFailed: {
    color: palette.destructive,
  },
  progressTrack: {
    width: '80px',
    height: '4px',
    borderRadius: radius.pill,
    backgroundColor: palette.borderLight,
    overflow: 'hidden',
    flexShrink: 0,
  },
  progressBar: {
    height: '100%',
    borderRadius: radius.pill,
    backgroundColor: palette.primary,
    transition: 'width 120ms linear',
  },
  taskSize: {
    width: '70px',
    textAlign: 'right',
    flexShrink: 0,
    whiteSpace: 'nowrap',

    '@media (max-width: 920px)': {
      display: 'none',
    },
  },
  pathChip: {
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    flex: '1 1 20%',
    minWidth: 0,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
  },
  chipTag: {
    fontSize: '10px',
    padding: '0 5px',
    borderRadius: '4px',
    fontWeight: 500,
    flexShrink: 0,
  },
  chipTagLocal: {
    backgroundColor: palette.borderLight,
    color: palette.textSecondary,
  },
  chipTagRemote: {
    backgroundColor: `rgba(52, 199, 89, 0.12)`,
    color: palette.online,
  },
  speedBadge: {
    backgroundColor: palette.textPrimary,
    color: '#FBBF24',
    padding: '2px 6px',
    borderRadius: '4px',
    fontSize: '10px',
    fontFamily: 'Consolas, monospace',
    flexShrink: 0,
    whiteSpace: 'nowrap',

    '@media (max-width: 920px)': {
      display: 'none',
    },
  },
  taskDel: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    width: '24px',
    height: '24px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textMuted,
    cursor: 'pointer',
    flexShrink: 0,

    '&:hover': {
      backgroundColor: 'rgba(220, 38, 38, 0.1)',
      color: palette.destructive,
    },
  },
  empty: {
    padding: '24px 16px',
    textAlign: 'center',
    color: palette.textMuted,
    fontFamily,
    fontSize: '12px',
  },
});

interface TransferTask {
  /** 唯一键(direction:id),区分发送与接收两侧可能重号的 id */
  key: string;
  id: number;
  name: string;
  direction: 'send' | 'recv';
  status: 'transferring' | 'sent' | 'failed' | 'paused';
  size: number;
  received: number;
  sendPath: string;
  recvPath: string;
}

/** 拉取类传输的 id 基准:与发送侧(1..n)和被控端推送(1..n)分区,避免冲突 */
const PULL_ID_BASE = 1_000_000;
let pullSeq = 0;
const nextPullId = () => PULL_ID_BASE + pullSeq++;

const DEFAULT_PATH = 'C:\\';

function joinPath(dir: string, name: string): string {
  const sep = dir.includes('/') ? '/' : '\\';
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

function parentDir(path: string): string {
  const clean = path.replace(/[\\/]+$/, '');
  const idx = Math.max(clean.lastIndexOf('\\'), clean.lastIndexOf('/'));
  if (idx < 0) return clean + '\\';
  return clean.slice(0, idx + 1);
}

function formatSize(bytes: number, isDir: boolean): string {
  if (isDir) return '--';
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatDate(ms: number | null): string {
  if (ms === null) return '--';
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const FileIcon: React.FC<{ isDir: boolean; ext: string }> = ({ isDir, ext }) => {
  if (isDir) {
    return (
      <span style={{ color: palette.warning, display: 'flex' }}>
        <FolderRegular fontSize={16} />
      </span>
    );
  }
  if (ext === 'png' || ext === 'jpg' || ext === 'jpeg' || ext === 'webp' || ext === 'gif') {
    return (
      <span style={{ color: palette.primaryActive, display: 'flex' }}>
        <DocumentImageRegular fontSize={16} />
      </span>
    );
  }
  const color =
    ext === 'xlsx' || ext === 'xls'
      ? palette.online
      : ext === 'zip' || ext === 'rar' || ext === '7z'
        ? palette.warning
        : palette.textMuted;
  return (
    <span style={{ color, display: 'flex' }}>
      <DocumentRegular fontSize={16} />
    </span>
  );
};

interface FileTransferPageProps {
  /** 当前连接的远端设备名称（远端面板头部展示） */
  deviceName?: string;
}

/**
 * 文件传输：左「我的电脑」/ 右「远端」双栏 + 底部传输列表队列。
 * 保持真实传输逻辑（listDirectory/sendFile/requestRemoteDir/requestFilePull/进度事件/连接联动）。
 */
export const FileTransferPage: React.FC<FileTransferPageProps> = ({ deviceName }) => {
  const styles = useStyles();
  const [connected, setConnected] = useState(false);
  const [incomingPath, setIncomingPath] = useState('');

  // 本机面板
  const [localPath, setLocalPath] = useState(DEFAULT_PATH);
  const [localEntries, setLocalEntries] = useState<FileEntry[]>([]);
  const [localHistory, setLocalHistory] = useState<string[]>([]);
  const [localSelected, setLocalSelected] = useState<Set<number>>(new Set());

  // 远端面板
  const [remotePath, setRemotePath] = useState(DEFAULT_PATH);
  const [remoteEntries, setRemoteEntries] = useState<FileEntry[]>([]);
  const [remoteError, setRemoteError] = useState<string | null>(null);
  const [remoteHistory, setRemoteHistory] = useState<string[]>([]);
  const [remoteSelected, setRemoteSelected] = useState<Set<number>>(new Set());
  const remoteReqSeq = useRef(0);

  const [tasks, setTasks] = useState<TransferTask[]>([]);

  const refreshLocal = useCallback(async (path: string) => {
    try {
      setLocalEntries(await listDirectory(path));
    } catch (error) {
      console.error('[file-transfer] 读取本机目录失败', error);
      setLocalEntries([]);
    }
  }, []);

  const refreshRemote = useCallback(async (path: string) => {
    const seq = ++remoteReqSeq.current;
    setRemoteError(null);
    try {
      await requestRemoteDir(path);
    } catch (error) {
      console.error('[file-transfer] 请求远端目录失败', error);
      if (remoteReqSeq.current === seq) setRemoteError(String(error));
    }
  }, []);

  useEffect(() => {
    void getConnectionState().then((s) => setConnected(s.connected));
    void getIncomingDir().then(setIncomingPath);
    void refreshLocal(DEFAULT_PATH);
    if (connected) void refreshRemote(DEFAULT_PATH);

    let unlistenProgress: (() => void) | undefined;
    let unlistenRemote: (() => void) | undefined;
    let unlistenConn: (() => void) | undefined;

    void onFileProgress((p) => {
      const key = `${p.direction}:${p.id}`;
      setTasks((prev) => {
        const idx = prev.findIndex((t) => t.key === key);
        if (idx < 0) {
          // 新接收/推送:任务尚不存在则创建
          const isRecv = p.direction === 'recv';
          return [
            {
              key,
              id: p.id,
              name: p.name ?? `文件-${p.id}`,
              direction: p.direction,
              status: 'transferring',
              size: p.total,
              received: p.received,
              sendPath: isRecv ? '远端' : '本机',
              recvPath: isRecv ? incomingPath : '远端',
            },
            ...prev,
          ];
        }
        return prev.map((t) =>
          t.key === key
            ? {
                ...t,
                size: p.total,
                received: p.received,
                status:
                  p.received >= p.total && p.total > 0 ? 'sent' : 'transferring',
              }
            : t,
        );
      });
    }).then((fn) => {
      unlistenProgress = fn;
    });

    void onRemoteDirectory((dir) => {
      if (dir.path !== remotePath) return;
      setRemoteEntries(dir.entries);
      setRemoteError(dir.error);
    }).then((fn) => {
      unlistenRemote = fn;
    });

    void onConnectionStateChange((s) => {
      setConnected(s.connected);
      if (s.connected) void refreshRemote(remotePath);
    }).then((fn) => {
      unlistenConn = fn;
    });

    return () => {
      unlistenProgress?.();
      unlistenRemote?.();
      unlistenConn?.();
    };
  }, [refreshLocal, refreshRemote, remotePath, connected, incomingPath]);

  const goLocal = (path: string, recordHistory: boolean) => {
    if (recordHistory) setLocalHistory((prev) => [...prev, localPath]);
    setLocalPath(path);
    setLocalSelected(new Set());
    void refreshLocal(path);
  };

  const goRemote = (path: string, recordHistory: boolean) => {
    if (recordHistory) setRemoteHistory((prev) => [...prev, remotePath]);
    setRemotePath(path);
    setRemoteSelected(new Set());
    void refreshRemote(path);
  };

  const localBack = () => {
    setLocalHistory((prev) => {
      if (prev.length === 0) return prev;
      const next = [...prev];
      const prevPath = next.pop()!;
      setLocalPath(prevPath);
      setLocalSelected(new Set());
      void refreshLocal(prevPath);
      return next;
    });
  };

  const remoteBack = () => {
    setRemoteHistory((prev) => {
      if (prev.length === 0) return prev;
      const next = [...prev];
      const prevPath = next.pop()!;
      setRemotePath(prevPath);
      setRemoteSelected(new Set());
      void refreshRemote(prevPath);
      return next;
    });
  };

  const canSendToRemote = connected && localSelected.size > 0;
  const canPullToLocal = connected && remoteSelected.size > 0;

  const sendToRemote = async () => {
    const files = localEntries.filter((_, i) => localSelected.has(i)).filter((f) => !f.isDir);
    if (files.length === 0) return;
    for (const f of files) {
      const full = joinPath(localPath, f.name);
      try {
        const id = await sendFile(full);
        setTasks((prev) => [
          {
            key: `send:${id}`,
            id,
            name: f.name,
            direction: 'send',
            status: 'transferring',
            size: f.size,
            received: 0,
            sendPath: full,
            recvPath: '远端',
          },
          ...prev,
        ]);
      } catch (error) {
        console.error(`[file-transfer] 发送 ${f.name} 失败`, error);
      }
    }
    setLocalSelected(new Set());
  };

  const pullToLocal = async () => {
    const files = remoteEntries.filter((_, i) => remoteSelected.has(i)).filter((f) => !f.isDir);
    if (files.length === 0) return;
    for (const f of files) {
      const id = nextPullId();
      const full = joinPath(remotePath, f.name);
      try {
        await requestFilePull(id, full);
        setTasks((prev) => [
          {
            key: `recv:${id}`,
            id,
            name: f.name,
            direction: 'recv',
            status: 'transferring',
            size: f.size,
            received: 0,
            sendPath: `远端 ${full}`,
            recvPath: incomingPath,
          },
          ...prev,
        ]);
      } catch (error) {
        console.error(`[file-transfer] 拉取 ${f.name} 失败`, error);
      }
    }
    setRemoteSelected(new Set());
  };

  const removeTask = (key: string) => setTasks((prev) => prev.filter((t) => t.key !== key));

  const toggleSelect = (set: Set<number>, idx: number): Set<number> => {
    const next = new Set(set);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    return next;
  };

  const completedCount = useMemo(() => tasks.filter((t) => t.status === 'sent').length, [tasks]);

  const pauseAll = () =>
    setTasks((prev) => prev.map((t) => (t.status === 'transferring' ? { ...t, status: 'paused' } : t)));
  const resumeAll = () =>
    setTasks((prev) => prev.map((t) => (t.status === 'paused' ? { ...t, status: 'transferring' } : t)));
  const cancelAll = () =>
    setTasks((prev) => prev.filter((t) => t.status !== 'transferring' && t.status !== 'paused'));
  const clearFinished = () => setTasks((prev) => prev.filter((t) => t.status !== 'sent'));

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>文件传输</h1>
        <span
          className={connected ? `${styles.connectHint} ${styles.connectHintActive}` : styles.connectHint}
        >
          {connected
            ? '已连接 · 支持双向并发传输'
            : '未连接 · 请先进入远程会话后再传输文件'}
        </span>
      </div>

      <div className={styles.transferPanel}>
        {/* 左栏：本机 */}
        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <span className={styles.paneTitle}>我的电脑</span>
            <span className={`${styles.tag} ${styles.tagLocal}`}>本机</span>
            <span className={styles.countText}>{localSelected.size} 个已选</span>
            <button
              type="button"
              className={canSendToRemote ? `${styles.headSendBtn} ${styles.headSendBtnActive}` : styles.headSendBtn}
              disabled={!canSendToRemote}
              onClick={() => void sendToRemote()}
            >
              发送
              <ArrowRightFilled fontSize={12} />
            </button>
          </div>
          <div className={styles.pathBar}>
            <button type="button" className={styles.pathIconBtn} aria-label="后退" disabled={localHistory.length === 0} onClick={localBack}>
              <ArrowLeftRegular fontSize={13} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="向上一级" onClick={() => goLocal(parentDir(localPath), false)}>
              <ArrowUpRegular fontSize={13} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="刷新" onClick={() => void refreshLocal(localPath)}>
              <ArrowSyncRegular fontSize={13} />
            </button>
            <input
              className={styles.pathInput}
              value={localPath}
              onChange={(e) => setLocalPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') goLocal(e.currentTarget.value, true);
              }}
              spellCheck={false}
            />
          </div>
          <div className={styles.fileTableWrap}>
            <table className={styles.fileTable}>
              <thead>
                <tr>
                  <th className={`${styles.th} ${styles.thCheck}`}>
                    <input
                      type="checkbox"
                      checked={localEntries.length > 0 && localSelected.size === localEntries.length}
                      onChange={() =>
                        setLocalSelected(localSelected.size === localEntries.length ? new Set() : new Set(localEntries.map((_, i) => i)))
                      }
                    />
                  </th>
                  <th className={styles.th}>名称</th>
                  <th className={`${styles.th} ${styles.hideSm}`}>修改日期</th>
                  <th className={`${styles.th} ${styles.hideSm}`}>类型</th>
                  <th className={styles.th}>大小</th>
                </tr>
              </thead>
              <tbody>
                {localEntries.map((file, idx) => (
                  <tr
                    key={file.name}
                    className={localSelected.has(idx) ? `${styles.tr} ${styles.trSelected}` : styles.tr}
                    onClick={() => setLocalSelected((prev) => toggleSelect(prev, idx))}
                    onDoubleClick={() => {
                      if (file.isDir) goLocal(joinPath(localPath, file.name), true);
                    }}
                  >
                    <td className={styles.td}>
                      <input
                        type="checkbox"
                        checked={localSelected.has(idx)}
                        onClick={(e) => e.stopPropagation()}
                        onChange={() => setLocalSelected((prev) => toggleSelect(prev, idx))}
                      />
                    </td>
                    <td className={styles.td}>
                      <span className={styles.fileName}>
                        <span className={styles.fileIcon}>
                          <FileIcon isDir={file.isDir} ext={file.ext} />
                        </span>
                        {file.name}
                      </span>
                    </td>
                    <td className={`${styles.td} ${styles.dateCol} ${styles.hideSm}`}>{formatDate(file.modifiedMs)}</td>
                    <td className={`${styles.td} ${styles.typeCol} ${styles.hideSm}`}>{file.isDir ? '文件夹' : file.ext || '文件'}</td>
                    <td className={`${styles.td} ${styles.sizeCol}`}>{formatSize(file.size, file.isDir)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {localEntries.length === 0 && <div className={styles.empty}>本目录为空</div>}
          </div>
        </div>

        {/* 右栏：远端 */}
        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <button
              type="button"
              className={canPullToLocal ? `${styles.headSendBtn} ${styles.headSendBtnActive}` : styles.headSendBtn}
              disabled={!canPullToLocal}
              onClick={() => void pullToLocal()}
            >
              <ArrowLeftFilled fontSize={12} />
              发送
            </button>
            <span className={styles.headGroup}>
              <span className={`${styles.tag} ${styles.tagRemote}`}>远端</span>
              <span className={styles.paneTitle}>{deviceName ?? '远程主机'}</span>
              <span className={styles.countText}>{remoteSelected.size} 个已选</span>
            </span>
          </div>
          <div className={styles.pathBar}>
            <button type="button" className={styles.pathIconBtn} aria-label="后退" disabled={remoteHistory.length === 0} onClick={remoteBack}>
              <ArrowLeftRegular fontSize={13} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="向上一级" onClick={() => goRemote(parentDir(remotePath), false)}>
              <ArrowUpRegular fontSize={13} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="刷新" onClick={() => void refreshRemote(remotePath)}>
              <ArrowSyncRegular fontSize={13} />
            </button>
            <input
              className={styles.pathInput}
              value={remotePath}
              onChange={(e) => setRemotePath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') goRemote(e.currentTarget.value, true);
              }}
              spellCheck={false}
            />
          </div>
          <div className={styles.fileTableWrap}>
            <table className={styles.fileTable}>
              <thead>
                <tr>
                  <th className={`${styles.th} ${styles.thCheck}`} />
                  <th className={styles.th}>名称</th>
                  <th className={`${styles.th} ${styles.hideSm}`}>修改日期</th>
                  <th className={`${styles.th} ${styles.hideSm}`}>类型</th>
                  <th className={styles.th}>大小</th>
                </tr>
              </thead>
              <tbody>
                {!connected && (
                  <tr>
                    <td colSpan={5}>
                      <div className={styles.empty}>未连接，无法浏览远端目录</div>
                    </td>
                  </tr>
                )}
                {connected && remoteError && (
                  <tr>
                    <td colSpan={5}>
                      <div className={styles.empty}>加载失败：{remoteError}</div>
                    </td>
                  </tr>
                )}
                {connected &&
                  !remoteError &&
                  remoteEntries.map((file, idx) => (
                    <tr
                      key={file.name}
                      className={remoteSelected.has(idx) ? `${styles.tr} ${styles.trSelected}` : styles.tr}
                      onClick={() => setRemoteSelected((prev) => toggleSelect(prev, idx))}
                      onDoubleClick={() => {
                        if (file.isDir) goRemote(joinPath(remotePath, file.name), true);
                      }}
                    >
                      <td className={styles.td}>
                        <input
                          type="checkbox"
                          checked={remoteSelected.has(idx)}
                          onClick={(e) => e.stopPropagation()}
                          onChange={() => setRemoteSelected((prev) => toggleSelect(prev, idx))}
                        />
                      </td>
                      <td className={styles.td}>
                        <span className={styles.fileName}>
                          <span className={styles.fileIcon}>
                            <FileIcon isDir={file.isDir} ext={file.ext} />
                          </span>
                          {file.name}
                        </span>
                      </td>
                      <td className={`${styles.td} ${styles.dateCol}`}>{formatDate(file.modifiedMs)}</td>
                      <td className={`${styles.td} ${styles.typeCol}`}>{file.isDir ? '文件夹' : file.ext || '文件'}</td>
                      <td className={`${styles.td} ${styles.sizeCol}`}>{formatSize(file.size, file.isDir)}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
            {connected && !remoteError && remoteEntries.length === 0 && (
              <div className={styles.empty}>本目录为空</div>
            )}
          </div>
        </div>
      </div>

      {/* 底部传输列表 */}
      <div className={styles.taskSection}>
        <div className={styles.taskHeader}>
          <span className={styles.taskTitle}>
            传输列表
            <span className={styles.taskCount}>已传输 {completedCount} 个文件</span>
          </span>
          <div className={styles.batchOps}>
            <button type="button" className={styles.batchBtn} onClick={pauseAll}>
              <PauseRegular fontSize={11} />
              全部暂停
            </button>
            <button type="button" className={styles.batchBtn} onClick={resumeAll}>
              <PlayRegular fontSize={11} />
              全部开始
            </button>
            <button type="button" className={styles.batchBtn} onClick={cancelAll}>
              <DismissRegular fontSize={11} />
              全部取消
            </button>
            <button type="button" className={`${styles.batchBtn} ${styles.batchBtnDanger}`} onClick={clearFinished}>
              <DeleteRegular fontSize={11} />
              清除完结任务
            </button>
          </div>
        </div>
        <div className={styles.taskList}>
          {tasks.map((task) => {
            const percent = task.size > 0 ? Math.min(100, Math.round((task.received / task.size) * 100)) : 0;
            const sendIsRemote = task.sendPath.startsWith('远端');
            const recvIsRemote = task.recvPath.startsWith('远端');
            return (
              <div key={task.key} className={styles.taskRow}>
                <span className={styles.taskNameCol}>
                  <span className={styles.fileIcon}>
                    <DocumentRegular fontSize={15} style={{ color: '#8A94A6' }} />
                  </span>
                  <span
                    style={{
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                      fontWeight: 500,
                      color: '#1F2937',
                    }}
                  >
                    {task.name}
                  </span>
                </span>
                <span className={styles.taskStatusCol}>
                  {task.status === 'sent' && (
                    <span className={styles.taskStatusSent}>
                      <CheckmarkCircleFilled fontSize={14} /> 已发送
                    </span>
                  )}
                  {task.status === 'paused' && <span className={styles.taskStatusPaused}>已暂停</span>}
                  {task.status === 'failed' && <span className={styles.taskStatusFailed}>失败</span>}
                  {task.status === 'transferring' && (
                    <>
                      <span className={styles.progressTrack}>
                        <span className={styles.progressBar} style={{ width: `${percent}%` }} />
                      </span>
                      <span style={{ color: '#8A94A6', fontSize: '11px', whiteSpace: 'nowrap' }}>
                        {formatSize(task.received, false)} / {formatSize(task.size, false)}
                      </span>
                    </>
                  )}
                </span>
                <span className={styles.taskSize}>{formatSize(task.size, false)}</span>
                <span className={styles.pathChip}>
                  <span className={sendIsRemote ? `${styles.chipTag} ${styles.chipTagRemote}` : `${styles.chipTag} ${styles.chipTagLocal}`}>
                    {sendIsRemote ? '远端' : '本机'}
                  </span>
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{task.sendPath}</span>
                </span>
                <span className={styles.pathChip}>
                  <span className={recvIsRemote ? `${styles.chipTag} ${styles.chipTagRemote}` : `${styles.chipTag} ${styles.chipTagLocal}`}>
                    {recvIsRemote ? '远端' : '本机'}
                  </span>
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{task.recvPath}</span>
                </span>
                <span className={styles.speedBadge}>{task.direction === 'send' ? '↑' : '↓'} {percent}%</span>
                <button
                  type="button"
                  className={styles.taskDel}
                  onClick={() => removeTask(task.key)}
                  aria-label="删除任务"
                >
                  <DeleteRegular fontSize={13} />
                </button>
              </div>
            );
          })}
          {tasks.length === 0 && <div className={styles.empty}>暂无传输任务</div>}
        </div>
      </div>
    </div>
  );
};

export default FileTransferPage;
