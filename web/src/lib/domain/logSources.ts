export function displaySourcePath(path: string): string {
  if (/^\\\\\?\\UNC\\/i.test(path)) return `\\\\${path.slice(8)}`;
  if (path.startsWith('\\\\?\\')) return path.slice(4);
  return path;
}

export function sourcePathKey(path: string): string {
  const display = displaySourcePath(path);
  if (/^(?:[a-z]:[\\/]|\\\\)/i.test(display)) {
    const normalized = display.replace(/\//g, '\\').replace(/\\+$/, '');
    if (/^\\\\(?:wsl\$|wsl\.localhost)\\/i.test(normalized)) {
      return normalized.replace(/^\\\\(?:wsl\$|wsl\.localhost)\\([^\\]+)/i,
        (_, distro: string) => `\\\\wsl$\\${distro.toLowerCase()}`);
    }
    return normalized.toLowerCase();
  }
  return display.length > 1 ? display.replace(/\/+$/, '') : display;
}
