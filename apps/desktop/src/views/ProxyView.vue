<script setup lang="ts">
import type {
  ProxyConnectionInfo,
  ProxyConnectionInput,
  ProxyConnectionState
} from '@fabdev/contracts'
import { open, save } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'

import AppModal from '../components/AppModal.vue'
import { useAppStore } from '../stores/fabdev'
import {
  parseProxyImport,
  selectNewProxyConnections,
  serializeProxyConnections
} from '../utils/config-transfer'
import { useI18n } from '../utils/i18n'
import { filterProxyConnections, removeProxyConnections } from '../utils/proxy'

const store = useAppStore()
const { t } = useI18n()
const action = ref<string | null>(null)
const message = ref('')
const showAddForm = ref(false)
const editingConnectionId = ref<string | null>(null)
const formMessage = ref('')
const allowedOriginsText = ref('')
const searchQuery = ref('')
const selectedConnectionIds = ref(new Set<string>())
const newConnection = reactive<ProxyConnectionInput>({
  id: '',
  domain: '',
  listenPort: 3000,
  target: '',
  allowedOrigins: []
})
let pollTimer: ReturnType<typeof setInterval> | null = null

const connections = computed(() => store.proxyManager.connections)
const runningCount = computed(
  () => connections.value.filter((connection) => connection.state === 'running').length
)
const degradedCount = computed(
  () => connections.value.filter((connection) => connection.state === 'degraded').length
)
const stoppedCount = computed(
  () => connections.value.filter((connection) => connection.state === 'stopped').length
)
const failedCount = computed(
  () => connections.value.filter((connection) => connection.state === 'failed').length
)
const isEditing = computed(() => editingConnectionId.value !== null)
const visibleConnections = computed(() => filterProxyConnections(connections.value, searchQuery.value))
const selectedConnections = computed(() =>
  connections.value.filter((connection) => selectedConnectionIds.value.has(connection.id))
)
const allVisibleConnectionsSelected = computed(
  () => visibleConnections.value.length > 0
    && visibleConnections.value.every((connection) => selectedConnectionIds.value.has(connection.id))
)

watch(connections, (currentConnections) => {
  const currentIds = new Set(currentConnections.map((connection) => connection.id))
  selectedConnectionIds.value = new Set(
    [...selectedConnectionIds.value].filter((connectionId) => currentIds.has(connectionId))
  )
})

onMounted(() => {
  void refresh()
  pollTimer = setInterval(() => {
    if (action.value === null) {
      void store.loadProxyManager().catch(() => undefined)
    }
  }, 3_000)
})

onUnmounted(() => {
  if (pollTimer !== null) {
    clearInterval(pollTimer)
  }
})

function isActive(connection: ProxyConnectionInfo) {
  return connection.state === 'running' || connection.state === 'degraded'
}

function stateLabel(state: ProxyConnectionState) {
  if (state === 'degraded') {
    return t('proxy.degraded')
  }
  return t(`state.${state}` as 'state.running')
}

function connectionById(connectionId: string) {
  return connections.value.find((connection) => connection.id === connectionId)
}

function isSelected(connectionId: string) {
  return selectedConnectionIds.value.has(connectionId)
}

function toggleConnection(connectionId: string) {
  const selected = new Set(selectedConnectionIds.value)
  if (selected.has(connectionId)) {
    selected.delete(connectionId)
  } else {
    selected.add(connectionId)
  }
  selectedConnectionIds.value = selected
}

function toggleAllConnections() {
  const selected = new Set(selectedConnectionIds.value)
  for (const connection of visibleConnections.value) {
    if (allVisibleConnectionsSelected.value) {
      selected.delete(connection.id)
    } else {
      selected.add(connection.id)
    }
  }
  selectedConnectionIds.value = selected
}

function clearSearch() {
  searchQuery.value = ''
}

function nextAvailablePort() {
  const usedPorts = new Set(connections.value.map((connection) => connection.listenPort))
  for (let port = 3000; port <= 65535; port += 1) {
    if (!usedPorts.has(port)) {
      return port
    }
  }
  return 3000
}

