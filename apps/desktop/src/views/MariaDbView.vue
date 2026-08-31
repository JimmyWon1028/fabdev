<script setup lang="ts">
import type {
  RuntimeUpdateArtifact,
  RuntimeUpdateOperation,
  ServiceState
} from '@fabdev/contracts'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { translateError, useI18n } from '../utils/i18n'
import { EMPTY_MARIADB_CONFIG, ERP_MARIADB_CONFIG } from '../utils/mariadb'
import { formatPathForDisplay, isWindowsPlatform } from '../utils/path'
import {
  catalogRuntimeState,
  formatRuntimeBytes,
  isRuntimeDownloadActive,
  latestRuntimeArtifact,
  runtimeProgressPercent
} from '../utils/runtime'

const store = useAppStore()
const { t } = useI18n()
const isWindows = isWindowsPlatform()
const saving = ref(false)
const configSaving = ref(false)
const passwordSaving = ref(false)
const runtimeAction = ref<string | null>(null)
const message = ref('')
const errorTitle = ref('')
const port = ref('3306')
const dataDir = ref('')
const systemSocket = ref('/tmp/mysql.sock')
const configFilename = ref('my.cnf')
const configContents = ref(EMPTY_MARIADB_CONFIG)
const currentPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')
let mounted = true

const mariaDbState = computed(() => store.status?.mariadb ?? 'notInstalled')
const mariaDbRunning = computed(() => mariaDbState.value === 'running')
const managedMariaDbAvailable = computed(() => mariaDbState.value !== 'notInstalled')
const onlineArtifact = computed(() => latestRuntimeArtifact(
  store.runtimeUpdateCheck?.artifacts ?? [],
  'mariadb'
))
const runtimeState = computed(() => {
  const artifact = onlineArtifact.value
  if (!artifact) {
    return managedMariaDbAvailable.value ? 'installed' : 'not-installed'
  }
  return catalogRuntimeState(artifact.activeVersion, artifact)
})
const onlineOperation = computed(() => {
  const operation = store.runtimeUpdateOperation
  const artifact = onlineArtifact.value
  return artifact
    && operation?.name === artifact.name
    && operation.version === artifact.version
    ? operation
    : null
})
const onlineProgress = computed(() => onlineOperation.value
  ? runtimeProgressPercent(
      onlineOperation.value.bytesDownloaded,
      onlineOperation.value.totalBytes
    )
  : 0
)
const localizedError = computed(() => store.error ? translateError(store.error) : '')
const stateLabels = computed<Record<ServiceState, string>>(() => ({
  notInstalled: t('state.notInstalled'),
  installed: t('state.installed'),
  starting: t('state.starting'),
  running: t('state.running'),
  stopping: t('state.stopping'),
  stopped: t('state.stopped'),
  updating: t('state.updating'),
  failed: t('state.failed')
}))

watch(
  () => store.error,
  (error) => {
    if (!error) {
      errorTitle.value = ''
    }
  }
)

async function loadSettings() {
  store.clearError()
  try {
    const settings = await store.loadMariaDbSettings()
    port.value = String(settings.port)
    dataDir.value = formatPathForDisplay(settings.dataDir, isWindows)
    systemSocket.value = settings.systemSocket || '/tmp/mysql.sock'
  } catch (error) {
    store.setError(error instanceof Error ? error.message : String(error))
  }
}

async function loadConfig() {
  store.clearError()
  try {
    const config = await store.loadMariaDbConfig()
    configFilename.value = config.filename
    configContents.value = config.contents
  } catch (error) {
    store.setError(error instanceof Error ? error.message : String(error))
  }
}

onMounted(() => {
  void refreshPage()
})

onBeforeUnmount(() => {
  mounted = false
})

async function refreshPage() {
  await Promise.all([store.refreshStatus(), loadSettings(), loadConfig()])
  if (isWindows) {
    try {
      await store.checkRuntimeUpdates()
    } catch (error) {
      store.setError(error instanceof Error ? error.message : String(error))
    }
  }
}

