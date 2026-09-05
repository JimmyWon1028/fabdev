<script setup lang="ts">
import type {
  PhpRuntimeInfo,
  RuntimeUpdateArtifact,
  RuntimeUpdateOperation
} from '@fabdev/contracts'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'
import { formatPathForDisplay, isWindowsPlatform } from '../utils/path'
import {
  buildCatalogRuntimeRows,
  formatRuntimeBytes,
  formatRuntimeTarget,
  isRuntimeDownloadActive,
  runtimeOperationForArtifact,
  runtimeProgressPercent
} from '../utils/runtime'

const store = useAppStore()
const { t } = useI18n()
const action = ref<string | null>(null)
const message = ref('')
const phpIniSeries = ref<string | null>(null)
const phpIniContents = ref('')
let mounted = true

const isWindows = isWindowsPlatform()
const onlineArtifacts = computed(() =>
  store.runtimeUpdateCheck?.artifacts.filter((artifact) => artifact.name === 'php') ?? []
)
const catalogRows = computed(() =>
  buildCatalogRuntimeRows(store.phpRuntimes.installed, onlineArtifacts.value)
)
const onlineOperation = computed(() => store.runtimeUpdateOperation)
const currentRuntimeTarget = computed(() =>
  formatRuntimeTarget(isWindows ? 'windows' : 'macos', isWindows ? 'x64' : 'arm64')
)
const showInFileManagerLabel = computed(() =>
  t(isWindows ? 'runtimes.showInExplorer' : 'runtimes.showInFinder')
)

onMounted(() => {
  void refresh()
})

onBeforeUnmount(() => {
  mounted = false
})

