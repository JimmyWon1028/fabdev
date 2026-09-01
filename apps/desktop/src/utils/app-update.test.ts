import { describe, expect, it } from 'vitest'

import {
  estimateUpdateDownload,
  formatUpdateBytes,
  formatUpdateDuration,
  isAppUpdateDownloadCancellation,
  updateDownloadPercent
} from './app-update'

describe('app update presentation', () => {
  it('formats installer sizes', () => {
    expect(formatUpdateBytes(0)).toBe('0 B')
    expect(formatUpdateBytes(1024)).toBe('1.0 KiB')
    expect(formatUpdateBytes(99_295_774)).toBe('94.7 MiB')
  })

  it('clamps download progress to a percentage', () => {
    expect(updateDownloadPercent(null)).toBe(0)
    expect(updateDownloadPercent({ downloadedBytes: 50, totalBytes: 100 })).toBe(50)
    expect(updateDownloadPercent({ downloadedBytes: 120, totalBytes: 100 })).toBe(100)
  })

  it('estimates download speed and remaining time without counting resumed bytes', () => {
    const resumed = estimateUpdateDownload(
      null,
      { downloadedBytes: 8 * 1024 * 1024, totalBytes: 32 * 1024 * 1024 },
      1_000
    )
    expect(resumed.bytesPerSecond).toBe(0)
    expect(resumed.remainingSeconds).toBeNull()

    const next = estimateUpdateDownload(
      resumed.sample,
      { downloadedBytes: 16 * 1024 * 1024, totalBytes: 32 * 1024 * 1024 },
      3_000
    )
    expect(next.bytesPerSecond).toBe(4 * 1024 * 1024)
    expect(next.remainingSeconds).toBe(4)
  })

  it('formats compact remaining durations', () => {
    expect(formatUpdateDuration(0)).toBe('0s')
    expect(formatUpdateDuration(12.2)).toBe('13s')
    expect(formatUpdateDuration(125)).toBe('2m 5s')
  })

  it('distinguishes an explicit Windows update cancellation from other failures', () => {
    expect(isAppUpdateDownloadCancellation('Windows update download was cancelled')).toBe(true)
    expect(isAppUpdateDownloadCancellation(new Error('network timeout'))).toBe(false)
  })
})
