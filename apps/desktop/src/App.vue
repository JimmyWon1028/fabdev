<script setup lang="ts">
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { RouterLink, RouterView, useRouter } from 'vue-router'

import fabDevIconUrl from '../src-tauri/icons/fabdev-app-icon.svg?url'
import HelpManual from './components/HelpManual.vue'
import { useAppStore } from './stores/fabdev'
import type { AppUpdateDownloadProgress } from './utils/app-update'
import { isHelpShortcut } from './utils/help'
import { useI18n } from './utils/i18n'
import { dynamicModuleRegistry, resolveDynamicModules } from './utils/navigation'

const store = useAppStore()
const router = useRouter()
const { t } = useI18n()
const unlisteners: UnlistenFn[] = []
const isQuitting = ref(false)
const showHelp = ref(false)
const appVersion = ref('')
const availableAppVersion = computed(() =>
  store.appUpdate?.updateAvailable ? store.appUpdate.latestVersion : null
)
const installedDynamicPackageNames = computed(() => {
  const names = new Set<string>()
  if (store.status?.mariadb && store.status.mariadb !== 'notInstalled') {
    names.add('mariadb')
  }
  if (store.nodeRuntime.installed.length > 0) {
    names.add('node')
  }
  return names
})
const dynamicModules = computed(() => resolveDynamicModules(dynamicModuleRegistry, {
  artifacts: store.runtimeUpdateCheck?.artifacts ?? [],
  installedPackageNames: installedDynamicPackageNames.value
}))

function handleGlobalKeydown(event: KeyboardEvent) {
  if (!isHelpShortcut(event)) {
    return
  }
  event.preventDefault()
  showHelp.value = true
}

function recordFrontendError(source: string, message: string) {
  void invoke('record_desktop_error', { source, message }).catch(() => undefined)
}

function handleWindowError(event: ErrorEvent) {
  const location = event.filename ? ` (${event.filename}:${event.lineno}:${event.colno})` : ''
  recordFrontendError('frontend-error', `${event.message}${location}`)
}

function handleUnhandledRejection(event: PromiseRejectionEvent) {
  const reason = event.reason instanceof Error ? event.reason.stack ?? event.reason.message : String(event.reason)
  recordFrontendError('frontend-rejection', reason)
}

onMounted(async () => {
  void getVersion().then((version) => {
    appVersion.value = version
  }).catch((error) => {
    recordFrontendError('app-version', error instanceof Error ? error.message : String(error))
  })
  window.addEventListener('keydown', handleGlobalKeydown)
  window.addEventListener('error', handleWindowError)
  window.addEventListener('unhandledrejection', handleUnhandledRejection)
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
  void store.checkRuntimeUpdates().catch(() => undefined)
  void store.checkAppUpdateOnLaunch()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKeydown)
  window.removeEventListener('error', handleWindowError)
  window.removeEventListener('unhandledrejection', handleUnhandledRejection)
  unlisteners.forEach((unlisten) => unlisten())
})
</script>

<template>
  <div class="app-shell" :aria-busy="isQuitting">
    <aside class="sidebar">
      <div class="brand">
        <img class="brand-mark" :src="fabDevIconUrl" alt="" />
        <div class="brand-copy">
          <div class="brand-title">
            <strong>fabDev</strong>
            <small v-if="appVersion" class="brand-version">{{ appVersion }}</small>
          </div>
          <small>{{ t('app.tagline') }}</small>
        </div>
      </div>

      <nav class="navigation" :aria-label="t('nav.label')">
        <div class="navigation-section">
          <RouterLink to="/">{{ t('nav.dashboard') }}</RouterLink>
          <RouterLink to="/sites">{{ t('nav.sites') }}</RouterLink>
          <RouterLink to="/runtimes">{{ t('nav.runtimes') }}</RouterLink>
          <RouterLink to="/proxy">{{ t('nav.proxy') }}</RouterLink>
        </div>

        <template v-if="dynamicModules.length > 0">
          <div class="navigation-divider" role="separator" />

          <div class="navigation-section">
            <RouterLink
              v-for="module in dynamicModules"
              :key="module.id"
              :to="module.route"
            >
              {{ t(module.labelKey) }}
            </RouterLink>
          </div>
        </template>
      </nav>

      <div class="sidebar-footer">
        <RouterLink class="settings-link" to="/settings">{{ t('nav.settings') }}</RouterLink>
        <div class="agent-state" role="status" :class="{ offline: !store.connected }">
          <span class="status-dot" />
          <span>{{ store.connected ? t('agent.connected') : t('agent.disconnected') }}</span>
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
