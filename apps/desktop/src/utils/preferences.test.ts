import { describe, expect, it } from 'vitest'

import {
  loadAutoCheckUpdates,
  loadAutoStartServices,
  loadLastUpdateCheck,
  loadLanguage,
  loadShowDashboardOnLaunch,
  loadTheme,
  loadThemeMode,
  defaultThemeMode,
  saveAutoCheckUpdates,
  saveAutoStartServices,
  saveLastUpdateCheck,
  saveLanguage,
  saveShowDashboardOnLaunch,
  saveTheme,
  saveThemeMode,
  supportedThemes
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

describe('show-dashboard-on-launch preference', () => {
  it('defaults to disabled when no preference exists', () => {
    expect(loadShowDashboardOnLaunch(memoryStorage())).toBe(false)
  })

  it('persists both preference values', () => {
    const storage = memoryStorage()

    saveShowDashboardOnLaunch(true, storage)
    expect(loadShowDashboardOnLaunch(storage)).toBe(true)

    saveShowDashboardOnLaunch(false, storage)
    expect(loadShowDashboardOnLaunch(storage)).toBe(false)
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

describe('theme preference', () => {
  it('defaults to the existing theme', () => {
    expect(loadTheme(memoryStorage())).toBe('default')
  })

  it('preserves the saved Neubrutalism identifier', () => {
    const storage = memoryStorage()

    saveTheme('neo-brutalism', storage)
    expect(loadTheme(storage)).toBe('neo-brutalism')
  })

  it('restores the old Neubrutalism dark preference as one theme plus a mode', () => {
    const storage = memoryStorage('neubrutalism-dark')
    expect(loadTheme(storage)).toBe('neo-brutalism')
    expect(loadThemeMode('neo-brutalism', storage)).toBe('dark')
  })

  it.each(supportedThemes)('persists both color modes for %s independently', (theme) => {
    const values = new Map<string, string>()
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value) }
    }
    expect(loadThemeMode(theme, storage)).toBe(defaultThemeMode(theme))
    saveThemeMode(theme, 'dark', storage)
    expect(loadThemeMode(theme, storage)).toBe('dark')
    saveThemeMode(theme, 'light', storage)
    expect(loadThemeMode(theme, storage)).toBe('light')
    const other = theme === 'notion' ? 'graphite' : 'notion'
    expect(loadThemeMode(other, storage)).toBe(defaultThemeMode(other))
  })

  it('falls back to the original palette for invalid or unavailable mode storage', () => {
    expect(loadThemeMode('notion', memoryStorage('invalid'))).toBe('light')
    expect(loadThemeMode('graphite', memoryStorage('invalid'))).toBe('dark')
    const storage = { getItem: () => { throw new Error('denied') }, setItem: () => {} }
    expect(loadThemeMode('glassmorphism', storage)).toBe('light')
  })

  it('loads and persists Notion', () => {
    const storage = memoryStorage()

    saveTheme('notion', storage)
    expect(loadTheme(storage)).toBe('notion')
  })

  it('loads and persists Glassmorphism', () => {
    const storage = memoryStorage()

    saveTheme('glassmorphism', storage)
    expect(loadTheme(storage)).toBe('glassmorphism')
  })

  it('preserves the existing Tron preference identifier', () => {
    const storage = memoryStorage()

    saveTheme('cyberpunk', storage)
    expect(loadTheme(storage)).toBe('cyberpunk')
  })

  it('persists Cyberpunk independently from Tron', () => {
    const storage = memoryStorage('cyberpunk')

    expect(loadTheme(storage)).toBe('cyberpunk')
    saveTheme('cyberpunk-city', storage)
    expect(loadTheme(storage)).toBe('cyberpunk-city')
    saveTheme('cyberpunk', storage)
    expect(loadTheme(storage)).toBe('cyberpunk')
  })

  it.each(['graphite', 'retro-terminal', 'porcelain', 'blueprint'] as const)('persists %s across reloads', (theme) => {
    const storage = memoryStorage('cyberpunk-city')
    saveTheme(theme, storage)
    expect(loadTheme(storage)).toBe(theme)
  })

  it('lists Neubrutalism only once and keeps its dark variant out of the menu', () => {
    expect(supportedThemes).toEqual([
      'default', 'notion', 'porcelain', 'neo-brutalism', 'glassmorphism', 'cyberpunk',
      'cyberpunk-city', 'graphite', 'retro-terminal', 'blueprint'
    ])
  })

  it('falls back when the saved theme is unsupported', () => {
    expect(loadTheme(memoryStorage('unknown'))).toBe('default')
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
