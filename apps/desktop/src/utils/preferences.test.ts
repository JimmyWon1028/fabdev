import { describe, expect, it } from 'vitest'

import {
  loadAutoCheckUpdates,
  loadAutoStartServices,
  loadLastUpdateCheck,
  loadLanguage,
  saveAutoCheckUpdates,
  saveAutoStartServices,
  saveLastUpdateCheck,
  saveLanguage
} from './preferences'

function memoryStorage(initialValue: string | null = null) {
  let value = initialValue
  return {
    getItem: () => value,
    setItem: (_key: string, nextValue: string) => {
      value = nextValue
    }
  }
}

describe('auto-start service preference', () => {
  it('defaults to enabled when no preference exists', () => {
    expect(loadAutoStartServices(memoryStorage())).toBe(true)
  })

  it('loads a disabled preference', () => {
    expect(loadAutoStartServices(memoryStorage('false'))).toBe(false)
  })

  it('persists both preference values', () => {
    const storage = memoryStorage()

    saveAutoStartServices(false, storage)
    expect(loadAutoStartServices(storage)).toBe(false)

    saveAutoStartServices(true, storage)
    expect(loadAutoStartServices(storage)).toBe(true)
  })
})

describe('language preference', () => {
  it('defaults to Traditional Chinese', () => {
    expect(loadLanguage(memoryStorage())).toBe('zh-TW')
  })

  it('loads and persists a supported language', () => {
    const storage = memoryStorage()

    saveLanguage('en', storage)
    expect(loadLanguage(storage)).toBe('en')
  })

  it('falls back when the saved language is unsupported', () => {
    expect(loadLanguage(memoryStorage('fr'))).toBe('zh-TW')
  })
})

describe('app update preferences', () => {
  it('defaults automatic checks to enabled and persists the switch', () => {
    const storage = memoryStorage()
    expect(loadAutoCheckUpdates(storage)).toBe(true)
    saveAutoCheckUpdates(false, storage)
    expect(loadAutoCheckUpdates(storage)).toBe(false)
  })

  it('persists valid last-check timestamps', () => {
    const storage = memoryStorage()
    expect(loadLastUpdateCheck(storage)).toBeNull()
    saveLastUpdateCheck('2026-08-29T00:00:00.000Z', storage)
    expect(loadLastUpdateCheck(storage)).toBe('2026-08-29T00:00:00.000Z')
  })
})
