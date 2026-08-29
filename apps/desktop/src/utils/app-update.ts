export interface AppUpdateArtifact {
  platform: string
  architecture: string
  minimumOsVersion: string
  fileName: string
  size: number
  sha256: string
  installMode: string
}

export interface AppUpdateCheck {
  currentVersion: string
  latestVersion: string
  updateAvailable: boolean
  publishedAt: string
  releaseUrl: string
  releaseNotesUrl: string
  unsignedCommunityBuild: boolean
  artifact: AppUpdateArtifact
}

export interface DownloadedAppUpdate {
  version: string
  fileName: string
  size: number
  sha256: string
}

export interface AppUpdateDownloadProgress {
  downloadedBytes: number
  totalBytes: number
}

export function updateDownloadPercent(progress: AppUpdateDownloadProgress | null): number {
  if (!progress || progress.totalBytes <= 0) {
    return 0
  }
  return Math.min(100, Math.round((progress.downloadedBytes / progress.totalBytes) * 100))
}

export function formatUpdateBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B'
  }
  const units = ['B', 'KiB', 'MiB', 'GiB']
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  const value = bytes / 1024 ** unitIndex
  return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}
