type NativeWindow = Window & { __TAURI__?: { core: { invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T> } } };
export const isDesktop = () => !!(window as NativeWindow).__TAURI__;
export function desktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const bridge = (window as NativeWindow).__TAURI__;
  if (!bridge) return Promise.reject(new Error('Desktop client required'));
  return bridge.core.invoke<T>(command, args);
}
