<script setup lang="ts">
import { ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'
import type { Language } from '../utils/preferences'

const store = useAppStore()
const { language, setLanguage, t } = useI18n()
const message = ref('')

function changeLanguage(event: Event) {
  setLanguage((event.target as HTMLSelectElement).value as Language)
  message.value = ''
}

function toggleAutoStartServices() {
  const enabled = !store.autoStartServices
  try {
    store.setAutoStartServices(enabled)
    message.value = enabled ? t('settings.autoStartEnabled') : t('settings.autoStartDisabled')
  } catch (error) {
    message.value = t('settings.saveError', {
      error: error instanceof Error ? error.message : String(error)
    })
  }
}
</script>

<template>
  <header class="page-header">
    <div>
      <p class="eyebrow">{{ t('settings.eyebrow') }}</p>
      <h1>{{ t('settings.title') }}</h1>
      <p>{{ t('settings.description') }}</p>
    </div>
  </header>

  <div class="page-body">
    <section class="settings-list" :aria-label="t('settings.label')">
      <article class="setting-row">
        <div>
          <h2>{{ t('settings.languageTitle') }}</h2>
          <p>{{ t('settings.languageDescription') }}</p>
          <small>{{ t('settings.languageHelp') }}</small>
        </div>
        <select
          class="language-select"
          :value="language"
          :aria-label="t('settings.languageTitle')"
          @change="changeLanguage"
        >
          <option value="en">{{ t('settings.english') }}</option>
          <option value="zh-TW">{{ t('settings.traditionalChinese') }}</option>
          <option value="zh-CN">{{ t('settings.simplifiedChinese') }}</option>
        </select>
      </article>
      <article class="setting-row">
        <div>
          <h2>{{ t('settings.autoStartTitle') }}</h2>
          <p>{{ t('settings.autoStartDescription') }}</p>
          <small>{{ t('settings.autoStartHelp') }}</small>
        </div>
        <button
          class="toggle-button"
          :class="{ active: store.autoStartServices }"
          type="button"
          role="switch"
          :aria-checked="store.autoStartServices"
          :aria-label="t('settings.autoStartTitle')"
          @click="toggleAutoStartServices"
        >
          <span />
        </button>
      </article>
    </section>

    <p v-if="message" class="form-message settings-message" aria-live="polite">
      {{ message }}
    </p>
  </div>
</template>
