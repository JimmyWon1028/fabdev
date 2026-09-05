import type { AgentResponse, AgentStatus, ProxyManagerState } from '@fabdev/contracts'
import { invoke } from '@tauri-apps/api/core'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { useAppStore } from './fabdev'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

function status(nginx: AgentStatus['nginx']): AgentResponse {
  return {
    type: 'status',
    payload: {
      protocolVersion: 38, agentVersion: '0.1.22', dns: nginx, nginx,
      phpFpm: nginx, phpFpmPools: [], mariadb: 'notInstalled'
    }
  }
}

function proxies(name: string): AgentResponse {
  const payload: ProxyManagerState = {
    connections: [{
      id: 'demo', name, domain: 'demo.test', listenHost: '127.0.0.1',
      listenPort: 18090, target: 'http://127.0.0.1:18091', allowedOrigins: [],
      upstreamResponseTimeoutSeconds: 60, state: 'running', lastError: null
    }]
  }
  return { type: 'proxyManager', payload }
}

beforeEach(() => {
  vi.resetAllMocks()
  setActivePinia(createPinia())
})

describe('status and Proxy request ordering', () => {
  it('coalesces repeated background status polls', async () => {
    const pending = deferred<AgentResponse>()
    vi.mocked(invoke).mockReturnValue(pending.promise)
    const store = useAppStore()
    const first = store.pollStatus()
    const second = store.pollStatus()
    const calls = vi.mocked(invoke).mock.calls.length
    pending.resolve(status('running'))
    await Promise.all([first, second])
    expect(calls).toBe(1)
    expect(store.status?.nginx).toBe('running')
  })

  it('does not let an old poll overwrite a completed stop', async () => {
    const pending = deferred<AgentResponse>()
    vi.mocked(invoke)
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce({ type: 'stopped' })
      .mockResolvedValueOnce(status('installed'))
    const store = useAppStore()
    const poll = store.pollStatus()
    await store.stopAll()
    pending.resolve(status('running'))
    await poll
    expect(store.status?.nginx).toBe('installed')
  })

  it('ignores old poll errors after a newer successful refresh', async () => {
    const pending = deferred<AgentResponse>()
    vi.mocked(invoke).mockReturnValueOnce(pending.promise).mockResolvedValueOnce(status('running'))
    const store = useAppStore()
    const poll = store.pollStatus()
    await store.refreshStatus()
    pending.reject(new Error('old connection failure'))
    await poll
    expect(store.connected).toBe(true)
    expect(store.error).toBeNull()
  })

  it('keeps busy until all foreground status requests finish', async () => {
    const first = deferred<AgentResponse>()
    const second = deferred<AgentResponse>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const store = useAppStore()
    const firstRefresh = store.refreshStatus()
    const secondRefresh = store.refreshStatus()
    first.resolve(status('installed'))
    await firstRefresh
    const stillBusy = store.busy
    second.resolve(status('running'))
    await secondRefresh
    expect(stillBusy).toBe(true)
    expect(store.busy).toBe(false)
  })

  it('coalesces Proxy polls and preserves the latest mutation result', async () => {
    const pending = deferred<AgentResponse>()
    vi.mocked(invoke).mockReturnValue(pending.promise)
    const store = useAppStore()
    const first = store.loadProxyManager()
    const second = store.loadProxyManager()
    const calls = vi.mocked(invoke).mock.calls.length
    vi.mocked(invoke).mockResolvedValue(proxies('updated'))
    await store.startProxyConnection('demo')
    pending.resolve(proxies('old'))
    await Promise.all([first, second])
    expect(calls).toBe(1)
    expect(store.proxyManager.connections[0].name).toBe('updated')
  })

  it('skips polling during a foreground request and recovers after failure', async () => {
    const pending = deferred<AgentResponse>()
    vi.mocked(invoke).mockReturnValueOnce(pending.promise)
    const store = useAppStore()
    const refresh = store.refreshStatus()
    await store.pollStatus()
    expect(invoke).toHaveBeenCalledTimes(1)
    pending.reject(new Error('connection failed'))
    await refresh
    expect(store.busy).toBe(false)
    expect(store.connected).toBe(false)
    expect(store.error).toBe('connection failed')
    vi.mocked(invoke).mockResolvedValueOnce(status('running'))
    await store.refreshStatus()
    expect(store.connected).toBe(true)
    expect(store.error).toBeNull()
  })

  it('does not let an old Proxy completion release a newer pending poll', async () => {
    const old = deferred<AgentResponse>()
    const current = deferred<AgentResponse>()
    vi.mocked(invoke)
      .mockReturnValueOnce(old.promise)
      .mockResolvedValueOnce(proxies('updated'))
      .mockReturnValueOnce(current.promise)
    const store = useAppStore()
    const oldPoll = store.loadProxyManager()
    await store.startProxyConnection('demo')
    const currentPoll = store.loadProxyManager()
    old.resolve(proxies('old'))
    await oldPoll
    const repeatedPoll = store.loadProxyManager()
    expect(invoke).toHaveBeenCalledTimes(3)
    current.resolve(proxies('current'))
    await Promise.all([currentPoll, repeatedPoll])
    expect(store.proxyManager.connections[0].name).toBe('current')
  })

  it('allows retrying a failed Proxy poll', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('connection failed'))
      .mockResolvedValueOnce(proxies('recovered'))
    const store = useAppStore()
    await expect(store.loadProxyManager()).rejects.toThrow('connection failed')
    await store.loadProxyManager()
    expect(store.proxyManager.connections[0].name).toBe('recovered')
  })

})
