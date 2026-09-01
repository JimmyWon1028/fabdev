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

export interface AppUpdateDownloadRateSample extends AppUpdateDownloadProgress {
  timestampMs: number
  bytesPerSecond: number
}

export interface AppUpdateDownloadEstimate {
  sample: AppUpdateDownloadRateSample
  bytesPerSecond: number
  remainingSeconds: number | null
}

export function isAppUpdateDownloadCancellation(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error)
  return message.toLowerCase().includes('windows update download was cancelled')
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

export function estimateUpdateDownload(
  previous: AppUpdateDownloadRateSample | null,
  progress: AppUpdateDownloadProgress,
  timestampMs: number
): AppUpdateDownloadEstimate {
  const reset =
    !previous ||
    progress.totalBytes !== previous.totalBytes ||
    progress.downloadedBytes < previous.downloadedBytes
  if (reset) {
    const sample = { ...progress, timestampMs, bytesPerSecond: 0 }
    return { sample, bytesPerSecond: 0, remainingSeconds: null }
  }

  const elapsedMs = timestampMs - previous.timestampMs
  const downloadedBytes = progress.downloadedBytes - previous.downloadedBytes
  if (elapsedMs <= 0 || downloadedBytes <= 0) {
    const remainingSeconds = previous.bytesPerSecond > 0
      ? Math.ceil((progress.totalBytes - progress.downloadedBytes) / previous.bytesPerSecond)
      : null
    return {
      sample: previous,
      bytesPerSecond: previous.bytesPerSecond,
      remainingSeconds
    }
  }

  const currentRate = (downloadedBytes * 1000) / elapsedMs
  const bytesPerSecond = previous.bytesPerSecond > 0
    ? previous.bytesPerSecond * 0.6 + currentRate * 0.4
    : currentRate
  const sample = { ...progress, timestampMs, bytesPerSecond }
  return {
    sample,
    bytesPerSecond,
    remainingSeconds: bytesPerSecond > 0
      ? Math.ceil((progress.totalBytes - progress.downloadedBytes) / bytesPerSecond)
      : null
  }
}

export function formatUpdateDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return '0s'
  }
  const roundedSeconds = Math.ceil(seconds)
  if (roundedSeconds < 60) {
    return `${roundedSeconds}s`
  }
  const minutes = Math.floor(roundedSeconds / 60)
  const remainder = roundedSeconds % 60
  return remainder === 0 ? `${minutes}m` : `${minutes}m ${remainder}s`
}
