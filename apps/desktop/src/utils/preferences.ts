const AUTO_START_SERVICES_KEY = 'fabdev.preferences.autoStartServices'
const AUTO_CHECK_UPDATES_KEY = 'fabdev.preferences.autoCheckUpdates'
const LAST_UPDATE_CHECK_KEY = 'fabdev.preferences.lastUpdateCheck'
const LANGUAGE_KEY = 'fabdev.preferences.language'
const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000

export const supportedLanguages = ['en', 'zh-TW', 'zh-CN'] as const
export type Language = (typeof supportedLanguages)[number]

type PreferenceStorage = Pick<Storage, 'getItem' | 'setItem'>

function browserStorage(): PreferenceStorage | null {
  return typeof window === 'undefined' ? null : window.localStorage
}

export function loadAutoStartServices(storage = browserStorage()): boolean {
  if (!storage) {
    return true
  }
  try {
    return storage.getItem(AUTO_START_SERVICES_KEY) !== 'false'
  } catch {
    return true
  }
}

export function saveAutoStartServices(
  enabled: boolean,
  storage = browserStorage()
): void {
  if (!storage) {
    return
  }
  storage.setItem(AUTO_START_SERVICES_KEY, String(enabled))
}

export function loadAutoCheckUpdates(storage = browserStorage()): boolean {
  if (!storage) {
    return true
  }
  try {
    return storage.getItem(AUTO_CHECK_UPDATES_KEY) !== 'false'
  } catch {
    return true
  }
}

export function saveAutoCheckUpdates(
  enabled: boolean,
  storage = browserStorage()
): void {
  if (!storage) {
    return
  }
  storage.setItem(AUTO_CHECK_UPDATES_KEY, String(enabled))
}

export function loadLastUpdateCheck(storage = browserStorage()): string | null {
  if (!storage) {
    return null
  }
  try {
    const value = storage.getItem(LAST_UPDATE_CHECK_KEY)
    return value && Number.isFinite(Date.parse(value)) ? value : null
  } catch {
    return null
  }
}

export function saveLastUpdateCheck(
  checkedAt: string,
  storage = browserStorage()
): void {
  if (!storage) {
    return
  }
  storage.setItem(LAST_UPDATE_CHECK_KEY, checkedAt)
}

export function shouldAutomaticallyCheckUpdates(
  enabled: boolean,
  lastChecked: string | null,
  now = Date.now()
): boolean {
  if (!enabled || !lastChecked) {
    return enabled
  }
  const checkedAt = Date.parse(lastChecked)
  return !Number.isFinite(checkedAt) || now - checkedAt >= UPDATE_CHECK_INTERVAL_MS
}

export function loadLanguage(storage = browserStorage()): Language {
  if (!storage) {
    return 'zh-TW'
  }
  try {
    const language = storage.getItem(LANGUAGE_KEY)
    return supportedLanguages.includes(language as Language) ? (language as Language) : 'zh-TW'
  } catch {
    return 'zh-TW'
  }
}

export function saveLanguage(language: Language, storage = browserStorage()): void {
  if (!storage) {
    return
  }
  storage.setItem(LANGUAGE_KEY, language)
}
