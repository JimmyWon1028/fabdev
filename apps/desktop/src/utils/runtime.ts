import type { PhpRuntimeInfo, RuntimeUpdateOperationStatus } from '@fabdev/contracts'

export interface RuntimeRow {
  series: string
  runtime: PhpRuntimeInfo
}

export const BUILT_IN_PHP_SERIES = ['7.4', '8.2'] as const

export function isBuiltInPhpSeries(series: string): boolean {
  return BUILT_IN_PHP_SERIES.some((builtInSeries) => builtInSeries === series)
}

export function phpSeriesFromVersion(version: string | null): string | null {
  if (!version) {
    return null
  }
  const parts = version.split('.')
  return parts.length >= 2 ? `${parts[0]}.${parts[1]}` : null
}

export function installedPhpSeries(installed: PhpRuntimeInfo[]): string[] {
  return [...new Set(installed.map((runtime) => runtime.series))]
}

export function buildRuntimeRows(installed: PhpRuntimeInfo[]): RuntimeRow[] {
  return installed.map((runtime) => ({ series: runtime.series, runtime })).sort((left, right) =>
    right.series.localeCompare(left.series, undefined, { numeric: true })
  )
}

export function formatRuntimeBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return '0 B'
  }
  if (bytes < 1024) {
    return `${Math.round(bytes)} B`
  }
  const units = ['KiB', 'MiB', 'GiB']
  let value = bytes / 1024
  let unit = units[0]
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024
    unit = units[index]
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${unit}`
}

export function runtimeProgressPercent(downloaded: number, total: number): number {
  if (!Number.isFinite(downloaded) || !Number.isFinite(total) || total <= 0) {
    return 0
  }
  return Math.min(100, Math.max(0, Math.round((downloaded / total) * 100)))
}

export function isRuntimeDownloadActive(status: RuntimeUpdateOperationStatus): boolean {
  return status === 'queued' || status === 'downloading'
}
