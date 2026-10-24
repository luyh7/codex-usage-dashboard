// Keep runtime/updater platform keys stable; use familiar OS names in downloads.
export const downloadPlatform = key => key.replace(/^darwin-/, 'macos-');
export const nativeAssetName = (version, key) =>
  `cousash-${version}-cli-${downloadPlatform(key)}${key.startsWith('windows-') ? '.exe' : ''}`;
export const desktopAssetName = (version, key, suffix) =>
  `cousash-${version}-${downloadPlatform(key)}${key.startsWith('windows-') && suffix === '.exe' ? '-setup' : ''}${suffix}`;
