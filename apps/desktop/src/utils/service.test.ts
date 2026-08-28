import type { AgentStatus } from '@fabdev/contracts'
import { describe, expect, it } from 'vitest'

import {
  areAllServicesRunning,
  canToggleAllServices,
  hasEnabledSites,
  shouldStopServicesBeforeStart,
  summarizeProxyConnections
} from './service'

function status(states: Partial<AgentStatus> = {}): AgentStatus {
  return {
    protocolVersion: 4,
    agentVersion: '0.1.0',
    dns: 'installed',
    nginx: 'installed',
    phpFpm: 'installed',
    phpFpmPools: [],
    mariadb: 'notInstalled',
    ...states
  }
}

describe('service startup decisions', () => {
  it('keeps an already running environment intact', () => {
    const running = status({ dns: 'running', nginx: 'running', phpFpm: 'running' })

    expect(areAllServicesRunning(running)).toBe(true)
    expect(shouldStopServicesBeforeStart(running)).toBe(true)
  })

  it('starts an installed environment without stopping it first', () => {
    const installed = status()

    expect(areAllServicesRunning(installed)).toBe(false)
    expect(shouldStopServicesBeforeStart(installed)).toBe(false)
  })

  it('cleans up a partial environment before restarting', () => {
    const partial = status({ dns: 'running', nginx: 'failed' })

    expect(areAllServicesRunning(partial)).toBe(false)
    expect(shouldStopServicesBeforeStart(partial)).toBe(true)
  })

  it('does not start services until at least one Site is enabled', () => {
    expect(hasEnabledSites([])).toBe(false)
    expect(hasEnabledSites([{ enabled: false }])).toBe(false)
    expect(hasEnabledSites([{ enabled: false }, { enabled: true }])).toBe(true)
  })

  it('keeps the service toggle available without requiring a prior Agent connection', () => {
    expect(canToggleAllServices(false, false, true)).toBe(true)
    expect(canToggleAllServices(false, true, false)).toBe(true)
    expect(canToggleAllServices(false, false, false)).toBe(false)
    expect(canToggleAllServices(true, true, true)).toBe(false)
  })
})

describe('Proxy connection summary', () => {
  it('reports an empty Proxy manager as stopped', () => {
    expect(summarizeProxyConnections([])).toEqual({
      total: 0,
      running: 0,
      issues: 0,
      state: 'stopped'
    })
  })

  it('counts running connections without treating intentionally stopped connections as errors', () => {
    expect(summarizeProxyConnections([
      { state: 'running' },
      { state: 'stopped' }
    ])).toEqual({
      total: 2,
      running: 1,
      issues: 0,
      state: 'running'
    })
  })

  it('prioritizes failed and degraded connections in the overview state', () => {
    expect(summarizeProxyConnections([
      { state: 'running' },
      { state: 'degraded' }
    ]).state).toBe('degraded')
    expect(summarizeProxyConnections([
      { state: 'degraded' },
      { state: 'failed' }
    ])).toEqual({
      total: 2,
      running: 0,
      issues: 2,
      state: 'failed'
    })
  })
})
