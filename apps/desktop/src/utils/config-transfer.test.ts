import { describe, expect, it } from 'vitest'

import type { ProxyConnectionInfo, Site } from '@fabdev/contracts'

import {
  parseProxyImport,
  parseSitesImport,
  selectNewProxyConnections,
  selectNewSites,
  serializeProxyConnections,
  serializeSites
} from './config-transfer'

const site: Site = {
  id: 'site-1',
  name: 'ERP',
  domain: 'erp.test',
  projectPath: '/Users/dev/Sites/erp',
  documentRoot: '/Users/dev/Sites/erp/public',
  phpVersion: '8.2',
  enabled: true,
  secured: true
}

const connection: ProxyConnectionInfo = {
  id: 'erp-api',
  name: 'ERP-API',
  domain: 'erp-api.test',
  listenHost: '127.0.0.1',
  listenPort: 3020,
  target: 'http://api.example.test',
  allowedOrigins: ['http://erp.test'],
  upstreamResponseTimeoutSeconds: 300,
  state: 'stopped',
  lastError: null
}

describe('Sites configuration transfer', () => {
  it('round-trips portable Site fields without registry IDs', () => {
    const contents = serializeSites([site])
    expect(contents).not.toContain('site-1')
    expect(contents).not.toContain('nodeVersion')
    expect(parseSitesImport(contents)).toEqual([{
      name: 'ERP',
      domain: 'erp.test',
      projectPath: '/Users/dev/Sites/erp',
      documentRoot: '/Users/dev/Sites/erp/public',
      phpVersion: '8.2',
      secured: true
    }])
  })

  it('accepts and ignores the legacy nodeVersion field', () => {
    const legacy = JSON.stringify({
      format: 'fabdev-sites',
      version: 1,
      sites: [{
        name: 'ERP',
        domain: 'erp.test',
        projectPath: '/Users/dev/Sites/erp',
        documentRoot: '/Users/dev/Sites/erp/public',
        phpVersion: '8.2',
        nodeVersion: '24.19.0',
        secured: false
      }]
    })

    expect(parseSitesImport(legacy)[0]).not.toHaveProperty('nodeVersion')
  })

  it('ignores existing and repeated domains', () => {
    const imported = parseSitesImport(serializeSites([
      site,
      { ...site, id: 'site-2', domain: 'new.test' },
      { ...site, id: 'site-3', domain: 'NEW.test.' }
    ]))
    expect(selectNewSites(imported, [site])).toMatchObject({
      skipped: 2,
      items: [{ domain: 'new.test' }]
    })
  })
})

describe('Proxy configuration transfer', () => {
  it('round-trips editable Proxy fields without runtime state', () => {
    const contents = serializeProxyConnections([connection])
    expect(contents).not.toContain('lastError')
    expect(parseProxyImport(contents)).toEqual([{
      id: 'erp-api',
      domain: 'erp-api.test',
      listenPort: 3020,
      target: 'http://api.example.test',
      allowedOrigins: ['http://erp.test'],
      upstreamResponseTimeoutSeconds: 300
    }])
  })

  it('uses 60 seconds for legacy, missing, or zero Proxy timeouts', () => {
    const legacy = JSON.stringify({
      format: 'fabdev-proxy',
      version: 1,
      connections: [{
        id: 'legacy',
        domain: 'legacy.test',
        listenPort: 3021,
        target: 'http://legacy.example.test',
        allowedOrigins: []
      }]
    })
    const zero = JSON.stringify({
      format: 'fabdev-proxy',
      version: 2,
      connections: [{
        id: 'zero',
        domain: 'zero.test',
        listenPort: 3022,
        target: 'http://zero.example.test',
        allowedOrigins: [],
        upstreamResponseTimeoutSeconds: 0
      }]
    })

    expect(parseProxyImport(legacy)[0].upstreamResponseTimeoutSeconds).toBe(60)
    expect(parseProxyImport(zero)[0].upstreamResponseTimeoutSeconds).toBe(60)
  })

  it('rejects Proxy timeouts above 360 seconds', () => {
    const contents = serializeProxyConnections([{
      ...connection,
      upstreamResponseTimeoutSeconds: 361
    }])

    expect(() => parseProxyImport(contents)).toThrow('between 1 and 360')
  })

  it('ignores ID, domain, and port conflicts including conflicts inside the file', () => {
    const imported = [
      { id: ' ERP-API ', domain: 'other.test', listenPort: 3021, target: 'http://one.test', allowedOrigins: [] },
      { id: 'other', domain: 'ERP-API.test.', listenPort: 3022, target: 'http://two.test', allowedOrigins: [] },
      { id: 'third', domain: 'third.test', listenPort: 3020, target: 'http://three.test', allowedOrigins: [] },
      { id: 'new', domain: 'new.test', listenPort: 3023, target: 'http://new.test', allowedOrigins: [] },
      { id: 'duplicate-new', domain: 'duplicate-new.test', listenPort: 3023, target: 'http://four.test', allowedOrigins: [] }
    ]
    expect(selectNewProxyConnections(imported, [connection])).toMatchObject({
      skipped: 4,
      items: [{ id: 'new' }]
    })
  })

  it('rejects malformed documents before importing anything', () => {
    expect(() => parseProxyImport('{"format":"fabdev-sites","version":1,"connections":[]}'))
      .toThrow('Unsupported fabDev Proxy import format')
    expect(() => parseSitesImport('{not-json')).toThrow('not valid JSON')
  })
})
