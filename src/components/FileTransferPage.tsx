import React, { useState } from 'react';
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
  PauseRegular,
  PlayRegular,
  DismissRegular,
  ImageRegular,
} from '@fluentui/react-icons';
import { palette, fontFamily, spacing, radius } from '../theme/tokens';

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
    flex: '1 1 30%',
    minWidth: 0,
    display: 'flex',
    alignItems: 'center',
    gap: '6px',
  },
  taskStatus: {
    flex: '0 0 88px',
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    color: '#1E7D43',
    fontSize: '12px',
    fontWeight: 500,
  },
  taskSize: {
    flex: '0 0 70px',
    textAlign: 'right',
    color: palette.textSecondary,
    fontSize: '12px',
  },
  taskPath: {
    flex: '1 1 24%',
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

type FileKind = 'folder' | 'file';

interface RemoteFile {
  name: string;
  kind: FileKind;
  modified: string;
  type: string;
  size: string;
}

type TaskStatus = 'sent' | 'transferring' | 'paused' | 'failed';

interface TransferTask {
  id: number;
  name: string;
  status: TaskStatus;
  size: string;
  sendPath: string;
  recvPath: string;
}

const localPath = 'F:\\desktop-cr\\client\\target\\release\\bundle\\nsis';

const remoteFiles: RemoteFile[] = [
  { name: '.env.development.local', kind: 'file', modified: '2026-07-10 09:12', type: 'local', size: '1.03 KB' },
  { name: '1、本报告书仅限企业每年向市场监管管理部门报送年度报告,并向...', kind: 'file', modified: '2026-07-15 11:30', type: 'txt', size: '4.71 KB' },
  { name: '20260704培训', kind: 'folder', modified: '2026-07-04 15:20', type: '文件夹', size: '--' },
  { name: '20260704培训.rar', kind: 'file', modified: '2026-07-04 15:22', type: 'rar', size: '39.70 MB' },
  { name: '23年-26年度离职人员情况表.xlsx', kind: 'file', modified: '2026-07-13 09:45', type: 'xlsx', size: '13.53 KB' },
  { name: 'Antigravity.lnk', kind: 'file', modified: '2026-07-02 17:08', type: '文件夹', size: '--' },
  { name: 'Aras SetWorkflowPath.exe', kind: 'file', modified: '2026-06-28 14:02', type: 'exe', size: '149.05 MB' },
  { name: 'Aras_AML执行结果_20260715_110008.csv', kind: 'file', modified: '2026-07-15 11:00', type: 'csv', size: '1.00 KB' },
  { name: 'BOM位号修复脚本.txt', kind: 'file', modified: '2026-07-12 10:18', type: 'txt', size: '5.19 KB' },
  { name: 'BOM导出20260629160406.zip', kind: 'file', modified: '2026-06-29 16:04', type: 'zip', size: '3.12 MB' },
  { name: 'BOM导出20260702115629.zip', kind: 'file', modified: '2026-07-02 11:56', type: 'zip', size: '60.15 KB' },
  { name: 'BOM导出20260714131525', kind: 'folder', modified: '2026-07-14 13:15', type: '文件夹', size: '--' },
  { name: 'BOM导出20260714131838', kind: 'folder', modified: '2026-07-14 13:18', type: '文件夹', size: '--' },
  { name: 'BOM搬转.json', kind: 'file', modified: '2026-07-09 16:41', type: 'json', size: '139.66 KB' },
];

const remotePath = 'E:\\Desktop';
const remoteName = 'AAAAA';

const initialTasks: TransferTask[] = [
  {
    id: 1,
    name: 'DesktopCR_0.1.0_x64-setup.exe',
    status: 'sent',
    size: '4.50 MB',
    sendPath: '本机 F:\\desktop-cr\\client\\target\\release\\bundle\\nsis\\DesktopCR_0.1.0_x6...',
    recvPath: '远端 E:\\Desktop',
  },
  {
    id: 2,
    name: '2025年终总结 - 廖宇杰.pptx',
    status: 'sent',
    size: '1.74 MB',
    sendPath: '本机 E:\\Desktop\\2025年终总结 - 廖宇杰.pptx',
    recvPath: '远端 E:\\Desktop',
  },
];

function formatSize(size: string, kind: FileKind): string {
  if (kind === 'folder') return '--';
  return size;
}

const FileIcon: React.FC<{ kind: FileKind; type: string }> = ({ kind, type }) => {
  if (kind === 'folder') {
    return (
      <span style={{ color: '#F5A623', display: 'flex' }}>
        <FolderRegular fontSize={16} />
      </span>
    );
  }
  if (type === 'png' || type === 'jpg' || type === 'webp') {
    return (
      <span style={{ color: '#5DA8FF', display: 'flex' }}>
        <ImageRegular fontSize={16} />
      </span>
    );
  }
  return (
    <span style={{ color: type === 'xlsx' ? '#1E7D43' : type === 'zip' || type === 'rar' ? '#D97706' : '#8A94A6', display: 'flex' }}>
      <DocumentRegular fontSize={16} />
    </span>
  );
};

/**
 * 截图「文件传输」界面：上部分为 本机/远端 双栏文件列表 + 双向发送按钮，
 * 下部分为传输任务列表（名称/状态/大小/发送路径/接收路径/操作）。
 */
export const FileTransferPage: React.FC = () => {
  const styles = useStyles();
  const [localSelected, setLocalSelected] = useState<Set<number>>(new Set());
  const [remoteSelected, setRemoteSelected] = useState<Set<number>>(new Set());
  const [tasks, setTasks] = useState<TransferTask[]>(initialTasks);

  const canSend = remoteSelected.size > 0 || localSelected.size > 0;

  const sendToRemote = () => {
    if (remoteSelected.size === 0) return;
    const names = remoteFiles.filter((_, i) => remoteSelected.has(i)).map((f) => f.name);
    setTasks((prev) => [
      ...names.map((name, idx) => ({
        id: Date.now() + idx,
        name,
        status: 'sent' as TaskStatus,
        size: '--',
        sendPath: `远端 ${remotePath}`,
        recvPath: '本机 (目标)',
      })),
      ...prev,
    ]);
    setRemoteSelected(new Set());
  };

  const sendToLocal = () => {
    if (localSelected.size === 0) return;
    setLocalSelected(new Set());
  };

  const removeTask = (id: number) => setTasks((prev) => prev.filter((t) => t.id !== id));

  const toggleSelect = (set: Set<number>, idx: number): Set<number> => {
    const next = new Set(set);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    return next;
  };

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>文件传输</h1>
      </div>

      <div className={styles.transferPanel}>
        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <span className={styles.paneTitle}>我的电脑</span>
            <span className={`${styles.tag} ${styles.tagLocal}`}>本机</span>
            <span className={styles.paneSub}>0 个已选</span>
          </div>
          <div className={styles.pathBar}>
            <button type="button" className={styles.pathIconBtn} aria-label="后退">
              <ArrowLeftRegular fontSize={14} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="向上一级">
              <ArrowUpRegular fontSize={14} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="刷新">
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
            <div className={styles.empty}>本目录为空</div>
          </div>
        </div>

        <div className={styles.middle}>
          <button
            type="button"
            className={canSend ? `${styles.transferBtn} ${styles.transferBtnActive}` : styles.transferBtn}
            disabled={!canSend}
            onClick={() => void sendToRemote()}
          >
            发送
            <ArrowRightFilled fontSize={12} />
          </button>
          <button
            type="button"
            className={canSend ? `${styles.transferBtn} ${styles.transferBtnActive}` : styles.transferBtn}
            disabled={!canSend}
            onClick={() => void sendToLocal()}
          >
            <ArrowLeftFilled fontSize={12} />
            发送
          </button>
        </div>

        <div className={styles.pane}>
          <div className={styles.paneHeader}>
            <span className={`${styles.tag} ${styles.tagRemote}`}>远端</span>
            <span className={styles.paneTitle}>{remoteName}</span>
            <span className={styles.paneSub}>{remoteSelected.size} 个已选</span>
          </div>
          <div className={styles.pathBar}>
            <button type="button" className={styles.pathIconBtn} aria-label="后退">
              <ArrowLeftRegular fontSize={14} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="向上一级">
              <ArrowUpRegular fontSize={14} />
            </button>
            <button type="button" className={styles.pathIconBtn} aria-label="刷新">
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
            {remoteFiles.map((file, idx) => (
              <div
                key={file.name}
                className={remoteSelected.has(idx) ? `${styles.fileRow} ${styles.fileRowSelected}` : styles.fileRow}
                onClick={() => setRemoteSelected((prev) => toggleSelect(prev, idx))}
              >
                <span className={styles.colName}>
                  <span className={styles.fileIcon}>
                    <FileIcon kind={file.kind} type={file.type} />
                  </span>
                  <span className={styles.fileName}>{file.name}</span>
                </span>
                <span className={styles.colDate}>{file.modified}</span>
                <span className={styles.colType}>{file.type}</span>
                <span className={styles.colSize}>{formatSize(file.size, file.kind)}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className={styles.taskSection}>
        <div className={styles.taskHeader}>
          <span className={styles.taskTitle}>传输列表</span>
          <span className={styles.taskCount}>已传输 {tasks.length} 个文件</span>
          <div className={styles.batchOps}>
            <button type="button" className={styles.batchBtn}>
              <PauseRegular fontSize={12} />
              全部暂停
            </button>
            <button type="button" className={styles.batchBtn}>
              <PlayRegular fontSize={12} />
              全部开始
            </button>
            <button type="button" className={styles.batchBtn}>
              <DismissRegular fontSize={12} />
              全部取消
            </button>
            <button
              type="button"
              className={styles.batchBtn}
              onClick={() => setTasks((prev) => prev.filter((t) => t.status !== 'sent'))}
            >
              <CheckmarkCircleFilled fontSize={12} />
              清除完结任务
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
          {tasks.map((task) => (
            <div key={task.id} className={styles.taskRow}>
              <span className={styles.taskName}>
                <span className={styles.fileIcon}>
                  <DocumentRegular fontSize={15} style={{ color: palette.textMuted }} />
                </span>
                <span className={styles.fileName}>{task.name}</span>
              </span>
              <span className={styles.taskStatus}>
                {task.status === 'sent' && <CheckmarkCircleFilled fontSize={14} />}
                {task.status === 'sent' ? '已发送' : task.status}
              </span>
              <span className={styles.taskSize}>{task.size}</span>
              <span className={styles.taskPath}>{task.sendPath}</span>
              <span className={styles.taskPath}>{task.recvPath}</span>
              <span style={{ flex: '0 0 30px' }}>
                <button type="button" className={styles.taskDel} onClick={() => removeTask(task.id)} aria-label="删除任务">
                  <DeleteRegular fontSize={14} />
                </button>
              </span>
            </div>
          ))}
          {tasks.length === 0 && <div className={styles.empty}>暂无传输任务</div>}
        </div>
      </div>
    </div>
  );
};

export default FileTransferPage;
