const AUTO_START_SERVICES_KEY = 'fabdev.preferences.autoStartServices'
const LANGUAGE_KEY = 'fabdev.preferences.language'

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