async function installLocalMariaDbRuntime() {
  const releasePath = await open({
    directory: false,
    multiple: false,
    title: t('dashboard.chooseMariaDbRelease'),
    filters: [{ name: t('runtimes.releaseFilter'), extensions: ['json'] }]
  })
  if (typeof releasePath !== 'string') {
    return
  }
  const artifactPath = await open({
    directory: false,
    multiple: false,
    title: t('dashboard.chooseMariaDbArtifact'),
    filters: [{ name: t('runtimes.packageFilter'), extensions: ['gz'] }]
  })
  if (typeof artifactPath !== 'string') {
    return
  }

  runtimeAction.value = 'install'
  message.value = t('dashboard.installingMariaDb')
  store.clearError()
  try {
    const version = await store.installMariaDbRuntime(artifactPath, releasePath)
    await Promise.all([store.refreshStatus(), loadSettings(), loadConfig()])
    message.value = t('dashboard.mariaDbInstalled', { version })
  } catch (error) {
    message.value = ''
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    runtimeAction.value = null
  }
}

async function installOrUpdateOnlineRuntime() {
  const artifact = onlineArtifact.value
  if (!artifact || mariaDbRunning.value) {
    return
  }
  let operation = onlineOperation.value
  if (operation?.status !== 'verified') {
    operation = await downloadOnlineRuntime(artifact)
  }
  if (operation?.status === 'verified') {
    await installOnlineRuntime(artifact, operation)
  }
}

