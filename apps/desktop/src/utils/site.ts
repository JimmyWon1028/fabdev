import type { Site } from '@fabdev/contracts'

export type SiteListFilter = 'all' | 'shared' | 'home' | 'linked'
export type SiteListSort = 'domainAsc' | 'domainDesc' | 'php' | 'shared'

export interface SiteListOptions {
  query: string
  filter: SiteListFilter
  sort: SiteListSort
  homeSiteIds: Set<string>
  sharedSiteIds: Set<string>
}

export interface SiteRemovalFailure {
  id: string
  message: string
}

export interface SiteRemovalResult {
  removed: string[]
  failed: SiteRemovalFailure[]
}

export async function removeSites(
  siteIds: Iterable<string>,
  remove: (siteId: string) => Promise<unknown>
): Promise<SiteRemovalResult> {
  const result: SiteRemovalResult = { removed: [], failed: [] }

  for (const siteId of new Set(siteIds)) {
    try {
      await remove(siteId)
      result.removed.push(siteId)
    } catch (error) {
      result.failed.push({
        id: siteId,
        message: error instanceof Error ? error.message : String(error)
      })
    }
  }

  return result
}

export function inferDomain(projectPath: string): string {
  const name = projectPath.split(/[\\/]/).filter(Boolean).at(-1) ?? 'site'
  const slug = name
    .normalize('NFKD')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return `${slug || 'site'}.test`
}

export function filterAndSortSites(sites: Site[], options: SiteListOptions): Site[] {
  const query = options.query.trim().toLocaleLowerCase()
  const matchesFilter = (site: Site) => {
    if (options.filter === 'shared') {
      return options.sharedSiteIds.has(site.id)
    }
    if (options.filter === 'home') {
      return options.homeSiteIds.has(site.id)
    }
    if (options.filter === 'linked') {
      return !options.homeSiteIds.has(site.id)
    }
    return true
  }
  const matchesQuery = (site: Site) => {
    if (!query) {
      return true
    }
    return [
      site.domain,
      site.name,
      site.projectPath,
      site.documentRoot,
      site.phpVersion ?? ''
    ]
      .some((value) => value.toLocaleLowerCase().includes(query))
  }
  const compareDomain = (left: Site, right: Site) => left.domain.localeCompare(
    right.domain,
    undefined,
    { numeric: true, sensitivity: 'base' }
  )

  return sites
    .filter((site) => matchesFilter(site) && matchesQuery(site))
    .sort((left, right) => {
      if (options.sort === 'domainDesc') {
        return compareDomain(right, left)
      }
      if (options.sort === 'php') {
        const leftPhp = left.phpVersion ?? '\uffff'
        const rightPhp = right.phpVersion ?? '\uffff'
        return leftPhp.localeCompare(rightPhp, undefined, { numeric: true }) || compareDomain(left, right)
      }
      if (options.sort === 'shared') {
        const shareOrder = Number(options.sharedSiteIds.has(right.id))
          - Number(options.sharedSiteIds.has(left.id))
        return shareOrder || compareDomain(left, right)
      }
      return compareDomain(left, right)
    })
}

export function isDirectChildPath(projectPath: string, parentPath: string): boolean {
  const project = normalizePath(projectPath)
  const parent = normalizePath(parentPath)
  if (!project || !parent || project === parent || !project.startsWith(`${parent}/`)) {
    return false
  }
  return !project.slice(parent.length + 1).includes('/')
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/g, '')
}
