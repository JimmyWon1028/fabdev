import type { AgentResponse, RuntimeUpdateArtifact, RuntimeUpdateOperation } from '@fabdev/contracts'
import { invoke } from '@tauri-apps/api/core'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useAppStore } from './fabdev'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

function operation(
  name = 'php',
  status: RuntimeUpdateOperation['status'] = 'queued',
  operationId = name
): RuntimeUpdateOperation {
  return {
    operationId, name, status, version: '8.2.33', platform: 'windows', architecture: 'x64',
    fileName: `${name}.tar.gz`, bytesDownloaded: status === 'verified' ? 100 : 0,
    totalBytes: 100, sha256: 'a'.repeat(64), error: null
  }
}

function artifact(name = 'php'): RuntimeUpdateArtifact {
  const value = operation(name)
  return {
    name, version: value.version, platform: value.platform, architecture: value.architecture,
    fileName: value.fileName, size: value.totalBytes, sha256: value.sha256,
    minimumOsVersion: '10', unsignedCommunityBuild: true, installed: false,
    packageUpdateAvailable: false, activeVersion: null
  }
}

function response(value: RuntimeUpdateOperation): AgentResponse {
  return { type: 'runtimeUpdateOperation', payload: value }
}

const catalog: AgentResponse = {
  type: 'runtimeUpdates',
  payload: {
    catalogSequence: 1, generatedAt: '', expiresAt: '', unsignedCommunityBuild: true,
    artifacts: [artifact('php'), artifact('node'), artifact('mariadb')]
  }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.useFakeTimers()
  setActivePinia(createPinia())
})

afterEach(() => {
  vi.clearAllTimers()
  vi.useRealTimers()
})

describe('Runtime download lifecycle', () => {
  it('continues polling without a page and keeps verified downloads after catalog refresh', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation()))
      .mockResolvedValueOnce(response(operation('php', 'verified')))
      .mockResolvedValueOnce(catalog)
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    await vi.advanceTimersByTimeAsync(250)
    await store.checkRuntimeUpdates()

    expect(store.runtimeOperationFor(artifact())?.status).toBe('verified')
    expect(vi.getTimerCount()).toBe(0)
  })

  it('tracks different Runtime downloads independently across page catalog refreshes', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation('php')))
      .mockResolvedValueOnce(response(operation('node')))
      .mockResolvedValueOnce(catalog)
      .mockResolvedValueOnce(response(operation('php', 'verified')))
      .mockResolvedValueOnce(response(operation('node', 'cancelled')))
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    await store.startRuntimeDownload('node', '8.2.33')
    await store.checkRuntimeUpdates()
    const firstPage = store.waitForRuntimeDownload('php')
    const secondPage = store.waitForRuntimeDownload('php')
    await vi.advanceTimersByTimeAsync(250)

    expect((await firstPage).status).toBe('verified')
    expect((await secondPage).status).toBe('verified')
    expect(store.runtimeOperationFor(artifact('node'))?.status).toBe('cancelled')
    expect(invoke).toHaveBeenCalledTimes(5)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('does not let a delayed progress response overwrite cancellation', async () => {
    let resolveProgress!: (value: AgentResponse) => void
    const pending = new Promise<AgentResponse>((resolve) => { resolveProgress = resolve })
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation()))
      .mockReturnValueOnce(pending)
      .mockResolvedValueOnce(response(operation('php', 'cancelled')))
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    const finished = store.waitForRuntimeDownload('php')
    await vi.advanceTimersByTimeAsync(250)
    await store.cancelRuntimeDownload('php')
    resolveProgress(response(operation('php', 'downloading')))

    expect((await finished).status).toBe('cancelled')
    expect(store.runtimeOperationFor(artifact())?.status).toBe('cancelled')
    expect(vi.getTimerCount()).toBe(0)
  })

  it('can resume polling after a transient Agent error by refreshing the catalog', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation()))
      .mockRejectedValueOnce(new Error('Agent temporarily unavailable'))
      .mockResolvedValueOnce(catalog)
      .mockResolvedValueOnce(response(operation('php', 'verified')))
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    await vi.advanceTimersByTimeAsync(250)
    expect(store.error).toBe('Agent temporarily unavailable')
    expect(store.runtimeOperationFor(artifact())?.status).toBe('queued')
    await store.checkRuntimeUpdates()
    await vi.advanceTimersByTimeAsync(250)

    expect(store.runtimeOperationFor(artifact())?.status).toBe('verified')
    expect(vi.getTimerCount()).toBe(0)
  })

  it('keeps failed downloads available for retry and selects the new operation', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation()))
      .mockResolvedValueOnce(response({ ...operation('php', 'failed'), error: 'checksum mismatch' }))
      .mockResolvedValueOnce(response(operation('php', 'queued', 'retry')))
      .mockResolvedValueOnce(response(operation('php', 'verified', 'retry')))
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    await vi.advanceTimersByTimeAsync(250)
    expect(store.runtimeOperationFor(artifact())?.error).toBe('checksum mismatch')
    await store.startRuntimeDownload('php', '8.2.33')
    await vi.advanceTimersByTimeAsync(250)

    expect(store.runtimeOperationFor(artifact())?.operationId).toBe('retry')
    expect(store.runtimeOperationFor(artifact())?.status).toBe('verified')
  })

  it('retains package identity checks when the catalog replaces a verified package', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(response(operation('php', 'verified')))
    const store = useAppStore()
    await store.startRuntimeDownload('php', '8.2.33')

    expect(store.runtimeOperationFor({ ...artifact(), sha256: 'b'.repeat(64) })).toBeNull()
    expect(store.runtimeOperationFor(artifact())?.status).toBe('verified')
  })

  it('installs the retained verified download after returning to its page', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(response(operation('php', 'verified')))
      .mockResolvedValueOnce(catalog)
      .mockResolvedValueOnce(response(operation('php', 'completed')))
      .mockResolvedValueOnce({ type: 'phpRuntimes', payload: { globalVersion: null, installed: [] } })
    const store = useAppStore()

    await store.startRuntimeDownload('php', '8.2.33')
    await store.checkRuntimeUpdates()
    const retained = store.runtimeOperationFor(artifact())!
    await store.installDownloadedRuntime(retained.operationId)

    expect(store.runtimeOperationFor(artifact())?.status).toBe('completed')
    expect(invoke).toHaveBeenNthCalledWith(3, 'agent_request', {
      request: { type: 'installDownloadedRuntime', payload: { operationId: 'php' } }
    })
    expect(invoke).toHaveBeenCalledTimes(4)
    expect(vi.getTimerCount()).toBe(0)
  })
})
