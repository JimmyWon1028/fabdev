import { describe, expect, it } from 'vitest'

import { formatUpdateBytes, updateDownloadPercent } from './app-update'

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
})
