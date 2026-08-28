import { describe, expect, it } from 'vitest'

import type { Site } from '@fabdev/contracts'

import { filterAndSortSites, inferDomain, isDirectChildPath, removeSites } from './site'

const sites: Site[] = [
  {
    id: 'site-one',
    name: 'Site One',
    domain: 'site-one.test',
    projectPath: '/Users/dev/Sites/site-one',
    documentRoot: '/Users/dev/Sites/site-one/public',
    phpVersion: '8.2',
    enabled: true,
    secured: false
  },
  {
    id: 'demo',
    name: 'Demo',
    domain: 'demo.test',
    projectPath: '/Users/dev/Sites/demo',
    documentRoot: '/Users/dev/Sites/demo/public',
    phpVersion: '7.4',
    enabled: true,
    secured: false
  },
  {
    id: 'adminer',
    name: 'Adminer',
    domain: 'adminer.test',
    projectPath: '/Users/dev/tools/adminer',
    documentRoot: '/Users/dev/tools/adminer',
    phpVersion: null,
    enabled: true,
    secured: false
  }
]

describe('inferDomain', () => {
  it('creates a test domain from the selected directory', () => {
    expect(inferDomain('/Users/dev/Sites/ERP Demo')).toBe('erp-demo.test')
  })

  it('uses a safe fallback for non-latin names', () => {
    expect(inferDomain('/Users/dev/Sites/測試')).toBe('site.test')
  })
})

describe('isDirectChildPath', () => {
  it('identifies a first-level project in the Site Home directory', () => {
    expect(isDirectChildPath('/Users/dev/Sites/site1', '/Users/dev/Sites')).toBe(true)
    expect(isDirectChildPath('/Users/dev/Sites/site1/', '/Users/dev/Sites/')).toBe(true)
  })

  it('does not identify the Home directory or nested projects', () => {
    expect(isDirectChildPath('/Users/dev/Sites', '/Users/dev/Sites')).toBe(false)
    expect(isDirectChildPath('/Users/dev/Sites/group/site1', '/Users/dev/Sites')).toBe(false)
    expect(isDirectChildPath('/Users/dev/Sites-old/site1', '/Users/dev/Sites')).toBe(false)
  })

  it('supports Windows-style paths', () => {
    expect(isDirectChildPath('C:\\Users\\dev\\Sites\\site1', 'C:\\Users\\dev\\Sites')).toBe(true)
  })
})

describe('filterAndSortSites', () => {
  const baseOptions = {
    query: '',
    filter: 'all' as const,
    sort: 'domainAsc' as const,
    homeSiteIds: new Set(['demo']),
    sharedSiteIds: new Set(['site-one'])
  }

  it('searches domains, project paths, document roots, and PHP versions', () => {
    expect(filterAndSortSites(sites, { ...baseOptions, query: 'Site One' }).map((site) => site.id))
      .toEqual(['site-one'])
    expect(filterAndSortSites(sites, { ...baseOptions, query: 'tools' }).map((site) => site.id))
      .toEqual(['adminer'])
    expect(filterAndSortSites(sites, { ...baseOptions, query: '7.4' }).map((site) => site.id))
      .toEqual(['demo'])
  })

  it('filters shared, Site Home, and linked Sites', () => {
    expect(filterAndSortSites(sites, { ...baseOptions, filter: 'shared' }).map((site) => site.id))
      .toEqual(['site-one'])
    expect(filterAndSortSites(sites, { ...baseOptions, filter: 'home' }).map((site) => site.id))
      .toEqual(['demo'])
    expect(filterAndSortSites(sites, { ...baseOptions, filter: 'linked' }).map((site) => site.id))
      .toEqual(['adminer', 'site-one'])
  })

  it('sorts by domain, PHP version, and shared status without mutating the source', () => {
    expect(filterAndSortSites(sites, baseOptions).map((site) => site.id))
      .toEqual(['adminer', 'demo', 'site-one'])
    expect(filterAndSortSites(sites, { ...baseOptions, sort: 'domainDesc' }).map((site) => site.id))
      .toEqual(['site-one', 'demo', 'adminer'])
    expect(filterAndSortSites(sites, { ...baseOptions, sort: 'php' }).map((site) => site.id))
      .toEqual(['demo', 'site-one', 'adminer'])
    expect(filterAndSortSites(sites, { ...baseOptions, sort: 'shared' }).map((site) => site.id))
      .toEqual(['site-one', 'adminer', 'demo'])
    expect(sites.map((site) => site.id)).toEqual(['site-one', 'demo', 'adminer'])
  })
})

describe('removeSites', () => {
  it('removes every selected Site once', async () => {
    const removed: string[] = []

    const result = await removeSites(['site-one', 'adminer', 'site-one'], async (siteId) => {
      removed.push(siteId)
    })

    expect(removed).toEqual(['site-one', 'adminer'])
    expect(result).toEqual({ removed: ['site-one', 'adminer'], failed: [] })
  })

  it('continues after one Site removal fails', async () => {
    const result = await removeSites(['site-one', 'broken', 'adminer'], async (siteId) => {
      if (siteId === 'broken') {
        throw new Error('unable to remove Site')
      }
    })

    expect(result).toEqual({
      removed: ['site-one', 'adminer'],
      failed: [{ id: 'broken', message: 'unable to remove Site' }]
    })
  })
})
