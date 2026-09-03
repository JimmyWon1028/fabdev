import { describe, expect, it, vi } from 'vitest'

import type { ProxyConnectionInfo } from '@fabdev/contracts'

import { filterProxyConnections, removeProxyConnections } from './proxy'

const connections: ProxyConnectionInfo[] = [
  {
    id: 'erp-api',
    name: 'ERP API',
    domain: 'erp-api.test',
    listenHost: '127.0.0.1',
    listenPort: 3020,
    target: 'http://api.example.test',
    allowedOrigins: ['http://erp.test:8100'],
    upstreamResponseTimeoutSeconds: 60,
    state: 'running',
    lastError: null
  },
  {
    id: 'warehouse',
    name: 'Warehouse',
    domain: 'warehouse.test',
    listenHost: '127.0.0.1',
    listenPort: 3030,
    target: 'http://192.168.1.20',
    allowedOrigins: [],
    upstreamResponseTimeoutSeconds: 300,
    state: 'stopped',
    lastError: null
  }
]

describe('filterProxyConnections', () => {
  it('searches identity, endpoint, target, and allowed origins', () => {
    expect(filterProxyConnections(connections, 'ERP API').map((item) => item.id))
      .toEqual(['erp-api'])
    expect(filterProxyConnections(connections, '127.0.0.1:3030').map((item) => item.id))
      .toEqual(['warehouse'])
    expect(filterProxyConnections(connections, '192.168.1.20').map((item) => item.id))
      .toEqual(['warehouse'])
    expect(filterProxyConnections(connections, '8100').map((item) => item.id))
      .toEqual(['erp-api'])
  })

  it('returns all connections for a blank query', () => {
    expect(filterProxyConnections(connections, '   ')).toBe(connections)
  })
})

describe('removeProxyConnections', () => {
  it('removes every selected Proxy connection once', async () => {
    const remove = vi.fn().mockResolvedValue(undefined)

    const result = await removeProxyConnections(['erp', 'api', 'erp'], remove)

    expect(remove.mock.calls).toEqual([['erp'], ['api']])
    expect(result).toEqual({ removed: ['erp', 'api'], failed: [] })
  })

  it('continues after one removal fails and reports the failed connection', async () => {
    const remove = vi.fn(async (connectionId: string) => {
      if (connectionId === 'broken') {
        throw new Error('unable to release port')
      }
    })

    const result = await removeProxyConnections(['first', 'broken', 'last'], remove)

    expect(remove.mock.calls).toEqual([['first'], ['broken'], ['last']])
    expect(result).toEqual({
      removed: ['first', 'last'],
      failed: [{ id: 'broken', message: 'unable to release port' }]
    })
  })
})
