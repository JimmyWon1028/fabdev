<script setup lang="ts">
import { getVersion } from '@tauri-apps/api/app'
import { confirm } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { formatUpdateBytes, updateDownloadPercent } from '../utils/app-update'
import { useI18n } from '../utils/i18n'
import type { Language } from '../utils/preferences'

const store = useAppStore()
const { language, setLanguage, t } = useI18n()
const message = ref('')
const updateMessage = ref('')
const currentVersion = ref('—')
const downloadPercent = computed(() => updateDownloadPercent(store.appUpdateDownload))
const downloadedSize = computed(() =>
  formatUpdateBytes(store.appUpdateDownload?.downloadedBytes ?? 0)
)
const totalSize = computed(() =>
  formatUpdateBytes(store.appUpdateDownload?.totalBytes ?? store.appUpdate?.artifact.size ?? 0)
)
const lastUpdateCheckLabel = computed(() =>
  store.lastUpdateCheck
    ? new Date(store.lastUpdateCheck).toLocaleString(language.value)
    : t('settings.notChecked')
)

onMounted(async () => {
  try {
    currentVersion.value = await getVersion()
  } catch {
    // The native update check will still return the compiled App version.
  }
})

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

function toggleAutoCheckUpdates() {
  const enabled = !store.autoCheckUpdates
  try {
    store.setAutoCheckUpdates(enabled)
    message.value = enabled
      ? t('settings.autoCheckUpdatesEnabled')
      : t('settings.autoCheckUpdatesDisabled')
  } catch (error) {
    message.value = t('settings.saveError', {
      error: error instanceof Error ? error.message : String(error)
    })
  }
}

async function checkForUpdates() {
  updateMessage.value = ''
  try {
    const update = await store.checkAppUpdate()
    updateMessage.value = update.updateAvailable
      ? t('settings.updateAvailable', { version: update.latestVersion })
      : t('settings.upToDate', { version: update.currentVersion })
  } catch {
    // The store exposes the detailed update error without affecting App services.
  }
}

async function downloadUpdate() {
  updateMessage.value = ''
  try {
    const download = await store.downloadAppUpdate()
    updateMessage.value = t('settings.downloadVerified', { fileName: download.fileName })
  } catch {
    // The store exposes the detailed update error.
  }
}

async function openReleaseNotes() {
  updateMessage.value = ''
  try {
    await store.openAppReleaseNotes()
  } catch (error) {
    updateMessage.value = error instanceof Error ? error.message : String(error)
  }
}

async function installUpdate() {
  const approved = await confirm(t('settings.installUpdateConfirm'), {
    title: t('settings.installUpdateTitle'),
    kind: 'warning',
    okLabel: t('settings.quitAndOpenInstaller'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }
  updateMessage.value = t('settings.preparingInstaller')
  try {
    await store.installDownloadedAppUpdate()
  } catch {
    // The store exposes the detailed update error.
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
      <article class="setting-row">
        <div>
          <h2>{{ t('settings.autoCheckUpdatesTitle') }}</h2>
          <p>{{ t('settings.autoCheckUpdatesDescription') }}</p>
          <small>{{ t('settings.autoCheckUpdatesHelp') }}</small>
        </div>
        <button
          class="toggle-button"
          :class="{ active: store.autoCheckUpdates }"
          type="button"
          role="switch"
          :aria-checked="store.autoCheckUpdates"
          :aria-label="t('settings.autoCheckUpdatesTitle')"
          @click="toggleAutoCheckUpdates"
        >
          <span />
        </button>
      </article>
      <article class="setting-row update-setting-row">
        <div class="update-setting-content">
          <div class="update-setting-header">
            <div>
              <h2>{{ t('settings.softwareUpdateTitle') }}</h2>
              <p>{{ t('settings.softwareUpdateDescription') }}</p>
            </div>
            <button
              class="secondary-button"
              type="button"
              :disabled="store.appUpdateBusy"
              @click="checkForUpdates"
            >
              {{ store.appUpdateBusy && !store.appUpdateDownload
                ? t('settings.checkingUpdates')
                : t('settings.checkNow') }}
            </button>
          </div>

          <dl class="update-version-grid">
            <div>
              <dt>{{ t('settings.currentVersion') }}</dt>
              <dd>{{ store.appUpdate?.currentVersion ?? currentVersion }}</dd>
            </div>
            <div>
              <dt>{{ t('settings.latestVersion') }}</dt>
              <dd>{{ store.appUpdate?.latestVersion ?? t('settings.notChecked') }}</dd>
            </div>
            <div>
              <dt>{{ t('settings.channel') }}</dt>
              <dd>Stable</dd>
            </div>
            <div>
              <dt>{{ t('settings.lastChecked') }}</dt>
              <dd>{{ lastUpdateCheckLabel }}</dd>
            </div>
          </dl>

          <div v-if="store.appUpdate" class="update-artifact-details">
            <strong>{{ store.appUpdate.artifact.fileName }}</strong>
            <span>{{ formatUpdateBytes(store.appUpdate.artifact.size) }}</span>
            <code>{{ store.appUpdate.artifact.sha256 }}</code>
            <small>{{ t('settings.unsignedCommunityWarning') }}</small>
          </div>

          <div v-if="store.appUpdateDownload" class="update-progress" aria-live="polite">
            <div class="update-progress-label">
              <span>{{ t('settings.downloadingUpdate') }}</span>
              <span>{{ downloadedSize }}／{{ totalSize }} · {{ downloadPercent }}%</span>
            </div>
            <progress :value="downloadPercent" max="100" />
          </div>

          <div class="update-actions">
            <button
              v-if="store.appUpdate"
              class="secondary-button"
              type="button"
              :disabled="store.appUpdateBusy"
              @click="openReleaseNotes"
            >
              {{ t('settings.releaseNotes') }}
            </button>
            <button
              v-if="store.appUpdate?.updateAvailable && !store.downloadedAppUpdate"
              class="primary-button"
              type="button"
              :disabled="store.appUpdateBusy"
              @click="downloadUpdate"
            >
              {{ store.appUpdateBusy
                ? t('settings.downloadingUpdate')
                : t('settings.downloadUpdate') }}
            </button>
            <button
              v-if="store.downloadedAppUpdate"
              class="primary-button"
              type="button"
              :disabled="store.appUpdateBusy"
              @click="installUpdate"
            >
              {{ t('settings.quitAndOpenInstaller') }}
            </button>
          </div>

          <p v-if="updateMessage" class="form-message" aria-live="polite">
            {{ updateMessage }}
          </p>
          <p v-if="store.appUpdateError" class="form-message error-message" aria-live="polite">
            {{ t('settings.updateError', { error: store.appUpdateError }) }}
          </p>
        </div>
      </article>
    </section>

    <p v-if="message" class="form-message settings-message" aria-live="polite">
      {{ message }}
    </p>
  </div>
</template>
