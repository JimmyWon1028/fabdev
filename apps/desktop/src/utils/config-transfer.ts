import type {
  ProxyConnectionInfo,
  ProxyConnectionInput,
  Site,
  SiteInput
} from '@fabdev/contracts'

const SITE_TRANSFER_VERSION = 1
const PROXY_TRANSFER_VERSION = 2
const LEGACY_PROXY_TRANSFER_VERSION = 1
const DEFAULT_PROXY_TIMEOUT_SECONDS = 60
const MAX_PROXY_TIMEOUT_SECONDS = 360

interface SiteTransferEntry extends SiteInput {
  name: string
  domain: string
  documentRoot: string
  secured: boolean
}

interface SiteTransferDocument {
  format: 'fabdev-sites'
  version: number
  sites: SiteTransferEntry[]
}

interface ProxyTransferDocument {
  format: 'fabdev-proxy'
  version: number
  connections: ProxyConnectionInput[]
}

export interface ImportSelection<T> {
  items: T[]
  skipped: number
}

export function serializeSites(sites: Site[]): string {
  const document: SiteTransferDocument = {
    format: 'fabdev-sites',
    version: SITE_TRANSFER_VERSION,
    sites: sites.map((site) => ({
      name: site.name,
      domain: site.domain,
      projectPath: site.projectPath,
      documentRoot: site.documentRoot,
      phpVersion: site.phpVersion,
      secured: site.secured
    }))
  }
  return `${JSON.stringify(document, null, 2)}\n`
}

export function parseSitesImport(contents: string): SiteTransferEntry[] {
  const document = parseDocument(contents)
  if (document.format !== 'fabdev-sites' || document.version !== SITE_TRANSFER_VERSION) {
    throw new Error('Unsupported fabDev Sites import format')
  }
  if (!Array.isArray(document.sites)) {
    throw new Error('Sites import must contain a sites array')
  }
  return document.sites.map((entry, index) => parseSiteEntry(entry, index))
}

export function selectNewSites(
  imported: SiteTransferEntry[],
  existing: Site[]
): ImportSelection<SiteTransferEntry> {
  const domains = new Set(existing.map((site) => normalizeDomain(site.domain)))
  const items: SiteTransferEntry[] = []
  let skipped = 0
  for (const site of imported) {
    const domain = normalizeDomain(site.domain)
    if (domains.has(domain)) {
      skipped += 1
      continue
    }
    domains.add(domain)
    items.push(site)
  }
  return { items, skipped }
}

export function serializeProxyConnections(connections: ProxyConnectionInfo[]): string {
  const document: ProxyTransferDocument = {
    format: 'fabdev-proxy',
    version: PROXY_TRANSFER_VERSION,
    connections: connections.map((connection) => ({
      id: connection.id,
      domain: connection.domain,
      listenPort: connection.listenPort,
      target: connection.target,
      allowedOrigins: [...connection.allowedOrigins],
      upstreamResponseTimeoutSeconds: connection.upstreamResponseTimeoutSeconds
    }))
  }
  return `${JSON.stringify(document, null, 2)}\n`
}

export function parseProxyImport(contents: string): ProxyConnectionInput[] {
  const document = parseDocument(contents)
  if (
    document.format !== 'fabdev-proxy'
    || (document.version !== LEGACY_PROXY_TRANSFER_VERSION
      && document.version !== PROXY_TRANSFER_VERSION)
  ) {
    throw new Error('Unsupported fabDev Proxy import format')
  }
  if (!Array.isArray(document.connections)) {
    throw new Error('Proxy import must contain a connections array')
  }
  return document.connections.map((entry, index) => parseProxyEntry(entry, index))
}

