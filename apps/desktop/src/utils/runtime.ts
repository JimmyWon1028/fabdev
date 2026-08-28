import type { PhpRuntimeInfo } from '@fabdev/contracts'

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
