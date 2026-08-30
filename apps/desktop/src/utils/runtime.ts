import type {
  PhpRuntimeInfo,
  RuntimeUpdateArtifact,
  RuntimeUpdateOperationStatus
} from '@fabdev/contracts'

export interface RuntimeRow {
  series: string
  runtime: PhpRuntimeInfo
}

export type WindowsRuntimeRowState = 'installed' | 'not-installed' | 'update-available'

export interface WindowsRuntimeRow {
  series: string
  version: string
  runtime: PhpRuntimeInfo | null
  artifact: RuntimeUpdateArtifact | null
  state: WindowsRuntimeRowState
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

export function buildWindowsRuntimeRows(
  installed: PhpRuntimeInfo[],
  artifacts: RuntimeUpdateArtifact[]
): WindowsRuntimeRow[] {
  const latestArtifactBySeries = new Map<string, RuntimeUpdateArtifact>()
  for (const artifact of artifacts) {
    if (artifact.name !== 'php') {
      continue
    }
    const series = phpSeriesFromVersion(artifact.version)
    if (!series) {
      continue
    }
    const current = latestArtifactBySeries.get(series)
    if (!current || comparePhpVersions(artifact.version, current.version) > 0) {
      latestArtifactBySeries.set(series, artifact)
    }
  }

  const latestInstalledBySeries = new Map<string, PhpRuntimeInfo>()
  for (const runtime of installed) {
    const current = latestInstalledBySeries.get(runtime.series)
    if (!current || comparePhpVersions(runtime.version, current.version) > 0) {
      latestInstalledBySeries.set(runtime.series, runtime)
    }
  }

  const rows = installed.map<WindowsRuntimeRow>((runtime) => {
    const latestInstalled = latestInstalledBySeries.get(runtime.series)
    const artifact = latestArtifactBySeries.get(runtime.series)
    const updateArtifact = latestInstalled?.version === runtime.version
      && artifact
      && comparePhpVersions(artifact.version, runtime.version) > 0
      ? artifact
      : null
    return {
      series: runtime.series,
      version: runtime.version,
      runtime,
      artifact: updateArtifact,
      state: updateArtifact ? 'update-available' : 'installed'
    }
  })

  for (const [series, artifact] of latestArtifactBySeries) {
    if (latestInstalledBySeries.has(series)) {
      continue
    }
    rows.push({
      series,
      version: artifact.version,
      runtime: null,
      artifact,
      state: 'not-installed'
    })
  }

  return rows.sort((left, right) => {
    const seriesOrder = comparePhpVersions(`${right.series}.0`, `${left.series}.0`)
    return seriesOrder || comparePhpVersions(right.version, left.version)
  })
}

function comparePhpVersions(left: string, right: string): number {
  const leftParts = left.split('.').map((part) => Number.parseInt(part, 10))
  const rightParts = right.split('.').map((part) => Number.parseInt(part, 10))
  const length = Math.max(leftParts.length, rightParts.length)
  for (let index = 0; index < length; index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0)
    if (difference !== 0) {
      return difference
    }
  }
  return 0
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
