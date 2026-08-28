import { describe, expect, it } from 'vitest'

import {
  buildRuntimeRows,
  installedPhpSeries,
  isBuiltInPhpSeries,
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

describe('PHP Runtime presentation', () => {
  it('builds only installed Runtime rows in version order', () => {
    const rows = buildRuntimeRows(installed)

    expect(rows.map((row) => row.series)).toEqual(['8.2', '7.4'])
    expect(rows.find((row) => row.series === '8.2')?.runtime?.version).toBe('8.2.33')
    expect(rows.find((row) => row.series === '8.4')).toBeUndefined()
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
})
