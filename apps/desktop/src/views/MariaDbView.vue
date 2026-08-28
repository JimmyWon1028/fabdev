<script setup lang="ts">
import type { ServiceState } from '@fabdev/contracts'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref, watch } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { translateError, useI18n } from '../utils/i18n'
import { EMPTY_MARIADB_CONFIG, ERP_MARIADB_CONFIG } from '../utils/mariadb'
import { formatPathForDisplay, isWindowsPlatform } from '../utils/path'

const store = useAppStore()
const { t } = useI18n()
const isWindows = isWindowsPlatform()
const saving = ref(false)
const configSaving = ref(false)
const passwordSaving = ref(false)
const runtimeAction = ref<'install' | 'remove' | null>(null)
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

const mariaDbState = computed(() => store.status?.mariadb ?? 'notInstalled')
const mariaDbRunning = computed(() => mariaDbState.value === 'running')
const managedMariaDbAvailable = computed(() => mariaDbState.value !== 'notInstalled')
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

onMounted(async () => {
  await Promise.all([store.refreshStatus(), loadSettings(), loadConfig()])
})

async function installMariaDbRuntime() {
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
        v-if="mariaDbState === 'notInstalled'"
        class="primary-button"
        :disabled="store.busy || runtimeAction !== null"
        @click="installMariaDbRuntime"
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
        v-else
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
      <button class="secondary-button" :disabled="store.busy" @click="store.refreshStatus">
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
