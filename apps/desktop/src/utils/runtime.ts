import {
  supportedNodeVersions,
  type NodeRuntimeInfo,
  type PhpRuntimeInfo,
  type RuntimeUpdateArtifact,
  type RuntimeUpdateOperationStatus
} from '@fabdev/contracts'

export interface RuntimeRow {
  series: string
  runtime: PhpRuntimeInfo
}

export type WindowsRuntimeRowState = 'installed' | 'not-installed' | 'update-available'
export type CatalogRuntimeState = 'installed' | 'not-installed' | 'update-available'

export interface WindowsRuntimeRow {
  series: string
  version: string
  runtime: PhpRuntimeInfo | null
  artifact: RuntimeUpdateArtifact | null
  state: WindowsRuntimeRowState
}

export interface WindowsNodeRuntimeRow {
  major: string
  version: string
  runtime: NodeRuntimeInfo | null
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
    if (!current || compareRuntimeVersions(artifact.version, current.version) > 0) {
      latestArtifactBySeries.set(series, artifact)
    }
  }

  const latestInstalledBySeries = new Map<string, PhpRuntimeInfo>()
  for (const runtime of installed) {
    const current = latestInstalledBySeries.get(runtime.series)
    if (!current || compareRuntimeVersions(runtime.version, current.version) > 0) {
      latestInstalledBySeries.set(runtime.series, runtime)
    }
  }

  const rows = installed.map<WindowsRuntimeRow>((runtime) => {
    const latestInstalled = latestInstalledBySeries.get(runtime.series)
    const artifact = latestArtifactBySeries.get(runtime.series)
    const updateArtifact = latestInstalled?.version === runtime.version
      && artifact
      && compareRuntimeVersions(artifact.version, runtime.version) > 0
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
    const seriesOrder = compareRuntimeVersions(`${right.series}.0`, `${left.series}.0`)
    return seriesOrder || compareRuntimeVersions(right.version, left.version)
  })
}

export function buildWindowsNodeRuntimeRows(
  installed: NodeRuntimeInfo[],
  artifacts: RuntimeUpdateArtifact[]
): WindowsNodeRuntimeRow[] {
  const supportedByMajor = new Map(
    supportedNodeVersions.map((version) => [version.split('.')[0], version])
  )
  const nodeArtifacts = artifacts.filter((artifact) => {
    const minimum = supportedByMajor.get(artifact.version.split('.')[0])
    return artifact.name === 'node'
      && minimum !== undefined
      && compareRuntimeVersions(artifact.version, minimum) >= 0
  })
  const latestArtifactByMajor = new Map<string, RuntimeUpdateArtifact>()
  for (const artifact of nodeArtifacts) {
    const major = artifact.version.split('.')[0]
    const current = latestArtifactByMajor.get(major)
    if (!current || compareRuntimeVersions(artifact.version, current.version) > 0) {
      latestArtifactByMajor.set(major, artifact)
    }
  }

  const latestInstalledByMajor = new Map<string, NodeRuntimeInfo>()
  for (const runtime of installed) {
    const major = runtime.version.split('.')[0]
    const current = latestInstalledByMajor.get(major)
    if (!current || compareRuntimeVersions(runtime.version, current.version) > 0) {
      latestInstalledByMajor.set(major, runtime)
    }
  }

  const rows = installed.map<WindowsNodeRuntimeRow>((runtime) => {
    const major = runtime.version.split('.')[0]
    const artifact = latestArtifactByMajor.get(major)
    const latestInstalled = latestInstalledByMajor.get(major)
    const updateArtifact = latestInstalled?.version === runtime.version
      && artifact
      && compareRuntimeVersions(artifact.version, runtime.version) > 0
      ? artifact
      : null
    return {
      major,
      version: runtime.version,
      runtime,
      artifact: updateArtifact,
      state: updateArtifact ? 'update-available' : 'installed'
    }
  })

  for (const [major, artifact] of latestArtifactByMajor) {
    if (latestInstalledByMajor.has(major)) {
      continue
    }
    rows.push({
      major,
      version: artifact.version,
      runtime: null,
      artifact,
      state: 'not-installed'
    })
  }

  for (const version of supportedNodeVersions) {
    const major = version.split('.')[0]
    if (latestInstalledByMajor.has(major) || latestArtifactByMajor.has(major)) {
      continue
    }
    rows.push({
      major,
      version,
      runtime: null,
      artifact: null,
      state: 'not-installed'
    })
  }

  return rows.sort((left, right) => compareRuntimeVersions(right.version, left.version))
}

export function latestRuntimeArtifact(
  artifacts: RuntimeUpdateArtifact[],
  name: string
): RuntimeUpdateArtifact | null {
  return artifacts
    .filter((artifact) => artifact.name === name)
    .sort((left, right) => compareRuntimeVersions(right.version, left.version))[0] ?? null
}

export function catalogRuntimeState(
  installedVersion: string | null,
  artifact: RuntimeUpdateArtifact | null
): CatalogRuntimeState {
  if (!installedVersion) {
    return 'not-installed'
  }
  if (!artifact || compareRuntimeVersions(artifact.version, installedVersion) <= 0) {
    return 'installed'
  }
  return 'update-available'
}

export function compareRuntimeVersions(left: string, right: string): number {
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
