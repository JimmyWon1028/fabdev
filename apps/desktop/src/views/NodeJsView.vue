<script setup lang="ts">
import type { RuntimeUpdateArtifact, RuntimeUpdateOperation } from '@fabdev/contracts'
import { confirm } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'
import type { TranslationKey } from '../utils/locales'
import { isWindowsPlatform } from '../utils/path'
import {
  buildWindowsNodeRuntimeRows,
  formatRuntimeBytes,
  isRuntimeDownloadActive,
  runtimeProgressPercent
} from '../utils/runtime'

const store = useAppStore()
const { t } = useI18n()
const isWindows = isWindowsPlatform()
const action = ref<string | null>(null)
const message = ref('')
let mounted = true

const rows = computed(() => buildWindowsNodeRuntimeRows(
  store.nodeRuntime.installed,
  store.runtimeUpdateCheck?.artifacts ?? []
))

onMounted(() => void refresh())
onBeforeUnmount(() => { mounted = false })

function operationFor(artifact: RuntimeUpdateArtifact | null) {
  const operation = store.runtimeUpdateOperation
  return artifact
    && operation?.name === artifact.name
    && operation.version === artifact.version
    ? operation
    : null
}

function operationStatusLabel(artifact: RuntimeUpdateArtifact | null) {
  const status = operationFor(artifact)?.status ?? 'failed'
  return t(`runtimes.onlineStatus.${status}` as TranslationKey)
}

function operationProgress(artifact: RuntimeUpdateArtifact | null) {
  const operation = operationFor(artifact)
  return operation
    ? runtimeProgressPercent(operation.bytesDownloaded, operation.totalBytes)
    : 0
}

