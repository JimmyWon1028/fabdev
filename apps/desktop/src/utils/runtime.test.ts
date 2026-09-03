import { describe, expect, it } from 'vitest'

import {
  buildCatalogRuntimeRows,
  buildNodeRuntimeRows,
  buildRuntimeRows,
  catalogRuntimeState,
  compareRuntimeVersions,
  formatRuntimeBytes,
  formatRuntimeTarget,
  installedPhpSeries,
  isRuntimeDownloadActive,
  latestRuntimeArtifact,
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
    installed: false,
    packageUpdateAvailable: false,
    activeVersion: null
  }
}

describe('PHP Runtime presentation', () => {
  it('builds only installed Runtime rows in version order', () => {
    const rows = buildRuntimeRows(installed)

    expect(rows.map((row) => row.series)).toEqual(['8.2', '7.4'])
    expect(rows.find((row) => row.series === '8.2')?.runtime?.version).toBe('8.2.33')
    expect(rows.find((row) => row.series === '8.4')).toBeUndefined()
  })

  it('adds every uninstalled PHP series offered by the current platform Catalog', () => {
    const rows = buildCatalogRuntimeRows(installed, [onlinePhp('8.4.24'), onlinePhp('9.0.1')])

    expect(rows.map((row) => row.version)).toEqual(['9.0.1', '8.4.24', '8.2.33', '7.4.33'])
    expect(rows.find((row) => row.version === '8.4.24')).toMatchObject({
      state: 'not-installed',
      runtime: null,
      series: '8.4'
    })
  })

  it('offers online reinstall after bundled PHP 7.4 and 8.2 are removed', () => {
    const rows = buildCatalogRuntimeRows([], [onlinePhp('7.4.33'), onlinePhp('8.2.33')])

    expect(rows.map((row) => [row.version, row.state, row.artifact?.version])).toEqual([
      ['8.2.33', 'not-installed', '8.2.33'],
      ['7.4.33', 'not-installed', '7.4.33']
    ])
  })

  it('marks the newest installed patch when a newer Catalog patch is available', () => {
    const installedPatches = [
      ...installed,
      { version: '8.2.31', series: '8.2', active: false, sites: [] }
    ]
    const rows = buildCatalogRuntimeRows(installedPatches, [onlinePhp('8.2.34')])

    expect(rows.find((row) => row.version === '8.2.33')).toMatchObject({
      state: 'update-available',
      artifact: { version: '8.2.34' }
    })
    expect(rows.find((row) => row.version === '8.2.31')).toMatchObject({
      state: 'installed',
      artifact: null
    })
  })

  it('does not offer an update when the installed patch is current or newer', () => {
    const rows = buildCatalogRuntimeRows(installed, [
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

  it('offers a same-version update when the Catalog package SHA changed', () => {
    const artifact = {
      ...onlinePhp('8.2.33'),
      installed: true,
      packageUpdateAvailable: true
    }
    const rows = buildCatalogRuntimeRows(installed, [artifact])

    expect(rows.find((row) => row.version === '8.2.33')).toMatchObject({
      state: 'update-available',
      artifact: { version: '8.2.33', packageUpdateAvailable: true }
    })
    expect(catalogRuntimeState('8.2.33', artifact)).toBe('update-available')
  })

  it('derives unique installed series and the global series', () => {
    expect(installedPhpSeries(installed)).toEqual(['8.2', '7.4'])
    expect(phpSeriesFromVersion('8.2.33')).toBe('8.2')
    expect(phpSeriesFromVersion(null)).toBeNull()
  })

  it('formats package sizes and clamps download progress', () => {
    expect(formatRuntimeBytes(1024)).toBe('1.00 KiB')
    expect(formatRuntimeBytes(12 * 1024 * 1024)).toBe('12.0 MiB')
    expect(runtimeProgressPercent(25, 100)).toBe(25)
    expect(runtimeProgressPercent(120, 100)).toBe(100)
    expect(runtimeProgressPercent(1, 0)).toBe(0)
  })

  it('formats Runtime targets without Windows-only labels', () => {
    expect(formatRuntimeTarget('macos', 'arm64')).toBe('macOS ARM64')
    expect(formatRuntimeTarget('windows', 'x64')).toBe('Windows x64')
  })

  it('allows cancellation only while the download is active', () => {
    expect(isRuntimeDownloadActive('queued')).toBe(true)
    expect(isRuntimeDownloadActive('downloading')).toBe(true)
    expect(isRuntimeDownloadActive('verified')).toBe(false)
    expect(isRuntimeDownloadActive('installing')).toBe(false)
  })

  it('selects the newest Catalog artifact for Node.js and MariaDB', () => {
    const artifacts = [
      { ...onlinePhp('24.18.0'), name: 'node' },
      { ...onlinePhp('24.19.0'), name: 'node' },
      { ...onlinePhp('12.3.2'), name: 'mariadb' }
    ]

    expect(latestRuntimeArtifact(artifacts, 'node')?.version).toBe('24.19.0')
    expect(latestRuntimeArtifact(artifacts, 'mariadb')?.version).toBe('12.3.2')
    expect(latestRuntimeArtifact(artifacts, 'php')).toBeNull()
  })

  it('shows Node.js 20 and 24 as separate installable rows', () => {
    const rows = buildNodeRuntimeRows([], [
      { ...onlinePhp('20.20.2'), name: 'node' },
      { ...onlinePhp('24.20.0'), name: 'node' }
    ])

    expect(rows.map((row) => [row.version, row.state])).toEqual([
      ['24.20.0', 'not-installed'],
      ['20.20.2', 'not-installed']
    ])
  })

  it('does not invent Node.js rows before Catalog publication', () => {
    const rows = buildNodeRuntimeRows([], [])

    expect(rows).toEqual([])
  })

  it('keeps installed Node.js versions side by side and offers patch updates per major', () => {
    const rows = buildNodeRuntimeRows([
      { version: '20.20.1', active: false },
      { version: '24.20.0', active: true }
    ], [
      { ...onlinePhp('20.20.2'), name: 'node' },
      { ...onlinePhp('24.20.0'), name: 'node' }
    ])

    expect(rows.find((row) => row.major === '20')).toMatchObject({
      state: 'update-available',
      artifact: { version: '20.20.2' }
    })
    expect(rows.find((row) => row.major === '24')).toMatchObject({
      state: 'installed',
      runtime: { active: true }
    })
  })

  it('builds PHP and Node.js rows from macOS ARM64 Catalog artifacts', () => {
    const macosPhp = {
      ...onlinePhp('8.4.24'),
      platform: 'macos',
      architecture: 'arm64',
      minimumOsVersion: '13.0',
      fileName: 'php-8.4.24-macos-arm64-community.tar.gz'
    }
    const macosNode = {
      ...macosPhp,
      name: 'node',
      version: '24.20.0',
      fileName: 'node-24.20.0-macos-arm64-community.tar.gz'
    }

    expect(buildCatalogRuntimeRows(installed, [macosPhp]))
      .toContainEqual(expect.objectContaining({ version: '8.4.24', state: 'not-installed' }))
    expect(buildNodeRuntimeRows([], [macosNode]))
      .toContainEqual(expect.objectContaining({ version: '24.20.0', state: 'not-installed' }))
  })

  it('distinguishes online install, current, and update states', () => {
    const node = { ...onlinePhp('24.19.0'), name: 'node' }

    expect(catalogRuntimeState(null, node)).toBe('not-installed')
    expect(catalogRuntimeState('24.19.0', { ...node, installed: true })).toBe('installed')
    expect(catalogRuntimeState('24.18.0', node)).toBe('update-available')
    expect(catalogRuntimeState('24.20.0', node)).toBe('installed')
    expect(compareRuntimeVersions('24.19.0', '24.18.9')).toBeGreaterThan(0)
  })
})
