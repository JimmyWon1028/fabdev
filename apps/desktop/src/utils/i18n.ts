import { readonly, ref } from 'vue'

import { translations, type TranslationKey } from './locales'
import { loadLanguage, saveLanguage, type Language } from './preferences'

const language = ref<Language>(loadLanguage())

function updateDocumentLanguage(value: Language) {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = value
  }
}

updateDocumentLanguage(language.value)

export function setLanguage(value: Language) {
  saveLanguage(value)
  language.value = value
  updateDocumentLanguage(value)
}

export function translate(
  key: TranslationKey,
  params: Record<string, string | number> = {},
  locale = language.value
): string {
  return Object.entries(params).reduce(
    (message, [name, value]) => message.replaceAll(`{${name}}`, String(value)),
    translations[locale][key]
  )
}

export function translateError(message: string, locale = language.value): string {
  if (message === 'unable to authenticate as MariaDB root; verify the current password') {
    return translate('errors.mariaDbCurrentPassword', {}, locale)
  }
  const mariaDbPasswordMatch = message.match(
    /^MariaDB rejected the root password change \(error ([^)]+)\)$/
  )
  if (mariaDbPasswordMatch) {
    return translate('errors.mariaDbPasswordRejected', { code: mariaDbPasswordMatch[1] }, locale)
  }
  const ingressMatch = message.match(
    /^system ingress is unavailable on DNS port (\d+), HTTP port (\d+), or HTTPS port (\d+)$/
  )
  if (ingressMatch) {
    return translate(
      'errors.systemIngressUnavailable',
      { dnsPort: ingressMatch[1], httpPort: ingressMatch[2], httpsPort: ingressMatch[3] },
      locale
    )
  }
  return message
}

export function useI18n() {
  return {
    language: readonly(language),
    setLanguage,
    t: translate
  }
}