async function refresh() {
  action.value = 'refresh'
  message.value = ''
  try {
    await Promise.all([store.loadPhpRuntimes(), store.loadTerminalPhp()])
    await store.checkRuntimeUpdates()
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function installPackage() {
  const releasePath = await open({
    directory: false,
    multiple: false,
    title: t('runtimes.chooseRelease'),
    filters: [{ name: t('runtimes.releaseFilter'), extensions: ['json'] }]
  })
  if (typeof releasePath !== 'string') {
    return
  }
  const artifactPath = await open({
    directory: false,
    multiple: false,
    title: t('runtimes.chooseArtifact'),
    filters: [{ name: t('runtimes.packageFilter'), extensions: ['gz'] }]
  })
  if (typeof artifactPath !== 'string') {
    return
  }

  action.value = 'install'
  message.value = t('runtimes.installing')
  try {
    const state = await store.installPhpRuntime(artifactPath, releasePath)
    message.value = t('runtimes.installedCount', { count: state.installed.length })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function checkOnlineRuntimes() {
  action.value = 'online-check'
  message.value = ''
  try {
    await store.checkRuntimeUpdates()
    message.value = onlineArtifacts.value.length > 0
      ? t('runtimes.onlineAvailableCount', { count: onlineArtifacts.value.length })
      : t('runtimes.onlineNone')
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function downloadOnlineRuntime(
  artifact: RuntimeUpdateArtifact | null
): Promise<RuntimeUpdateOperation | null> {
  if (!artifact) {
    return null
  }
  const approved = await confirm(
    t('runtimes.onlineDownloadConfirm', {
      version: artifact.version,
      size: formatRuntimeBytes(artifact.size),
      sha256: artifact.sha256
    }),
    {
      title: t('runtimes.onlineDownloadTitle'),
      kind: 'warning',
      okLabel: t('runtimes.onlineDownload'),
      cancelLabel: t('runtimes.cancel')
    }
  )
  if (!approved) {
    return null
  }

  action.value = `online-download:${artifact.version}`
  message.value = t('runtimes.onlineDownloading')
  let completedOperation: RuntimeUpdateOperation | null = null
  try {
    let operation = await store.startRuntimeDownload(artifact.name, artifact.version)
    operation = await pollRuntimeDownload(operation)
    completedOperation = operation
    if (operation.status === 'verified') {
      message.value = t('runtimes.onlineVerified', { version: operation.version })
    } else if (operation.status === 'cancelled') {
      message.value = t('runtimes.onlineCancelled')
    } else if (operation.status === 'failed') {
      throw new Error(operation.error ?? t('runtimes.onlineDownloadFailed'))
    }
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
  return completedOperation
}

async function pollRuntimeDownload(
  initial: RuntimeUpdateOperation
): Promise<RuntimeUpdateOperation> {
  let operation = initial
  while (mounted && isRuntimeDownloadActive(operation.status)) {
    await new Promise((resolve) => window.setTimeout(resolve, 250))
    operation = await store.getRuntimeUpdateOperation(operation.operationId)
  }
  return operation
}

async function cancelOnlineDownload() {
  const operation = onlineOperation.value
  if (!operation || !isRuntimeDownloadActive(operation.status)) {
    return
  }
  try {
    await store.cancelRuntimeDownload(operation.operationId)
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  }
}

async function installOnlineRuntime(
  artifact: RuntimeUpdateArtifact | null,
  operation: RuntimeUpdateOperation | null
): Promise<boolean> {
  if (!artifact || !operation || operation.status !== 'verified') {
    return false
  }
  const approved = await confirm(
    t('runtimes.onlineInstallConfirm', {
      version: operation.version,
      size: formatRuntimeBytes(operation.totalBytes),
      sha256: operation.sha256
    }),
    {
      title: t('runtimes.onlineInstallTitle'),
      kind: 'warning',
      okLabel: t('runtimes.onlineInstall'),
      cancelLabel: t('runtimes.cancel')
    }
  )
  if (!approved) {
    return false
  }

  action.value = `online-install:${artifact.version}`
  message.value = t('runtimes.onlineInstalling')
  try {
    const installed = await store.installDownloadedRuntime(operation.operationId)
    if (installed.status === 'completed') {
      message.value = t('runtimes.onlineInstalled', { version: installed.version })
      return true
    } else {
      throw new Error(installed.error ?? t('runtimes.onlineInstallFailed'))
    }
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
    return false
  } finally {
    action.value = null
  }
}

function operationForArtifact(artifact: RuntimeUpdateArtifact | null): RuntimeUpdateOperation | null {
  const operation = onlineOperation.value
  return runtimeOperationForArtifact(artifact, operation)
}

function operationProgress(artifact: RuntimeUpdateArtifact | null): number {
  const operation = operationForArtifact(artifact)
  return operation ? runtimeProgressPercent(operation.bytesDownloaded, operation.totalBytes) : 0
}

function operationStatusLabel(artifact: RuntimeUpdateArtifact | null): string {
  const operation = operationForArtifact(artifact)
  return operation ? t(`runtimes.onlineStatus.${operation.status}`) : ''
}

async function installOrUpdateOnlineRuntime(artifact: RuntimeUpdateArtifact) {
  let operation = operationForArtifact(artifact)
  if (operation?.status !== 'verified') {
    operation = await downloadOnlineRuntime(artifact)
  }
  if (operation?.status === 'verified') {
    await installOnlineRuntime(artifact, operation)
  }
}

async function setGlobal(runtime: PhpRuntimeInfo) {
  action.value = `global:${runtime.version}`
  message.value = ''
  try {
    await store.setGlobalPhp(runtime.version)
    message.value = t('runtimes.globalChanged', { version: runtime.version })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function enableTerminalPhp() {
  action.value = 'terminal-php:enable'
  message.value = ''
  try {
    const state = await store.enableTerminalPhp()
    message.value = t('runtimes.terminalEnabled', {
      path: formatPathForDisplay(state.binPath, isWindows)
    })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function disableTerminalPhp() {
  action.value = 'terminal-php:disable'
  message.value = ''
  try {
    await store.disableTerminalPhp()
    message.value = t('runtimes.terminalDisabled')
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function removeRuntime(runtime: PhpRuntimeInfo) {
  const approved = await confirm(
    t('runtimes.removeConfirm', { version: runtime.version }),
    {
      title: t('runtimes.removeTitle'),
      kind: 'warning',
      okLabel: t('runtimes.removeRuntime'),
      cancelLabel: t('runtimes.cancel')
    }
  )
  if (!approved) {
    return
  }

  action.value = `remove:${runtime.version}`
  message.value = ''
  try {
    await store.removePhpRuntime(runtime.version)
    message.value = t('runtimes.removed', { version: runtime.version })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function editPhpIni(runtime: PhpRuntimeInfo) {
  action.value = `ini:${runtime.series}`
  message.value = ''
  try {
    phpIniContents.value = await store.getPhpIni(runtime.series)
    phpIniSeries.value = runtime.series
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function savePhpIni() {
  if (!phpIniSeries.value) {
    return
  }
  action.value = `save-ini:${phpIniSeries.value}`
  message.value = ''
  try {
    await store.savePhpIni(phpIniSeries.value, phpIniContents.value)
    message.value = t('runtimes.iniSaved', { version: phpIniSeries.value })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function loadErpPhpIni() {
  if (!phpIniSeries.value) {
    return
  }
  action.value = `erp-ini:${phpIniSeries.value}`
  message.value = ''
  try {
    phpIniContents.value = await store.getErpPhpIni(phpIniSeries.value)
    message.value = t('runtimes.erpConfigLoaded')
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function revealPhpIni() {
  if (!phpIniSeries.value) {
    return
  }
  message.value = ''
  try {
    const path = await store.revealPhpIni(phpIniSeries.value)
    message.value = t(isWindows ? 'runtimes.revealedInExplorer' : 'runtimes.revealed', {
      path: formatPathForDisplay(path, isWindows)
    })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  }
}
</script>

<template>
  <header class="page-header">
    <div>
      <p class="eyebrow">{{ t('runtimes.eyebrow') }}</p>
      <h1>{{ t('nav.runtimes') }}</h1>
      <p>{{ t('runtimes.description') }}</p>
    </div>
    <div class="header-actions">
      <button class="primary-button runtime-install-button" :disabled="action !== null" @click="installPackage">
        {{ t('runtimes.installLocal') }}
      </button>
      <button class="secondary-button" :disabled="action !== null" @click="refresh">
        {{ t('common.refresh') }}
      </button>
    </div>
  </header>

  <div class="page-body">
    <div class="runtime-summary">
      <span>{{ t('runtimes.globalPhp') }}</span>
      <strong>{{ store.phpRuntimes.globalVersion ?? t('runtimes.notSet') }}</strong>
      <small>{{ t('runtimes.existingSitesIndependent') }}</small>
    </div>

    <section class="runtime-online-card" :aria-label="t('runtimes.onlineTitle')">
      <div class="runtime-online-header">
        <div>
          <p class="eyebrow">{{ t('runtimes.onlineEyebrow') }}</p>
          <h2>{{ t('runtimes.onlineTitle') }}</h2>
          <p>{{ t('runtimes.onlineDescription') }}</p>
        </div>
        <button
          class="secondary-button"
          :disabled="action !== null"
          @click="checkOnlineRuntimes"
        >
          {{ action === 'online-check' ? t('runtimes.onlineChecking') : t('runtimes.onlineCheck') }}
        </button>
      </div>

    </section>

    <div v-if="message" class="notice">
      <span>{{ message }}</span>
    </div>

    <section class="runtime-list" :aria-label="t('runtimes.listLabel')">
      <article
        v-for="row in catalogRows"
        :key="`runtime:${row.version}`"
        class="runtime-card"
      >
        <div class="runtime-details">
          <span class="runtime-version">{{ row.series }}</span>
          <div>
            <h2>PHP {{ row.version }}</h2>
            <p>
              {{ row.artifact
                ? formatRuntimeTarget(row.artifact.platform, row.artifact.architecture)
                : currentRuntimeTarget }}
              <template v-if="row.state === 'update-available' && row.artifact">
                {{ t('runtimes.updateTo', { version: row.artifact.version }) }}
              </template>
              <template v-else-if="row.runtime?.sites.length">
                {{ t('runtimes.siteCount', {
                  count: row.runtime.sites.length
                }) }}
              </template>
              <template v-else-if="row.runtime">{{ t('runtimes.noSites') }}</template>
              <template v-else-if="row.artifact">
                {{ t('runtimes.onlinePackageSize', { size: formatRuntimeBytes(row.artifact.size) }) }}
              </template>
            </p>
            <div
              v-if="operationForArtifact(row.artifact)"
              class="runtime-row-progress"
            >
              <progress
                :value="operationForArtifact(row.artifact)?.bytesDownloaded ?? 0"
                :max="operationForArtifact(row.artifact)?.totalBytes || 1"
              />
              <small>
                {{ operationStatusLabel(row.artifact) }}
                · {{ operationProgress(row.artifact) }}%
              </small>
            </div>
          </div>
        </div>

        <div class="runtime-actions">
          <span v-if="row.runtime?.active" class="state-pill" data-state="running">
            {{ t('runtimes.globalVersion') }}
          </span>
          <span
            v-else-if="row.state === 'installed'"
            class="state-pill"
            data-state="installed"
          >
            {{ t('state.installed') }}
          </span>
          <span v-else-if="row.state === 'update-available'" class="state-pill" data-state="warning">
            {{ t('runtimes.updateAvailable') }}
          </span>
          <span v-else class="state-pill">{{ t('state.notInstalled') }}</span>
          <button
            v-if="row.artifact && !isRuntimeDownloadActive(operationForArtifact(row.artifact)?.status ?? 'failed')"
            class="primary-button"
            :disabled="action !== null"
            @click="installOrUpdateOnlineRuntime(row.artifact)"
          >
            {{ t(row.state === 'update-available' ? 'runtimes.update' : 'runtimes.install') }}
          </button>
          <button
            v-if="row.artifact && isRuntimeDownloadActive(operationForArtifact(row.artifact)?.status ?? 'failed')"
            class="danger-button"
            @click="cancelOnlineDownload"
          >
            {{ t('runtimes.onlineCancelDownload') }}
          </button>
          <button
            v-if="row.runtime"
            class="secondary-button"
            :disabled="action !== null"
            @click="editPhpIni(row.runtime)"
          >
            php.ini
          </button>
          <button
            v-if="row.runtime && !row.runtime.active"
            class="secondary-button"
            :disabled="action !== null"
            @click="setGlobal(row.runtime)"
          >
            {{ action === `global:${row.runtime.version}` ? t('runtimes.switching') : t('runtimes.setGlobal') }}
          </button>
          <button
            v-if="row.runtime"
            class="danger-button"
            :disabled="action !== null || row.runtime.active || row.runtime.sites.length > 0"
            :title="
              row.runtime.active
                ? t('runtimes.switchGlobalFirst')
                : row.runtime.sites.length
                  ? t('runtimes.usedBySite')
                  : t('runtimes.removeRuntime')
            "
            @click="removeRuntime(row.runtime)"
          >
            {{ action === `remove:${row.runtime.version}` ? t('runtimes.removing') : t('runtimes.remove') }}
          </button>
        </div>
      </article>
    </section>

    <section class="terminal-card" :aria-label="t('runtimes.terminalTitle')">
      <div class="terminal-details">
        <div>
          <p class="eyebrow">{{ t('runtimes.terminalEyebrow') }}</p>
          <h2>{{ t('runtimes.terminalTitle') }}</h2>
          <p>{{ t(isWindows ? 'runtimes.terminalDescriptionWindows' : 'runtimes.terminalDescriptionMac') }}</p>
        </div>
        <span
          class="state-pill"
          :data-state="store.terminalPhp?.enabled ? 'running' : 'installed'"
        >
          {{ t(store.terminalPhp?.enabled ? 'runtimes.terminalOn' : 'runtimes.terminalOff') }}
        </span>
      </div>
      <code v-if="store.terminalPhp">{{ formatPathForDisplay(store.terminalPhp.binPath, isWindows) }}</code>
      <div class="runtime-actions">
        <button
          class="secondary-button"
          :disabled="action !== null || !store.phpRuntimes.globalVersion"
          @click="enableTerminalPhp"
        >
          {{ action === 'terminal-php:enable'
            ? t('runtimes.terminalWorking')
            : t(store.terminalPhp?.enabled ? 'runtimes.terminalRepair' : 'runtimes.terminalEnable') }}
        </button>
        <button
          v-if="store.terminalPhp?.enabled"
          class="danger-button"
          :disabled="action !== null"
          @click="disableTerminalPhp"
        >
          {{ action === 'terminal-php:disable' ? t('runtimes.terminalWorking') : t('runtimes.terminalDisable') }}
        </button>
      </div>
      <small>{{ t('runtimes.terminalRestartHelp') }}</small>
    </section>

    <section v-if="phpIniSeries" class="php-ini-editor">
      <div class="php-ini-header">
        <div>
          <p class="eyebrow">
            PHP {{ phpIniSeries }}
          </p>
          <h2>php.ini</h2>
        </div>
        <div class="header-actions">
          <button class="secondary-button" :disabled="action !== null" @click="revealPhpIni">
            {{ showInFileManagerLabel }}
          </button>
          <button class="secondary-button" :disabled="action !== null" @click="loadErpPhpIni">
            {{ t('runtimes.erpConfig') }}
          </button>
          <button class="secondary-button" :disabled="action !== null" @click="phpIniSeries = null">
            {{ t('runtimes.close') }}
          </button>
        </div>
      </div>
      <textarea v-model="phpIniContents" spellcheck="false" :aria-label="t('runtimes.iniContents')" />
      <div class="php-ini-actions">
        <button class="primary-button" :disabled="action !== null" @click="savePhpIni">
          {{ action?.startsWith('save-ini:') ? t('runtimes.applying') : t('runtimes.saveApply') }}
        </button>
        <small>
          {{ t('runtimes.iniHelp') }}
        </small>
      </div>
    </section>

    <p class="runtime-footnote">
      {{ t('runtimes.footnote') }}
    </p>
  </div>
</template>
