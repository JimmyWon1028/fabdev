export function isWindowsPlatform(
  userAgent = typeof navigator === 'undefined' ? '' : navigator.userAgent
): boolean {
  return /Windows/i.test(userAgent)
}

export function formatPathForDisplay(
  path: string,
  windows = isWindowsPlatform()
): string {
  if (!windows) {
    return path
  }

  const withoutDevicePrefix = path.startsWith('\\\\?\\UNC\\')
    ? '\\\\' + path.slice('\\\\?\\UNC\\'.length)
    : path.replace(/^\\\\\?\\/, '')
  return withoutDevicePrefix.replaceAll('/', '\\')
}