function openAddForm() {
  editingConnectionId.value = null
  newConnection.id = ''
  newConnection.domain = ''
  newConnection.listenPort = nextAvailablePort()
  newConnection.target = ''
  newConnection.allowedOrigins = []
  allowedOriginsText.value = ''
  showAddForm.value = true
  formMessage.value = ''
}

function openEditForm(connection: ProxyConnectionInfo) {
  editingConnectionId.value = connection.id
  newConnection.id = connection.id
  newConnection.domain = connection.domain
  newConnection.listenPort = connection.listenPort
  newConnection.target = connection.target
  newConnection.allowedOrigins = [...connection.allowedOrigins]
  allowedOriginsText.value = connection.allowedOrigins.join('\n')
  showAddForm.value = true
  formMessage.value = ''
}

function closeAddForm() {
  showAddForm.value = false
  editingConnectionId.value = null
  formMessage.value = ''
}

function allowedOrigins() {
  return allowedOriginsText.value
    .split(/[\n,]/)
    .map((origin) => origin.trim())
    .filter(Boolean)
}

async function refresh() {
  action.value = 'refresh'
  message.value = ''
  try {
    await store.loadProxyManager()
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function exportConnections() {
  action.value = 'export'
  message.value = ''
  try {
    const path = await save({
      title: t('proxy.exportTitle'),
      defaultPath: 'fabdev-proxy.json',
      filters: [{ name: t('proxy.transferFilter'), extensions: ['json'] }]
    })
    if (typeof path !== 'string') {
      return
    }
    await store.loadProxyManager()
    await store.writeConfigTransferFile(path, serializeProxyConnections(connections.value))
    message.value = t('proxy.exported', { count: connections.value.length })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function importConnections() {
  const path = await open({
    directory: false,
    multiple: false,
    title: t('proxy.importTitle'),
    filters: [{ name: t('proxy.transferFilter'), extensions: ['json'] }]
  })
  if (typeof path !== 'string') {
    return
  }
  action.value = 'import'
  message.value = ''
  try {
    const imported = parseProxyImport(await store.readConfigTransferFile(path))
    await store.loadProxyManager()
    const selected = selectNewProxyConnections(imported, connections.value)
    let added = 0
    for (const connection of selected.items) {
      await store.addProxyConnection(connection)
      added += 1
    }
    message.value = t('proxy.imported', { added, skipped: selected.skipped })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function startAll() {
  action.value = 'all-start'
  message.value = ''
  try {
    const result = await store.startAllProxyConnections()
    const failures = result.connections.filter(
      (connection) => connection.state === 'failed'
    )
    message.value = failures.length
      ? t('proxy.startAllPartial', {
          started: result.connections.length - failures.length,
          failed: failures.length
        })
      : t('proxy.startAllCompleted', { count: result.connections.length })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function stopAll() {
  action.value = 'all-stop'
  message.value = ''
  try {
    const result = await store.stopAllProxyConnections()
    message.value = t('proxy.stopAllCompleted', { count: result.connections.length })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function saveConnection() {
  const connectionId = editingConnectionId.value
  action.value = connectionId ? `edit:${connectionId}` : 'add'
  formMessage.value = ''
  const input: ProxyConnectionInput = {
    id: newConnection.id,
    domain: newConnection.domain,
    listenPort: newConnection.listenPort,
    target: newConnection.target,
    allowedOrigins: allowedOrigins()
  }
  const savedId = newConnection.id.trim().toLowerCase()
  try {
    if (connectionId) {
      await store.updateProxyConnection(connectionId, input)
      message.value = t('proxy.updated', { id: connectionId })
    } else {
      await store.addProxyConnection(input)
      message.value = t('proxy.added', { id: savedId })
    }
    closeAddForm()
  } catch (error) {
    formMessage.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function removeConnection(connection: ProxyConnectionInfo) {
  const confirmed = window.confirm(
    t('proxy.removeConfirm', {
      id: connection.id,
      endpoint: `${connection.listenHost}:${connection.listenPort}`
    })
  )
  if (!confirmed) {
    return
  }
  action.value = `remove:${connection.id}`
  message.value = ''
  try {
    await store.removeProxyConnection(connection.id)
    selectedConnectionIds.value = new Set(
      [...selectedConnectionIds.value].filter((connectionId) => connectionId !== connection.id)
    )
    message.value = t('proxy.removed', { id: connection.id })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function removeSelected() {
  const selected = selectedConnections.value
  if (selected.length === 0) {
    return
  }
  const confirmed = window.confirm(
    t('proxy.removeSelectedConfirm', {
      count: selected.length,
      ids: selected.map((connection) => connection.id).join(', ')
    })
  )
  if (!confirmed) {
    return
  }
  action.value = 'remove-selected'
  message.value = ''
  try {
    const result = await removeProxyConnections(
      selected.map((connection) => connection.id),
      (connectionId) => store.removeProxyConnection(connectionId)
    )
    selectedConnectionIds.value = new Set(result.failed.map((failure) => failure.id))
    message.value = result.failed.length > 0
      ? t('proxy.removeSelectedPartial', {
          removed: result.removed.length,
          failed: result.failed.length,
          details: result.failed
            .map((failure) => `${failure.id}: ${failure.message}`)
            .join('; ')
        })
      : t('proxy.removeSelectedCompleted', { count: result.removed.length })
  } finally {
    action.value = null
  }
}

async function startConnection(connection: ProxyConnectionInfo) {
  action.value = `start:${connection.id}`
  message.value = ''
  try {
    await store.startProxyConnection(connection.id)
    const updated = connectionById(connection.id)
    message.value = updated?.state === 'failed'
      ? updated.lastError ?? t('proxy.startFailed', { name: connection.name })
      : t('proxy.started', { name: connection.name })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function stopConnection(connection: ProxyConnectionInfo) {
  action.value = `stop:${connection.id}`
  message.value = ''
  try {
    await store.stopProxyConnection(connection.id)
    message.value = t('proxy.stopped', { name: connection.name })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function restartConnection(connection: ProxyConnectionInfo) {
  action.value = `restart:${connection.id}`
  message.value = ''
  try {
    await store.restartProxyConnection(connection.id)
    const updated = connectionById(connection.id)
    message.value = updated?.state === 'failed'
      ? updated.lastError ?? t('proxy.startFailed', { name: connection.name })
      : t('proxy.restarted', { name: connection.name })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function openConnection(connection: ProxyConnectionInfo) {
  action.value = `open:${connection.id}`
  message.value = ''
  try {
    if (!isActive(connection)) {
      await store.startProxyConnection(connection.id)
    }
    const updated = connectionById(connection.id) ?? connection
    if (!isActive(updated)) {
      throw new Error(updated.lastError ?? t('proxy.startFailed', { name: connection.name }))
    }
    await store.openProxyInChrome(updated.domain, updated.listenPort)
    message.value = t('proxy.opened', { domain: updated.domain, port: updated.listenPort })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}
</script>

<template>
  <header class="page-header proxy-page-header">
    <div>
      <p class="eyebrow">{{ t('proxy.eyebrow') }}</p>
      <h1>{{ t('proxy.title') }}</h1>
      <p>{{ t('proxy.description') }}</p>
    </div>
    <div class="header-actions">
      <button class="secondary-button" :disabled="action !== null" @click="importConnections">
        {{ t('proxy.import') }}
      </button>
      <button class="secondary-button" :disabled="action !== null" @click="exportConnections">
        {{ t('proxy.export') }}
      </button>
      <button
        class="primary-button"
        :disabled="action !== null"
        aria-haspopup="dialog"
        @click="openAddForm"
      >
        ＋ {{ t('proxy.add') }}
      </button>
      <button class="secondary-button" :disabled="action !== null" @click="refresh">
        {{ t('common.refresh') }}
      </button>
      <button class="danger-button" :disabled="action !== null" @click="stopAll">
        {{ action === 'all-stop' ? t('proxy.working') : t('proxy.stopAll') }}
      </button>
      <button class="primary-button" :disabled="action !== null" @click="startAll">
        {{ action === 'all-start' ? t('proxy.working') : t('proxy.startAll') }}
      </button>
    </div>
  </header>

  <div class="page-body">
    <div v-if="message" class="notice">
      <span>{{ message }}</span>
    </div>

    <section class="proxy-summary" :aria-label="t('proxy.summary')">
      <div>
        <small>{{ t('proxy.total') }}</small>
        <strong>{{ connections.length }}</strong>
      </div>
      <div>
        <small>{{ t('state.running') }}</small>
        <strong>{{ runningCount }}</strong>
      </div>
      <div>
        <small>{{ t('proxy.degraded') }}</small>
        <strong>{{ degradedCount }}</strong>
      </div>
      <div>
        <small>{{ t('state.stopped') }}</small>
        <strong>{{ stoppedCount }}</strong>
      </div>
      <div>
        <small>{{ t('state.failed') }}</small>
        <strong>{{ failedCount }}</strong>
      </div>
    </section>

    <section class="proxy-list" :aria-label="t('proxy.listLabel')">
      <div v-if="connections.length > 0" class="proxy-search-toolbar">
        <label class="proxy-search-field">
          <span class="visually-hidden">{{ t('proxy.searchLabel') }}</span>
          <input
            v-model="searchQuery"
            type="search"
            :placeholder="t('proxy.searchPlaceholder')"
          >
        </label>
        <small>
          {{ t('proxy.resultsCount', {
            visible: visibleConnections.length,
            total: connections.length
          }) }}
        </small>
        <button
          v-if="searchQuery.trim()"
          type="button"
          class="secondary-button"
          @click="clearSearch"
        >
          {{ t('proxy.clearSearch') }}
        </button>
      </div>
      <div v-if="connections.length === 0" class="proxy-empty">
        {{ t('proxy.empty') }}
      </div>
      <div v-else-if="visibleConnections.length === 0" class="proxy-empty proxy-no-results">
        <strong>{{ t('proxy.noResults') }}</strong>
        <button type="button" class="secondary-button" @click="clearSearch">
          {{ t('proxy.clearSearch') }}
        </button>
      </div>
      <div v-else class="proxy-selection-toolbar">
        <label>
          <input
            type="checkbox"
            :checked="allVisibleConnectionsSelected"
            :indeterminate="visibleConnections.some((connection) => isSelected(connection.id))
              && !allVisibleConnectionsSelected"
            :disabled="action !== null"
            @change="toggleAllConnections"
          >
          <span>
            {{ allVisibleConnectionsSelected
              ? t('proxy.clearSelection')
              : t('proxy.selectAll') }}
          </span>
        </label>
        <span>{{ t('proxy.selectedCount', { count: selectedConnections.length }) }}</span>
        <button
          type="button"
          class="danger-button"
          :disabled="action !== null || selectedConnections.length === 0"
          @click="removeSelected"
        >
          {{ action === 'remove-selected' ? t('proxy.working') : t('proxy.removeSelected') }}
        </button>
      </div>
      <article
        v-for="connection in visibleConnections"
        :key="connection.id"
        class="proxy-row"
        :class="{ selected: isSelected(connection.id) }"
      >
        <label class="proxy-select">
          <input
            type="checkbox"
            :checked="isSelected(connection.id)"
            :disabled="action !== null"
            :aria-label="t('proxy.selectLabel', { id: connection.id })"
            @change="toggleConnection(connection.id)"
          >
        </label>
        <div class="proxy-identity">
          <span class="status-dot" :data-state="connection.state" />
          <div>
            <strong>{{ connection.id }}</strong>
            <small>{{ connection.domain }}</small>
          </div>
          <button
            type="button"
            class="open-link-icon"
            :class="{ busy: action === `open:${connection.id}` }"
            :disabled="action !== null"
            :title="t('proxy.openLabel', { domain: connection.domain })"
            :aria-label="t('proxy.openLabel', { domain: connection.domain })"
            @click="openConnection(connection)"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M14 5h5v5M19 5l-8 8M18 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
            </svg>
          </button>
        </div>

        <div class="proxy-endpoint">
          <small>{{ t('proxy.localEndpoint') }}</small>
          <code>{{ connection.listenHost }}:{{ connection.listenPort }}</code>
        </div>

        <div class="proxy-target">
          <small>{{ t('proxy.remoteTarget') }}</small>
          <code>{{ connection.target }}</code>
        </div>

        <div class="proxy-state">
          <span class="state-pill" :data-state="connection.state">
            {{ stateLabel(connection.state) }}
          </span>
          <small v-if="connection.lastError" :title="connection.lastError">
            {{ connection.lastError }}
          </small>
        </div>

        <div class="proxy-actions">
          <button
            class="secondary-button"
            :disabled="action !== null"
            @click="openEditForm(connection)"
          >
            {{ t('proxy.edit') }}
          </button>
          <button
            v-if="!isActive(connection)"
            class="primary-button"
            :disabled="action !== null"
            @click="startConnection(connection)"
          >
            {{ action === `start:${connection.id}` ? t('proxy.working') : t('proxy.start') }}
          </button>
          <template v-else>
            <button
              class="secondary-button"
              :disabled="action !== null"
              @click="restartConnection(connection)"
            >
              {{ action === `restart:${connection.id}` ? t('proxy.working') : t('proxy.restart') }}
            </button>
            <button
              class="danger-button"
              :disabled="action !== null"
              @click="stopConnection(connection)"
            >
              {{ action === `stop:${connection.id}` ? t('proxy.working') : t('proxy.stop') }}
            </button>
          </template>
          <button
            class="danger-button proxy-remove-button"
            :disabled="action !== null"
            :aria-label="t('proxy.removeLabel', { id: connection.id })"
            @click="removeConnection(connection)"
          >
            {{ action === `remove:${connection.id}` ? t('proxy.working') : t('proxy.remove') }}
          </button>
        </div>
      </article>
    </section>

    <p class="runtime-footnote">{{ t('proxy.loopbackNote') }}</p>
  </div>

  <AppModal
    v-if="showAddForm"
    :title="isEditing ? t('proxy.editTitle') : t('proxy.addTitle')"
    :description="isEditing ? t('proxy.editDescription') : t('proxy.addDescription')"
    :close-label="t('proxy.cancel')"
    :busy="action !== null"
    size="wide"
    @close="closeAddForm"
  >
    <form class="modal-form proxy-modal-form" @submit.prevent="saveConnection">
      <div class="proxy-add-fields">
        <label>
          {{ t('proxy.id') }}
          <input
            v-model.trim="newConnection.id"
            required
            :disabled="isEditing"
            maxlength="63"
            pattern="[a-z0-9-]+"
            placeholder="erp-api"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            autofocus
          >
          <small>{{ t('proxy.idHelp') }}</small>
        </label>
        <label>
          {{ t('proxy.domain') }}
          <input
            v-model.trim="newConnection.domain"
            required
            placeholder="erp-api.test"
            autocomplete="off"
            spellcheck="false"
          >
        </label>
        <label>
          {{ t('proxy.port') }}
          <input
            v-model.number="newConnection.listenPort"
            required
            type="number"
            min="1024"
            max="65535"
          >
        </label>
        <label class="proxy-target-field">
          {{ t('proxy.target') }}
          <input
            v-model.trim="newConnection.target"
            required
            type="url"
            placeholder="http://api.example.com"
            autocomplete="off"
            spellcheck="false"
          >
          <small>{{ t('proxy.targetHelp') }}</small>
        </label>
        <label class="proxy-origins-field">
          {{ t('proxy.allowedOrigins') }}
          <textarea
            v-model="allowedOriginsText"
            rows="3"
            :placeholder="t('proxy.allowedOriginsPlaceholder')"
            spellcheck="false"
          />
          <small>{{ t('proxy.allowedOriginsHelp') }}</small>
        </label>
      </div>
      <p v-if="formMessage" class="modal-message" role="alert">
        {{ formMessage }}
      </p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary-button"
          :disabled="action !== null"
          @click="closeAddForm"
        >
          {{ t('proxy.cancel') }}
        </button>
        <button type="submit" class="primary-button" :disabled="action !== null">
          {{ action === 'add' || action?.startsWith('edit:')
            ? t('proxy.working')
            : isEditing ? t('proxy.saveChanges') : t('proxy.save') }}
        </button>
      </div>
    </form>
  </AppModal>
</template>
