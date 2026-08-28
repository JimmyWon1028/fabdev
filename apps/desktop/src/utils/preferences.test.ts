import { describe, expect, it } from 'vitest'

import {
  loadAutoStartServices,
  loadLanguage,
  saveAutoStartServices,
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
