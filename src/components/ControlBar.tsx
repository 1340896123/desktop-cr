import React from 'react';
import { Button, makeStyles, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Tooltip, tokens } from '@fluentui/react-components';
import {
  FullScreenMaximizeRegular,
  FullScreenMinimizeRegular,
  ClipboardRegular,
  SettingsRegular,
  VideoRegular,
  ImageRegular,
} from '@fluentui/react-icons';
import { setFullscreen, setQuality, setResolution, syncClipboard } from '../services/connection';

const useStyles = makeStyles({
  bar: {
    position: 'absolute',
    top: '12px',
    left: '50%',
    transform: 'translateX(-50%)',
    display: 'flex',
    alignItems: 'center',
    gap: '4px',
    padding: '4px',
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: '8px',
    boxShadow: tokens.shadow4,
    zIndex: 10,
  },
});

interface ControlBarProps {
  isFullscreen: boolean;
  onToggleFullscreen: () => void;
  onOpenSettings: () => void;
}

/**
 * 顶部悬浮工具栏：画质 / 分辨率 / 全屏 / 剪贴板等操作入口。
 * 当前阶段以 UI 为主，动作通过 services/connection 封装转发到 Rust 命令。
 */
export const ControlBar: React.FC<ControlBarProps> = ({
  isFullscreen,
  onToggleFullscreen,
  onOpenSettings,
}) => {
  const styles = useStyles();

  return (
    <div className={styles.bar}>
      <Menu>
        <MenuTrigger disableButtonEnhancement>
          <Tooltip content="画质" relationship="label">
            <Button icon={<VideoRegular />} appearance="subtle" aria-label="画质" />
          </Tooltip>
        </MenuTrigger>
        <MenuPopover>
          <MenuList>
            <MenuItem
              onClick={() =>
                void setQuality({ fps: 30, quality: 'low' })
              }
            >
              流畅（低画质）
            </MenuItem>
            <MenuItem
              onClick={() =>
                void setQuality({ fps: 30, quality: 'medium' })
              }
            >
              平衡
            </MenuItem>
            <MenuItem
              onClick={() =>
                void setQuality({ fps: 60, quality: 'high' })
              }
            >
              高清（60fps）
            </MenuItem>
          </MenuList>
        </MenuPopover>
      </Menu>

      <Menu>
        <MenuTrigger disableButtonEnhancement>
          <Tooltip content="分辨率" relationship="label">
            <Button icon={<ImageRegular />} appearance="subtle" aria-label="分辨率" />
          </Tooltip>
        </MenuTrigger>
        <MenuPopover>
          <MenuList>
            {[
              { w: 1920, h: 1080 },
              { w: 2560, h: 1440 },
              { w: 3840, h: 2160 },
            ].map((r) => (
              <MenuItem
                key={`${r.w}x${r.h}`}
                onClick={() =>
                  void setResolution({ width: r.w, height: r.h, fps: 60 })
                }
              >
                {r.w} x {r.h}
              </MenuItem>
            ))}
          </MenuList>
        </MenuPopover>
      </Menu>

      <Tooltip content={isFullscreen ? '退出全屏' : '全屏'} relationship="label">
        <Button
          icon={isFullscreen ? <FullScreenMinimizeRegular /> : <FullScreenMaximizeRegular />}
          appearance="subtle"
          onClick={() => {
            void setFullscreen(!isFullscreen);
            onToggleFullscreen();
          }}
          aria-label="全屏"
        />
      </Tooltip>

      <Tooltip content="剪贴板同步" relationship="label">
        <Button
          icon={<ClipboardRegular />}
          appearance="subtle"
          onClick={() => void syncClipboard()}
          aria-label="剪贴板同步"
        />
      </Tooltip>

      <Tooltip content="设置" relationship="label">
        <Button icon={<SettingsRegular />} appearance="subtle" onClick={onOpenSettings} aria-label="设置" />
      </Tooltip>
    </div>
  );
};

export default ControlBar;