async function refresh() {
  action.value = 'refresh'
  message.value = ''
  try {
    await store.loadNodeRuntime()
    if (isWindows) {
      await store.checkRuntimeUpdates()
    }
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function installOrUpdate(artifact: RuntimeUpdateArtifact) {
  let operation = operationFor(artifact)
  if (operation?.status !== 'verified') {
    operation = await download(artifact)
  }
  if (operation?.status !== 'verified') {
    return
  }
  const approved = await confirm(t('node.onlineInstallConfirm', {
    version: artifact.version,
    size: formatRuntimeBytes(artifact.size),
    sha256: artifact.sha256
  }), {
    title: t('node.onlineInstallTitle'),
    kind: 'warning',
    okLabel: t('node.install'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }
  action.value = `install:${artifact.version}`
  message.value = t('node.onlineInstalling')
  try {
    const result = await store.installDownloadedRuntime(operation.operationId)
    if (result.status !== 'completed') {
      throw new Error(result.error ?? t('runtimes.onlineInstallFailed'))
    }
    message.value = t('node.onlineInstalled', { version: result.version })
    await refresh()
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function download(artifact: RuntimeUpdateArtifact): Promise<RuntimeUpdateOperation | null> {
  const approved = await confirm(t('node.onlineDownloadConfirm', {
    version: artifact.version,
    size: formatRuntimeBytes(artifact.size),
    sha256: artifact.sha256
  }), {
    title: t('node.onlineDownloadTitle'),
    kind: 'warning',
    okLabel: t('runtimes.onlineDownload'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return null
  }
  action.value = `download:${artifact.version}`
  message.value = t('node.onlineDownloading')
  try {
    let operation = await store.startRuntimeDownload(artifact.name, artifact.version)
    while (mounted && isRuntimeDownloadActive(operation.status)) {
      await new Promise((resolve) => window.setTimeout(resolve, 250))
      operation = await store.getRuntimeUpdateOperation(operation.operationId)
    }
    if (operation.status === 'verified') {
      message.value = t('node.onlineVerified', { version: artifact.version })
      return operation
    }
    if (operation.status === 'failed') {
      throw new Error(operation.error ?? t('runtimes.onlineDownloadFailed'))
    }
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
  return null
}

async function cancelDownload(artifact: RuntimeUpdateArtifact) {
  const operation = operationFor(artifact)
  if (operation && isRuntimeDownloadActive(operation.status)) {
    await store.cancelRuntimeDownload(operation.operationId)
  }
}

async function setGlobal(version: string) {
  action.value = `global:${version}`
  message.value = ''
  try {
    await store.setGlobalNode(version)
    message.value = t('node.globalChanged', { version })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function repairTerminal() {
  action.value = 'terminal'
  try {
    await store.enableTerminalNode()
    message.value = t('node.terminalRepaired')
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function disableTerminal() {
  action.value = 'terminal'
  try {
    await store.disableTerminalNode()
    message.value = t('node.terminalDisabled')
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function removeRuntime(version: string) {
  const approved = await confirm(t('node.removeConfirm', { version }), {
    title: t('node.removeTitle'),
    kind: 'warning',
    okLabel: t('node.remove'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }
  action.value = `remove:${version}`
  try {
    await store.removeNodeRuntime(version)
    message.value = t('node.removed', { version })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}
</script>

<template>
  <header class="page-header">
    <div>
      <p class="eyebrow">{{ t('node.eyebrow') }}</p>
      <h1>{{ t('nav.nodejs') }}</h1>
      <p>{{ t('node.description') }}</p>
    </div>
    <button class="secondary-button" :disabled="action !== null" @click="refresh">
      {{ t('common.refresh') }}
    </button>
  </header>

  <div class="page-body">
    <div v-if="message" class="notice"><span>{{ message }}</span></div>

    <section v-if="store.nodeRuntime.installed.length" class="settings-card">
      <div>
        <p class="eyebrow">{{ t('node.terminalEyebrow') }}</p>
        <h2>{{ t('node.terminalTitle') }}</h2>
        <p>{{ t('node.terminalDescription') }}</p>
      </div>
      <div class="runtime-actions">
        <span class="state-pill" :data-state="store.nodeRuntime.terminal.enabled ? 'installed' : undefined">
          {{ store.nodeRuntime.terminal.enabled ? t('state.enabled') : t('state.disabled') }}
        </span>
        <button class="secondary-button" :disabled="action !== null || !store.nodeRuntime.activeVersion" @click="repairTerminal">
          {{ t('node.repairTerminal') }}
        </button>
        <button v-if="store.nodeRuntime.terminal.enabled" class="danger-button" :disabled="action !== null" @click="disableTerminal">
          {{ t('state.disable') }}
        </button>
      </div>
    </section>

    <section class="runtime-list" :aria-label="t('node.listLabel')">
      <article v-for="row in rows" :key="row.version" class="runtime-card">
        <div class="runtime-details">
          <span class="runtime-version">{{ row.major }}</span>
          <div>
            <h2>Node.js {{ row.version }}</h2>
            <p v-if="row.major === '20'">{{ t('node.eolDescription') }}</p>
            <p v-else-if="row.state === 'update-available'">{{ t('node.updateDescription', { version: row.artifact?.version ?? row.version }) }}</p>
            <p v-else-if="row.runtime">{{ t('node.installedDescription') }}</p>
            <p v-else>{{ t('node.notInstalledDescription') }}</p>
            <div v-if="operationFor(row.artifact)" class="runtime-row-progress">
              <progress :value="operationFor(row.artifact)?.bytesDownloaded ?? 0" :max="operationFor(row.artifact)?.totalBytes || 1" />
              <small>{{ operationStatusLabel(row.artifact) }} · {{ operationProgress(row.artifact) }}%</small>
            </div>
          </div>
        </div>
        <div class="runtime-actions">
          <span class="state-pill" :data-state="row.state === 'update-available' ? 'warning' : row.runtime ? 'installed' : undefined">
            {{ row.runtime?.active ? t('runtimes.globalVersion') : row.state === 'update-available' ? t('runtimes.updateAvailable') : row.runtime ? t('state.installed') : t('state.notInstalled') }}
          </span>
          <button v-if="row.artifact && !isRuntimeDownloadActive(operationFor(row.artifact)?.status ?? 'failed')" class="primary-button" :disabled="action !== null" @click="installOrUpdate(row.artifact)">
            {{ row.state === 'update-available' ? t('runtimes.update') : t('node.install') }}
          </button>
          <button v-if="row.artifact && isRuntimeDownloadActive(operationFor(row.artifact)?.status ?? 'failed')" class="danger-button" @click="cancelDownload(row.artifact)">
            {{ t('runtimes.onlineCancelDownload') }}
          </button>
          <button v-if="row.runtime && !row.runtime.active" class="secondary-button" :disabled="action !== null" @click="setGlobal(row.version)">
            {{ t('runtimes.setGlobal') }}
          </button>
          <button v-if="row.runtime" class="danger-button" :disabled="action !== null" @click="removeRuntime(row.version)">
            {{ t('node.remove') }}
          </button>
        </div>
      </article>
    </section>

    <p class="runtime-footnote">{{ t('node.isolationNote') }}</p>
  </div>
</template>
