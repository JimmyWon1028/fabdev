import { describe, expect, it } from 'vitest'

import {
  buildRuntimeRows,
  buildWindowsRuntimeRows,
  formatRuntimeBytes,
  installedPhpSeries,
  isBuiltInPhpSeries,
  isRuntimeDownloadActive,
  runtimeProgressPercent,
  phpSeriesFromVersion
} from './runtime'

const installed = [
  {
    version: '8.2.33',
    series: '8.2',
    active: true,
    sites: ['erp.test']
  },
  {
    version: '7.4.33',
    series: '7.4',
    active: false,
    sites: []
  }
]

function onlinePhp(version: string) {
  return {
    name: 'php',
    version,
    platform: 'windows',
    architecture: 'x64',
    minimumOsVersion: '11.0',
    fileName: `php-${version}-windows-x64-community.tar.gz`,
    size: 1024,
    sha256: 'a'.repeat(64),
    unsignedCommunityBuild: true,
    installed: false
  }
}

describe('PHP Runtime presentation', () => {
  it('builds only installed Runtime rows in version order', () => {
    const rows = buildRuntimeRows(installed)

    expect(rows.map((row) => row.series)).toEqual(['8.2', '7.4'])
    expect(rows.find((row) => row.series === '8.2')?.runtime?.version).toBe('8.2.33')
    expect(rows.find((row) => row.series === '8.4')).toBeUndefined()
  })

  it('adds every uninstalled Windows PHP series offered by the Catalog', () => {
    const rows = buildWindowsRuntimeRows(installed, [onlinePhp('8.4.24'), onlinePhp('9.0.1')])

    expect(rows.map((row) => row.version)).toEqual(['9.0.1', '8.4.24', '8.2.33', '7.4.33'])
    expect(rows.find((row) => row.version === '8.4.24')).toMatchObject({
      state: 'not-installed',
      runtime: null,
      series: '8.4'
    })
  })

  it('marks the newest installed patch when a newer Windows patch is available', () => {
    const installedPatches = [
      ...installed,
      { version: '8.2.31', series: '8.2', active: false, sites: [] }
    ]
    const rows = buildWindowsRuntimeRows(installedPatches, [onlinePhp('8.2.34')])

    expect(rows.find((row) => row.version === '8.2.33')).toMatchObject({
      state: 'update-available',
      artifact: { version: '8.2.34' }
    })
    expect(rows.find((row) => row.version === '8.2.31')).toMatchObject({
      state: 'installed',
      artifact: null
    })
  })

  it('does not offer an update when the installed Windows patch is current or newer', () => {
    const rows = buildWindowsRuntimeRows(installed, [
      onlinePhp('8.2.33'),
      onlinePhp('8.2.32'),
      { ...onlinePhp('24.19.0'), name: 'node' }
    ])

    expect(rows).toHaveLength(2)
    expect(rows.find((row) => row.series === '8.2')).toMatchObject({
      state: 'installed',
      artifact: null
    })
  })

  it('derives unique installed series and the global series', () => {
    expect(installedPhpSeries(installed)).toEqual(['8.2', '7.4'])
    expect(phpSeriesFromVersion('8.2.33')).toBe('8.2')
    expect(phpSeriesFromVersion(null)).toBeNull()
  })

  it('marks only PHP 7.4 and 8.2 as built in', () => {
    expect(isBuiltInPhpSeries('7.4')).toBe(true)
    expect(isBuiltInPhpSeries('8.2')).toBe(true)
    expect(isBuiltInPhpSeries('8.4')).toBe(false)
  })

  it('formats package sizes and clamps download progress', () => {
    expect(formatRuntimeBytes(1024)).toBe('1.00 KiB')
    expect(formatRuntimeBytes(12 * 1024 * 1024)).toBe('12.0 MiB')
    expect(runtimeProgressPercent(25, 100)).toBe(25)
    expect(runtimeProgressPercent(120, 100)).toBe(100)
    expect(runtimeProgressPercent(1, 0)).toBe(0)
  })

  it('allows cancellation only while the download is active', () => {
    expect(isRuntimeDownloadActive('queued')).toBe(true)
    expect(isRuntimeDownloadActive('downloading')).toBe(true)
    expect(isRuntimeDownloadActive('verified')).toBe(false)
    expect(isRuntimeDownloadActive('installing')).toBe(false)
  })
})