export function selectNewProxyConnections(
  imported: ProxyConnectionInput[],
  existing: ProxyConnectionInfo[]
): ImportSelection<ProxyConnectionInput> {
  const ids = new Set(existing.map((connection) => normalizeId(connection.id)))
  const domains = new Set(existing.map((connection) => normalizeDomain(connection.domain)))
  const ports = new Set(existing.map((connection) => connection.listenPort))
  const items: ProxyConnectionInput[] = []
  let skipped = 0
  for (const connection of imported) {
    const id = normalizeId(connection.id)
    const domain = normalizeDomain(connection.domain)
    if (ids.has(id) || domains.has(domain) || ports.has(connection.listenPort)) {
      skipped += 1
      continue
    }
    ids.add(id)
    domains.add(domain)
    ports.add(connection.listenPort)
    items.push(connection)
  }
  return { items, skipped }
}

function parseDocument(contents: string): Record<string, unknown> {
  let value: unknown
  try {
    value = JSON.parse(contents)
  } catch {
    throw new Error('Import file is not valid JSON')
  }
  if (!isRecord(value)) {
    throw new Error('Import file must contain a JSON object')
  }
  return value
}

function parseSiteEntry(value: unknown, index: number): SiteTransferEntry {
  if (!isRecord(value)) {
    throw new Error(`Site import entry ${index + 1} must be an object`)
  }
  return {
    name: requiredString(value.name, `Site import entry ${index + 1} name`),
    domain: requiredString(value.domain, `Site import entry ${index + 1} domain`),
    projectPath: requiredString(
      value.projectPath,
      `Site import entry ${index + 1} projectPath`
    ),
    documentRoot: requiredString(
      value.documentRoot,
      `Site import entry ${index + 1} documentRoot`
    ),
    phpVersion: nullableString(value.phpVersion, `Site import entry ${index + 1} phpVersion`),
    secured: optionalBoolean(value.secured, false, `Site import entry ${index + 1} secured`)
  }
}

function parseProxyEntry(value: unknown, index: number): ProxyConnectionInput {
  if (!isRecord(value)) {
    throw new Error(`Proxy import entry ${index + 1} must be an object`)
  }
  if (!Number.isInteger(value.listenPort)) {
    throw new Error(`Proxy import entry ${index + 1} listenPort must be an integer`)
  }
  if (!Array.isArray(value.allowedOrigins)
    || !value.allowedOrigins.every((origin) => typeof origin === 'string')) {
    throw new Error(`Proxy import entry ${index + 1} allowedOrigins must be a string array`)
  }
  return {
    id: requiredString(value.id, `Proxy import entry ${index + 1} id`),
    domain: requiredString(value.domain, `Proxy import entry ${index + 1} domain`),
    listenPort: value.listenPort as number,
    target: requiredString(value.target, `Proxy import entry ${index + 1} target`),
    allowedOrigins: [...value.allowedOrigins],
    upstreamResponseTimeoutSeconds: proxyTimeoutSeconds(value.upstreamResponseTimeoutSeconds, index)
  }
}

function proxyTimeoutSeconds(value: unknown, index: number): number {
  if (value === undefined || value === null || value === 0) {
    return DEFAULT_PROXY_TIMEOUT_SECONDS
  }
  if (!Number.isInteger(value) || (value as number) < 1 || (value as number) > MAX_PROXY_TIMEOUT_SECONDS) {
    throw new Error(
      `Proxy import entry ${index + 1} upstreamResponseTimeoutSeconds must be 0 or an integer between 1 and ${MAX_PROXY_TIMEOUT_SECONDS}`
    )
  }
  return value as number
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function requiredString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`${label} must be a non-empty string`)
  }
  return value
}

function nullableString(value: unknown, label: string): string | null {
  if (value === null || value === undefined) {
    return null
  }
  if (typeof value !== 'string') {
    throw new Error(`${label} must be a string or null`)
  }
  return value
}

function optionalBoolean(value: unknown, fallback: boolean, label: string): boolean {
  if (value === undefined) {
    return fallback
  }
  if (typeof value !== 'boolean') {
    throw new Error(`${label} must be a boolean`)
  }
  return value
}

function normalizeDomain(domain: string): string {
  return domain.trim().replace(/\.+$/, '').toLocaleLowerCase()
}

function normalizeId(id: string): string {
  return id.trim().toLocaleLowerCase()
}
