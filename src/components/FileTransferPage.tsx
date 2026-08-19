import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { makeStyles } from '@fluentui/react-components';
import {
  ArrowLeftRegular,
  ArrowUpRegular,
  ArrowSyncRegular,
  ChevronDownRegular,
  ArrowRightFilled,
  ArrowLeftFilled,
  FolderRegular,
  DocumentRegular,
  CheckmarkCircleFilled,
  DeleteRegular,
  DismissRegular,
  ImageRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, radius } from '../theme/tokens';
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
    overflowY: 'auto',
    padding: `${spacing.xl}px ${spacing.xxl}px`,
    display: 'flex',
    flexDirection: 'column',
    gap: `${spacing.sm}px`,
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: '4px',
  },
  title: {
    fontFamily,
    fontSize: '24px',
    fontWeight: 700,
    color: palette.textPrimary,
    letterSpacing: '-0.02em',
    margin: 0,
  },
  connectHint: {
    fontFamily,
    fontSize: '13px',
    color: palette.textMuted,
    background: palette.muted,
    borderRadius: radius.pill,
    padding: '6px 14px',
    maxWidth: '420px',
    textAlign: 'center',
  },
  connectHintActive: {
    color: '#1E7D43',
    background: '#E7F7EC',
  },
  transferPanel: {
    display: 'flex',
    gap: '12px',
    minHeight: '360px',
    flex: '1 1 55%',
  },
  pane: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.card,
    overflow: 'hidden',
    minWidth: 0,
  },
  paneHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 14px',
    borderBottom: `1px solid ${palette.borderLight}`,
  },
  paneTitle: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  tag: {
    fontSize: '11px',
    lineHeight: '16px',
    padding: '1px 8px',
    borderRadius: radius.pill,
    fontWeight: 600,
  },
  tagLocal: {
    backgroundColor: palette.muted,
    color: palette.textSecondary,
  },
  tagRemote: {
    backgroundColor: '#E7F7EC',
    color: '#1E7D43',
  },
  paneSub: {
    marginLeft: 'auto',
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
  },
  pathBar: {
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
    padding: '6px 10px',
    borderBottom: `1px solid ${palette.borderLight}`,
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
    color: palette.textSecondary,
    cursor: 'pointer',
    transition: 'background-color 150ms ease, color 150ms ease',
    flexShrink: 0,

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },

    '&:disabled': {
      color: palette.textMuted,
      cursor: 'not-allowed',
      opacity: 0.5,
    },
  },
  pathInputWrap: {
    flex: 1,
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    height: '28px',
    padding: '0 10px',
    backgroundColor: palette.background,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.control,
    minWidth: 0,
  },
  pathInput: {
    flex: 1,
    minWidth: 0,
    border: 'none',
    outline: 'none',
    background: 'transparent',
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
  },
  pathText: {
    flex: 1,
    fontFamily,
    fontSize: '12px',
    color: palette.textSecondary,
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  fileList: {
    flex: 1,
    overflowY: 'auto',
    fontFamily,
    fontSize: '13px',
    color: palette.textPrimary,
  },
  listHead: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '6px 12px',
    color: palette.textMuted,
    fontSize: '12px',
    borderBottom: `1px solid ${palette.borderLight}`,
  },
  colName: {
    flex: '1 1 48%',
    minWidth: 0,
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
  },
  colDate: {
    flex: '1 1 24%',
    minWidth: 0,
  },
  colType: {
    flex: '0 0 48px',
  },
  colSize: {
    flex: '0 0 76px',
    textAlign: 'right',
  },
  fileRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '5px 12px',
    cursor: 'pointer',
    transition: 'background-color 120ms ease',
    borderBottom: `1px solid rgba(229, 231, 235, 0.4)`,

    '&:hover': {
      backgroundColor: palette.muted,
    },
  },
  fileRowSelected: {
    backgroundColor: palette.primarySoft,
    '&:hover': {
      backgroundColor: palette.primarySoft,
    },
  },
  fileName: {
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  fileIcon: {
    flexShrink: 0,
    display: 'flex',
  },
  middle: {
    display: 'flex',
    flexDirection: 'column',
    justifyContent: 'center',
    gap: '8px',
    padding: '0 2px',
  },
  transferBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '4px',
    padding: '8px 12px',
    borderRadius: radius.control,
    border: `1px solid ${palette.border}`,
    backgroundColor: palette.backgroundElevated,
    color: palette.textMuted,
    fontFamily,
    fontSize: '13px',
    cursor: 'not-allowed',
    whiteSpace: 'nowrap',
  },
  transferBtnActive: {
    border: `1px solid ${palette.primary}`,
    backgroundColor: palette.primary,
    color: '#fff',
    cursor: 'pointer',
    boxShadow: 'none',

    '&:hover': {
      backgroundColor: palette.primaryHover,
    },
  },
  taskSection: {
    flex: '1 1 45%',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: palette.backgroundElevated,
    border: `1px solid ${palette.borderLight}`,
    borderRadius: radius.card,
    overflow: 'hidden',
    minHeight: '180px',
  },
  taskHeader: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '10px 14px',
    borderBottom: `1px solid ${palette.borderLight}`,
  },
  taskTitle: {
    fontFamily,
    fontSize: '14px',
    fontWeight: 600,
    color: palette.textPrimary,
  },
  taskCount: {
    fontFamily,
    fontSize: '12px',
    color: palette.textMuted,
  },
  batchOps: {
    marginLeft: 'auto',
    display: 'flex',
    alignItems: 'center',
    gap: '2px',
  },
  batchBtn: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '4px',
    padding: '5px 10px',
    border: 'none',
    background: 'transparent',
    borderRadius: radius.control,
    color: palette.textSecondary,
    fontFamily,
    fontSize: '12px',
    cursor: 'pointer',
    transition: 'background-color 150ms ease',

    '&:hover': {
      backgroundColor: palette.muted,
      color: palette.textPrimary,
    },
  },
  taskTable: {
    flex: 1,
    overflowY: 'auto',
    fontFamily,
    fontSize: '13px',
  },
  taskRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '7px 14px',
    borderBottom: `1px solid rgba(229, 231, 235, 0.4)`,

    '&:hover': {
      backgroundColor: palette.muted,
    },
  },
  taskName: {
    flex: '1 1 28%',
    minWidth: 0,
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
  },
  taskStatus: {
    flex: '0 0 160px',
    display: 'flex',
    flexDirection: 'column',
    gap: '3px',
    color: '#1E7D43',
    fontSize: '12px',
    fontWeight: 500,
  },
  progressTrack: {
    width: '100%',
    height: '4px',
    borderRadius: radius.pill,
    backgroundColor: palette.muted,
    overflow: 'hidden',
  },
  progressBar: {
    height: '100%',
    borderRadius: radius.pill,
    backgroundColor: palette.primary,
    transition: 'width 120ms linear',
  },
  taskSize: {
    flex: '0 0 70px',
    textAlign: 'right',
    color: palette.textSecondary,
    fontSize: '12px',
  },
  taskPath: {
    flex: '1 1 20%',
    minWidth: 0,
    color: palette.textMuted,
    fontSize: '12px',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  taskDel: {
    flexShrink: 0,
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

    '&:hover': {
      backgroundColor: 'rgba(220, 38, 38, 0.1)',
      color: palette.destructive,
    },
  },
  empty: {
    padding: '32px 16px',
    textAlign: 'center',
    color: palette.textMuted,
    fontFamily,
    fontSize: '13px',
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
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const FileIcon: React.FC<{ isDir: boolean; ext: string }> = ({ isDir, ext }) => {
  if (isDir) {
    return (
      <span style={{ color: '#F5A623', display: 'flex' }}>
        <FolderRegular fontSize={16} />
      </span>
    );
  }
  if (ext === 'png' || ext === 'jpg' || ext === 'jpeg' || ext === 'webp' || ext === 'gif') {
    return (
      <span style={{ color: '#5DA8FF', display: 'flex' }}>
        <ImageRegular fontSize={16} />
      </span>
    );
  }
  return (
    <span
      style={{
        color:
          ext === 'xlsx' || ext === 'xls'
            ? '#1E7D43'
            : ext === 'zip' || ext === 'rar' || ext === '7z'
              ? '#D97706'
              : '#8A94A6',
        display: 'flex',
      }}
    >
      <DocumentRegular fontSize={16} />
    </span>
  );
};

export const FileTransferPage: React.FC = () => {
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
        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <span className={styles.paneTitle}>我的电脑</span>
            <span className={`${styles.tag} ${styles.tagLocal}`}>本机</span>
            <span className={styles.paneSub}>{localSelected.size} 个已选</span>
          </div>
          <div className={styles.pathBar}>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="后退"
              disabled={localHistory.length === 0}
              onClick={localBack}
            >
              <ArrowLeftRegular fontSize={14} />
            </button>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="向上一级"
              onClick={() => goLocal(parentDir(localPath), false)}
            >
              <ArrowUpRegular fontSize={14} />
            </button>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="刷新"
              onClick={() => void refreshLocal(localPath)}
            >
              <ArrowSyncRegular fontSize={14} />
            </button>
            <div className={styles.pathInputWrap}>
              <span className={styles.pathText}>{localPath}</span>
              <ChevronDownRegular fontSize={12} style={{ color: palette.textMuted, flexShrink: 0 }} />
            </div>
          </div>
          <div className={styles.fileList}>
            <div className={styles.listHead}>
              <span className={styles.colName}>名称</span>
              <span className={styles.colDate}>修改日期</span>
              <span className={styles.colType}>类型</span>
              <span className={styles.colSize}>大小</span>
            </div>
            {localEntries.map((file, idx) => (
              <div
                key={file.name}
                className={
                  localSelected.has(idx) ? `${styles.fileRow} ${styles.fileRowSelected}` : styles.fileRow
                }
                onClick={() => setLocalSelected((prev) => toggleSelect(prev, idx))}
                onDoubleClick={() => {
                  if (file.isDir) goLocal(joinPath(localPath, file.name), true);
                }}
              >
                <span className={styles.colName}>
                  <span className={styles.fileIcon}>
                    <FileIcon isDir={file.isDir} ext={file.ext} />
                  </span>
                  <span className={styles.fileName}>{file.name}</span>
                </span>
                <span className={styles.colDate}>{formatDate(file.modifiedMs)}</span>
                <span className={styles.colType}>{file.isDir ? '文件夹' : file.ext || '文件'}</span>
                <span className={styles.colSize}>{formatSize(file.size, file.isDir)}</span>
              </div>
            ))}
            {localEntries.length === 0 && <div className={styles.empty}>本目录为空</div>}
          </div>
        </div>

        <div className={styles.middle}>
          <button
            type="button"
            className={canSendToRemote ? `${styles.transferBtn} ${styles.transferBtnActive}` : styles.transferBtn}
            disabled={!canSendToRemote}
            onClick={() => void sendToRemote()}
          >
            发送
            <ArrowRightFilled fontSize={12} />
          </button>
          <button
            type="button"
            className={canPullToLocal ? `${styles.transferBtn} ${styles.transferBtnActive}` : styles.transferBtn}
            disabled={!canPullToLocal}
            onClick={() => void pullToLocal()}
          >
            <ArrowLeftFilled fontSize={12} />
            发送
          </button>
        </div>

        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <span className={`${styles.tag} ${styles.tagRemote}`}>远端</span>
            <span className={styles.paneTitle}>远程主机</span>
            <span className={styles.paneSub}>{remoteSelected.size} 个已选</span>
          </div>
          <div className={styles.pathBar}>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="后退"
              disabled={remoteHistory.length === 0}
              onClick={remoteBack}
            >
              <ArrowLeftRegular fontSize={14} />
            </button>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="向上一级"
              onClick={() => goRemote(parentDir(remotePath), false)}
            >
              <ArrowUpRegular fontSize={14} />
            </button>
            <button
              type="button"
              className={styles.pathIconBtn}
              aria-label="刷新"
              onClick={() => void refreshRemote(remotePath)}
            >
              <ArrowSyncRegular fontSize={14} />
            </button>
            <div className={styles.pathInputWrap}>
              <span className={styles.pathText}>{remotePath}</span>
              <ChevronDownRegular fontSize={12} style={{ color: palette.textMuted, flexShrink: 0 }} />
            </div>
          </div>
          <div className={styles.fileList}>
            <div className={styles.listHead}>
              <span className={styles.colName}>名称</span>
              <span className={styles.colDate}>修改日期</span>
              <span className={styles.colType}>类型</span>
              <span className={styles.colSize}>大小</span>
            </div>
            {!connected && <div className={styles.empty}>未连接,无法浏览远端目录</div>}
            {connected && remoteError && <div className={styles.empty}>加载失败:{remoteError}</div>}
            {connected &&
              !remoteError &&
              remoteEntries.map((file, idx) => (
                <div
                  key={file.name}
                  className={
                    remoteSelected.has(idx) ? `${styles.fileRow} ${styles.fileRowSelected}` : styles.fileRow
                  }
                  onClick={() => setRemoteSelected((prev) => toggleSelect(prev, idx))}
                  onDoubleClick={() => {
                    if (file.isDir) goRemote(joinPath(remotePath, file.name), true);
                  }}
                >
                  <span className={styles.colName}>
                    <span className={styles.fileIcon}>
                      <FileIcon isDir={file.isDir} ext={file.ext} />
                    </span>
                    <span className={styles.fileName}>{file.name}</span>
                  </span>
                  <span className={styles.colDate}>{formatDate(file.modifiedMs)}</span>
                  <span className={styles.colType}>{file.isDir ? '文件夹' : file.ext || '文件'}</span>
                  <span className={styles.colSize}>{formatSize(file.size, file.isDir)}</span>
                </div>
              ))}
            {connected && !remoteError && remoteEntries.length === 0 && (
              <div className={styles.empty}>本目录为空</div>
            )}
          </div>
        </div>
      </div>

      <div className={styles.taskSection}>
        <div className={styles.taskHeader}>
          <span className={styles.taskTitle}>传输列表</span>
          <span className={styles.taskCount}>已完成 {completedCount} / {tasks.length}</span>
          <div className={styles.batchOps}>
            <button
              type="button"
              className={styles.batchBtn}
              onClick={() => setTasks((prev) => prev.filter((t) => t.status !== 'sent'))}
            >
              <CheckmarkCircleFilled fontSize={12} />
              清除完结任务
            </button>
            <button
              type="button"
              className={styles.batchBtn}
              onClick={() => setTasks([])}
            >
              <DismissRegular fontSize={12} />
              全部清除
            </button>
          </div>
        </div>
        <div className={styles.taskTable}>
          <div className={styles.listHead}>
            <span className={styles.taskName}>名称</span>
            <span className={styles.taskStatus}>状态</span>
            <span className={styles.taskSize}>大小</span>
            <span className={styles.taskPath}>发送路径</span>
            <span className={styles.taskPath}>接收路径</span>
            <span style={{ flex: '0 0 30px' }} />
          </div>
          {tasks.map((task) => {
            const percent = task.size > 0 ? Math.min(100, Math.round((task.received / task.size) * 100)) : 0;
            return (
              <div key={task.key} className={styles.taskRow}>
                <span className={styles.taskName}>
                  <span className={styles.fileIcon}>
                    <DocumentRegular fontSize={15} style={{ color: palette.textMuted }} />
                  </span>
                  <span className={styles.fileName}>{task.name}</span>
                </span>
                <span className={styles.taskStatus}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    {task.status === 'sent' && <CheckmarkCircleFilled fontSize={14} />}
                    {task.status === 'sent' ? '已完成' : task.status === 'transferring' ? '传输中' : task.status}
                  </span>
                  {task.status === 'transferring' && (
                    <span className={styles.progressTrack}>
                      <span className={styles.progressBar} style={{ width: `${percent}%` }} />
                    </span>
                  )}
                  {task.status === 'transferring' && (
                    <span style={{ color: palette.textMuted, fontSize: '11px', fontWeight: 400 }}>
                      {formatSize(task.received, false)} / {formatSize(task.size, false)}
                    </span>
                  )}
                </span>
                <span className={styles.taskSize}>{formatSize(task.size, false)}</span>
                <span className={styles.taskPath}>{task.sendPath}</span>
                <span className={styles.taskPath}>{task.recvPath}</span>
                <span style={{ flex: '0 0 30px' }}>
                  <button
                    type="button"
                    className={styles.taskDel}
                    onClick={() => removeTask(task.key)}
                    aria-label="删除任务"
                  >
                    <DeleteRegular fontSize={14} />
                  </button>
                </span>
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