import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isTauri } from './connection';

/** 账号登录会话(与 Rust 侧 AccountSession 对应) */
export interface AccountSession {
  server: string;
  username: string;
  token: string;
}

/** 登录 dcr-signal 账号服务;成功后返回会话 */
export async function loginAccount(
  server: string,
  username: string,
  password: string,
): Promise<AccountSession> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,模拟登录成功');
    return { server, username, token: 'mock-token' };
  }
  return invoke<AccountSession>('login_account', { server, username, password });
}

/** 注册 dcr-signal 账号;成功后自动签发令牌并返回会话(注册即登录) */
export async function registerAccount(
  server: string,
  username: string,
  password: string,
): Promise<AccountSession> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,模拟注册成功');
    return { server, username, token: 'mock-token' };
  }
  return invoke<AccountSession>('register_account', { server, username, password });
}

/** 校验令牌是否仍有效(应用启动时调用);令牌失效时抛错 */
export async function checkAccountToken(session: AccountSession): Promise<string> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,跳过令牌校验');
    return session.username;
  }
  return invoke<string>('check_account_token', { session });
}

/** 退出登录(清除本地会话) */
export async function logoutAccount(): Promise<void> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,跳过退出登录');
    return;
  }
  await invoke('logout_account');
}

/** 读取当前登录会话;未登录返回 null */
export async function getAccount(): Promise<AccountSession | null> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,返回空会话');
    return null;
  }
  const account = await invoke<AccountSession | null>('get_account');
  return account ?? null;
}

/**
 * 订阅令牌失效事件:信令注册/设备列表被服务端以「令牌无效」拒绝时触发,
 * 前端应清除会话并强制重新登录(否则令牌过期后「我的设备」会静默为空)。
 */
export async function onAuthExpired(handler: () => void): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.warn('[auth] 非 Tauri 环境,跳过令牌失效订阅');
    return () => {
      /* noop */
    };
  }
  return listen('auth-expired', () => handler());
}