<script setup lang="ts">
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { RouterLink, RouterView, useRouter } from 'vue-router'

import fabDevIconUrl from '../src-tauri/icons/fabdev-app-icon.svg?url'
import HelpManual from './components/HelpManual.vue'
import { useAppStore } from './stores/fabdev'
import type { AppUpdateDownloadProgress } from './utils/app-update'
import { isHelpShortcut } from './utils/help'
import { useI18n } from './utils/i18n'

const store = useAppStore()
const router = useRouter()
const { t } = useI18n()
const unlisteners: UnlistenFn[] = []
const isQuitting = ref(false)
const showHelp = ref(false)
const availableAppVersion = computed(() =>
  store.appUpdate?.updateAvailable ? store.appUpdate.latestVersion : null
)

function handleGlobalKeydown(event: KeyboardEvent) {
  if (!isHelpShortcut(event)) {
    return
  }
  event.preventDefault()
  showHelp.value = true
}

onMounted(async () => {
  window.addEventListener('keydown', handleGlobalKeydown)
  unlisteners.push(
    await listen('fabdev://service-state-changed', () => store.refreshStatus()),
    await listen<string>('fabdev://agent-error', (event) => store.setError(event.payload)),
    await listen<AppUpdateDownloadProgress>(
      'fabdev://app-update-download-progress',
      (event) => store.setAppUpdateDownloadProgress(event.payload)
    ),
    await listen('fabdev://check-for-updates', () => {
      void router.push('/settings').then(() => store.checkAppUpdate()).catch(() => undefined)
    }),
    await listen('fabdev://quit-started', () => {
      isQuitting.value = true
    }),
    await listen('fabdev://quit-failed', () => {
      isQuitting.value = false
    })
  )
  if (store.autoStartServices) {
    await store.startServicesOnLaunch()
  } else {
    await Promise.all([store.refreshStatus(), store.loadSites()])
  }
  await store.restoreMariaDbOnLaunch()
  void Promise.all([
    store.loadPhpRuntimes(),
    store.loadTerminalPhp(),
    store.loadNodeRuntime(),
    store.loadProxyManager()
  ]).catch((error) => {
    store.setError(error instanceof Error ? error.message : String(error))
  })
  void store.checkAppUpdateOnLaunch()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeydown)
  unlisteners.forEach((unlisten) => unlisten())
})
</script>

<template>
  <div class="app-shell" :aria-busy="isQuitting">
    <aside class="sidebar">
      <div class="brand">
        <img class="brand-mark" :src="fabDevIconUrl" alt="" />
        <div>
          <strong>fabDev</strong>
          <small>{{ t('app.tagline') }}</small>
        </div>
      </div>

      <nav class="navigation" :aria-label="t('nav.label')">
        <RouterLink to="/">{{ t('nav.dashboard') }}</RouterLink>
        <RouterLink to="/sites">{{ t('nav.sites') }}</RouterLink>
        <RouterLink to="/runtimes">{{ t('nav.runtimes') }}</RouterLink>
        <RouterLink to="/mariadb">{{ t('nav.mariadb') }}</RouterLink>
        <RouterLink to="/nodejs">{{ t('nav.nodejs') }}</RouterLink>
        <RouterLink to="/proxy">{{ t('nav.proxy') }}</RouterLink>
        <RouterLink to="/settings">{{ t('nav.settings') }}</RouterLink>
      </nav>

      <div class="agent-state" :class="{ offline: !store.connected }">
        <span class="status-dot" />
        <div>
          <strong>{{ store.connected ? t('agent.connected') : t('agent.disconnected') }}</strong>
          <small>{{ store.status?.agentVersion ?? t('agent.waiting') }}</small>
        </div>
      </div>
    </aside>

    <main class="content">
      <aside
        v-if="availableAppVersion"
        class="app-update-notice"
        role="status"
        aria-live="polite"
      >
        <div>
          <strong>{{ t('app.updateAvailableTitle') }}</strong>
          <span>{{ t('app.updateAvailableDescription', { version: availableAppVersion }) }}</span>
        </div>
        <RouterLink class="secondary-button" to="/settings">
          {{ t('app.viewUpdate') }}
        </RouterLink>
      </aside>
      <RouterView />
    </main>

    <HelpManual v-if="showHelp" @close="showHelp = false" />

    <div v-if="isQuitting" class="quit-overlay" role="status" aria-live="assertive">
      <div class="quit-dialog">
        <span class="quit-spinner" aria-hidden="true" />
        <div>
          <strong>{{ t('app.quittingTitle') }}</strong>
          <p>{{ t('app.quittingDescription') }}</p>
        </div>
      </div>
    </div>
  </div>
</template>