async function downloadOnlineRuntime(
  artifact: RuntimeUpdateArtifact
): Promise<RuntimeUpdateOperation | null> {
  const approved = await confirm(t('mariadb.onlineDownloadConfirm', {
    version: artifact.version,
    size: formatRuntimeBytes(artifact.size),
    sha256: artifact.sha256
  }), {
    title: t('mariadb.onlineDownloadTitle'),
    kind: 'warning',
    okLabel: t('runtimes.onlineDownload'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return null
  }

  runtimeAction.value = `online-download:${artifact.version}`
  message.value = t('mariadb.onlineDownloading')
  store.clearError()
  try {
    let operation = await store.startRuntimeDownload(artifact.name, artifact.version)
    while (mounted && isRuntimeDownloadActive(operation.status)) {
      await new Promise((resolve) => window.setTimeout(resolve, 250))
      operation = await store.getRuntimeUpdateOperation(operation.operationId)
    }
    if (operation.status === 'verified') {
      message.value = t('mariadb.onlineVerified', { version: operation.version })
      return operation
    }
    if (operation.status === 'failed') {
      throw new Error(operation.error ?? t('runtimes.onlineDownloadFailed'))
    }
    message.value = t('runtimes.onlineCancelled')
  } catch (error) {
    message.value = ''
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    runtimeAction.value = null
  }
  return null
}

async function installOnlineRuntime(
  artifact: RuntimeUpdateArtifact,
  operation: RuntimeUpdateOperation
) {
  const updating = runtimeState.value === 'update-available'
  const approved = await confirm(t('mariadb.onlineInstallConfirm', {
    version: artifact.version,
    size: formatRuntimeBytes(artifact.size),
    sha256: artifact.sha256
  }), {
    title: t(updating ? 'mariadb.onlineUpdateTitle' : 'mariadb.onlineInstallTitle'),
    kind: 'warning',
    okLabel: t(updating ? 'runtimes.update' : 'dashboard.installMariaDb'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }

  runtimeAction.value = `online-install:${artifact.version}`
  message.value = t(updating ? 'mariadb.onlineUpdating' : 'mariadb.onlineInstalling')
  store.clearError()
  try {
    const result = await store.installDownloadedRuntime(operation.operationId)
    if (result.status !== 'completed') {
      throw new Error(result.error ?? t('runtimes.onlineInstallFailed'))
    }
    await Promise.all([store.refreshStatus(), loadSettings(), loadConfig()])
    message.value = t(updating ? 'mariadb.onlineUpdated' : 'mariadb.onlineInstalled', {
      version: result.version
    })
    await store.checkRuntimeUpdates()
  } catch (error) {
    message.value = ''
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    runtimeAction.value = null
  }
}

async function cancelOnlineDownload() {
  const operation = onlineOperation.value
  if (!operation || !isRuntimeDownloadActive(operation.status)) {
    return
  }
  await store.cancelRuntimeDownload(operation.operationId)
}

async function removeMariaDbRuntime() {
  const approved = await confirm(t('dashboard.removeMariaDbConfirm'), {
    title: t('dashboard.removeMariaDbTitle'),
    kind: 'warning',
    okLabel: t('dashboard.removeMariaDb'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }

  runtimeAction.value = 'remove'
  message.value = ''
  store.clearError()
  try {
    const version = await store.removeMariaDbRuntime()
    await Promise.all([store.refreshStatus(), loadSettings()])
    message.value = t('dashboard.mariaDbRemoved', { version })
  } catch (error) {
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    runtimeAction.value = null
  }
}

async function chooseDataDir() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: t('dashboard.chooseMariaDbDataDir')
  })
  if (typeof selected === 'string') {
    dataDir.value = formatPathForDisplay(selected, isWindows)
  }
}

async function saveSettings() {
  saving.value = true
  message.value = ''
  store.clearError()
  try {
    const settings = await store.saveMariaDbSettings({
      port: Number(port.value),
      dataDir: dataDir.value,
      connectionMode: managedMariaDbAvailable.value ? 'managed' : 'system',
      systemSocket: systemSocket.value
    })
    port.value = String(settings.port)
    dataDir.value = formatPathForDisplay(settings.dataDir, isWindows)
    systemSocket.value = settings.systemSocket || '/tmp/mysql.sock'
    message.value = t('dashboard.mariaDbSettingsSaved')
  } catch (error) {
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    saving.value = false
  }
}

async function saveConfig() {
  configSaving.value = true
  message.value = ''
  store.clearError()
  try {
    const config = await store.saveMariaDbConfig(configContents.value)
    configFilename.value = config.filename
    configContents.value = config.contents
    message.value = t('mariadb.configSaved', { filename: config.filename })
  } catch (error) {
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    configSaving.value = false
  }
}

function applyErpConfig() {
  configContents.value = ERP_MARIADB_CONFIG
  message.value = t('mariadb.erpConfigLoaded')
  store.clearError()
}

async function changeRootPassword() {
  message.value = ''
  store.clearError()
  if (newPassword.value !== confirmPassword.value) {
    errorTitle.value = t('mariadb.passwordChangeFailed')
    store.setError(t('mariadb.passwordMismatch'))
    return
  }

  passwordSaving.value = true
  try {
    await store.setMariaDbRootPassword(currentPassword.value, newPassword.value)
    currentPassword.value = ''
    newPassword.value = ''
    confirmPassword.value = ''
    message.value = t('mariadb.passwordChanged')
  } catch (error) {
    errorTitle.value = t('mariadb.passwordChangeFailed')
    store.setError(error instanceof Error ? error.message : String(error))
  } finally {
    passwordSaving.value = false
  }
}
</script>

<template>
  <header class="page-header">
    <div>
      <p class="eyebrow">{{ t('mariadb.eyebrow') }}</p>
      <h1>{{ t('mariadb.title') }}</h1>
      <p>{{ t('mariadb.description') }}</p>
    </div>
    <div class="header-actions">
      <button
        v-if="!isWindows && mariaDbState === 'notInstalled'"
        class="primary-button"
        :disabled="store.busy || runtimeAction !== null"
        @click="installLocalMariaDbRuntime"
      >
        {{ runtimeAction === 'install' ? t('dashboard.installingMariaDb') : t('dashboard.installMariaDb') }}
      </button>
      <button
        v-else-if="mariaDbRunning"
        class="secondary-button"
        :disabled="store.busy || runtimeAction !== null"
        @click="store.stopMariaDb"
      >
        {{ t('dashboard.stopMariaDb') }}
      </button>
      <button
        v-else-if="managedMariaDbAvailable"
        class="primary-button"
        :disabled="store.busy || runtimeAction !== null"
        @click="store.startMariaDb"
      >
        {{ t('dashboard.startMariaDb') }}
      </button>
      <button
        v-if="mariaDbState !== 'notInstalled' && !mariaDbRunning"
        class="danger-button"
        :disabled="store.busy || runtimeAction !== null"
        @click="removeMariaDbRuntime"
      >
        {{ runtimeAction === 'remove' ? t('dashboard.removingMariaDb') : t('dashboard.removeMariaDb') }}
      </button>
      <button class="secondary-button" :disabled="store.busy" @click="refreshPage">
        {{ t('common.refresh') }}
      </button>
    </div>
  </header>

  <div class="page-body">
    <div v-if="store.error" class="notice warning">
      <strong>{{ errorTitle || t('dashboard.notReady') }}</strong>
      <span>{{ localizedError }}</span>
    </div>

    <div v-if="message" class="notice">
      <span>{{ message }}</span>
    </div>

    <section v-if="isWindows && onlineArtifact" class="runtime-list">
      <article class="runtime-card">
        <div class="runtime-details">
          <span class="runtime-version">DB</span>
          <div>
            <h2>MariaDB {{ onlineArtifact.version }}</h2>
            <p>
              Windows x64 · {{ formatRuntimeBytes(onlineArtifact.size) }}
              <template v-if="runtimeState === 'update-available' && onlineArtifact.activeVersion">
                · {{ t('mariadb.updateFrom', {
                  current: onlineArtifact.activeVersion,
                  version: onlineArtifact.version
                }) }}
              </template>
            </p>
            <div v-if="onlineOperation" class="runtime-row-progress">
              <progress
                :value="onlineOperation.bytesDownloaded"
                :max="onlineOperation.totalBytes || 1"
              />
              <small>
                {{ t(`runtimes.onlineStatus.${onlineOperation.status}`) }} · {{ onlineProgress }}%
              </small>
            </div>
          </div>
        </div>
        <div class="runtime-actions">
          <span
            class="state-pill"
            :data-state="runtimeState === 'update-available' ? 'warning' : managedMariaDbAvailable ? 'installed' : undefined"
          >
            {{ runtimeState === 'update-available'
              ? t('runtimes.updateAvailable')
              : managedMariaDbAvailable
                ? t('state.installed')
                : t('state.notInstalled') }}
          </span>
          <button
            v-if="runtimeState !== 'installed' && !isRuntimeDownloadActive(onlineOperation?.status ?? 'failed')"
            class="primary-button"
            :disabled="store.busy || runtimeAction !== null || mariaDbRunning"
            :title="mariaDbRunning ? t('mariadb.stopBeforeUpdate') : undefined"
            @click="installOrUpdateOnlineRuntime"
          >
            {{ runtimeState === 'update-available'
              ? t('runtimes.update')
              : t('dashboard.installMariaDb') }}
          </button>
          <button
            v-if="isRuntimeDownloadActive(onlineOperation?.status ?? 'failed')"
            class="danger-button"
            @click="cancelOnlineDownload"
          >
            {{ t('runtimes.onlineCancelDownload') }}
          </button>
        </div>
      </article>
    </section>

    <section v-if="managedMariaDbAvailable" class="form-card mariadb-settings-card">
      <div class="mariadb-settings-heading">
        <div>
          <h2>{{ t('dashboard.mariaDbSettingsTitle') }}</h2>
          <p>TCP 127.0.0.1:{{ store.mariaDbSettings?.port ?? 3306 }}</p>
        </div>
        <span class="state-pill" :data-state="mariaDbState">{{ stateLabels[mariaDbState] }}</span>
      </div>
      <p>{{ t('dashboard.mariaDbSettingsDescription') }}</p>
      <form @submit.prevent="saveSettings">
        <label>
          {{ t('dashboard.mariaDbPort') }}
          <input
            v-model="port"
            type="number"
            min="1024"
            max="65535"
            required
            :disabled="mariaDbRunning || saving"
          />
        </label>
        <label>
          {{ t('dashboard.mariaDbDataDir') }}
          <div class="input-action">
            <input v-model="dataDir" readonly required />
            <button
              type="button"
              class="secondary-button"
              :disabled="mariaDbRunning || saving"
              @click="chooseDataDir"
            >
              {{ t('dashboard.chooseMariaDbDataDir') }}
            </button>
          </div>
          <small>{{ t('dashboard.mariaDbDataDirHelp') }}</small>
        </label>
        <p v-if="mariaDbRunning" class="form-message warning-text">
          {{ t('dashboard.mariaDbSettingsStopFirst') }}
        </p>
        <div class="mariadb-settings-actions">
          <button type="submit" class="primary-button" :disabled="mariaDbRunning || saving">
            {{ saving ? t('dashboard.savingMariaDbSettings') : t('dashboard.saveMariaDbSettings') }}
          </button>
        </div>
      </form>
    </section>

    <section v-if="managedMariaDbAvailable" class="form-card mariadb-settings-card mariadb-password-card">
      <h2>{{ t('mariadb.rootPasswordTitle') }}</h2>
      <p>{{ t('mariadb.rootPasswordDescription') }}</p>
      <form @submit.prevent="changeRootPassword">
        <label>
          {{ t('mariadb.currentPassword') }}
          <input
            v-model="currentPassword"
            type="password"
            autocomplete="current-password"
            :disabled="!mariaDbRunning || passwordSaving"
          />
        </label>
        <label>
          {{ t('mariadb.newPassword') }}
          <input
            v-model="newPassword"
            type="password"
            autocomplete="new-password"
            minlength="1"
            maxlength="256"
            required
            :disabled="!mariaDbRunning || passwordSaving"
          />
        </label>
        <label>
          {{ t('mariadb.confirmPassword') }}
          <input
            v-model="confirmPassword"
            type="password"
            autocomplete="new-password"
            minlength="1"
            maxlength="256"
            required
            :disabled="!mariaDbRunning || passwordSaving"
          />
        </label>
        <p v-if="!mariaDbRunning" class="form-message warning-text">
          {{ t('mariadb.passwordStartFirst') }}
        </p>
        <div class="mariadb-settings-actions">
          <button type="submit" class="primary-button" :disabled="!mariaDbRunning || passwordSaving">
            {{ passwordSaving ? t('mariadb.changingPassword') : t('mariadb.changePassword') }}
          </button>
        </div>
      </form>
    </section>

    <section v-if="managedMariaDbAvailable" class="php-ini-editor mariadb-config-editor">
      <div class="php-ini-header">
        <div>
          <p class="eyebrow">{{ t('mariadb.configTitle') }}</p>
          <h2>{{ t('mariadb.configContents', { filename: configFilename }) }}</h2>
        </div>
        <button
          type="button"
          class="secondary-button"
          :disabled="mariaDbRunning || configSaving"
          @click="applyErpConfig"
        >
          {{ t('mariadb.erpConfig') }}
        </button>
      </div>
      <textarea
        v-model="configContents"
        spellcheck="false"
        :aria-label="t('mariadb.configContents', { filename: configFilename })"
        :disabled="mariaDbRunning || configSaving"
      ></textarea>
      <div class="php-ini-actions">
        <span>{{ t('mariadb.configHelp') }}</span>
        <button
          type="button"
          class="primary-button"
          :disabled="mariaDbRunning || configSaving || mariaDbState === 'notInstalled'"
          @click="saveConfig"
        >
          {{ configSaving ? t('mariadb.savingConfig') : t('mariadb.saveConfig') }}
        </button>
      </div>
      <p v-if="mariaDbRunning" class="form-message warning-text">
        {{ t('dashboard.mariaDbSettingsStopFirst') }}
      </p>
    </section>
  </div>
</template>
