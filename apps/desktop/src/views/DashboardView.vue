<script setup lang="ts">
import type { ServiceState } from '@fabdev/contracts'
import { computed, onMounted, onUnmounted } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { translateError, useI18n } from '../utils/i18n'
import { installedPhpSeries } from '../utils/runtime'
import {
  areAllServicesRunning,
  canToggleAllServices,
  summarizeProxyConnections,
  type ProxySummaryState
} from '../utils/service'

const store = useAppStore()
const { t } = useI18n()

type DashboardServiceState = ServiceState | ProxySummaryState

interface ServiceCard {
  name: string
  detail: string
  state: DashboardServiceState
}

const canStartServices = computed(() => store.sites.some((site) => site.enabled))
const allServicesRunning = computed(() =>
  store.status ? areAllServicesRunning(store.status) : false
)
const canToggleServices = computed(() =>
  canToggleAllServices(store.busy, allServicesRunning.value, canStartServices.value)
)
const phpFpmPools = computed(() => store.status?.phpFpmPools ?? [])
const saturatedPhpPools = computed(() =>
  phpFpmPools.value.filter((pool) => pool.listenQueue > 0 || pool.maxChildrenReached > 0)
)
const saturatedPhpVersions = computed(() =>
  saturatedPhpPools.value.map((pool) => `PHP ${pool.version}`).join(' / ')
)
const proxySummary = computed(() => summarizeProxyConnections(store.proxyManager.connections))
let statusTimer: number | undefined

function toggleAllServices() {
  if (allServicesRunning.value) {
    void store.stopAll()
    return
  }
  void store.startAll()
}

function pollDashboard() {
  void store.pollStatus()
  void store.loadProxyManager().catch(() => undefined)
}

function refreshDashboard() {
  void Promise.allSettled([
    store.refreshStatus(),
    store.loadSites(),
    store.loadProxyManager()
  ])
}

const services = computed<ServiceCard[]>(() => {
  const cards: ServiceCard[] = [
    { name: 'DNS', detail: '*.test → 127.0.0.1', state: store.status?.dns ?? 'notInstalled' },
    { name: 'Nginx', detail: 'HTTP 127.0.0.1:80', state: store.status?.nginx ?? 'notInstalled' },
    {
      name: 'PHP-FPM',
      detail: store.phpRuntimes.installed.length
        ? `PHP ${installedPhpSeries(store.phpRuntimes.installed).join(' / ')}`
        : t('dashboard.phpNotInstalled'),
      state: store.status?.phpFpm ?? 'notInstalled'
    },
    {
      name: 'Node.js',
      detail: store.nodeRuntime.activeVersion
        ? t('dashboard.nodeRuntimeDetail', {
            version: store.nodeRuntime.activeVersion
          })
        : t('dashboard.nodeRuntimeAvailable', { version: '20 / 24' }),
      state: store.nodeRuntime.installed.length ? 'installed' : 'notInstalled'
    },
    {
      name: 'MariaDB',
      detail: `TCP 127.0.0.1:${store.mariaDbSettings?.port ?? 3306}`,
      state: store.status?.mariadb ?? 'notInstalled'
    }
  ]

  if (proxySummary.value.total > 0) {
    cards.push({
      name: 'Proxy',
      detail: t('dashboard.proxyDetail', {
        running: proxySummary.value.running,
        total: proxySummary.value.total,
        issues: proxySummary.value.issues
      }),
      state: proxySummary.value.state
    })
  }

  return cards.filter((service) => service.state !== 'notInstalled')
})

const stateLabels = computed<Record<DashboardServiceState, string>>(() => ({
  notInstalled: t('state.notInstalled'),
  installed: t('state.installed'),
  starting: t('state.starting'),
  running: t('state.running'),
  stopping: t('state.stopping'),
  stopped: t('state.stopped'),
  updating: t('state.updating'),
  degraded: t('proxy.degraded'),
  failed: t('state.failed')
}))
const localizedError = computed(() => store.error ? translateError(store.error) : '')

onMounted(() => {
  void store.loadSites().catch((error) => {
    store.setError(error instanceof Error ? error.message : String(error))
  })
  void store.loadMariaDbSettings().catch((error) => {
    store.setError(error instanceof Error ? error.message : String(error))
  })
  pollDashboard()
  statusTimer = window.setInterval(pollDashboard, 5000)
})

onUnmounted(() => {
  if (statusTimer !== undefined) {
    window.clearInterval(statusTimer)
  }
})

</script>

<template>
  <header class="page-header">
    <div>
      <p class="eyebrow">{{ t('dashboard.eyebrow') }}</p>
      <h1>{{ t('dashboard.title') }}</h1>
      <p>{{ t('dashboard.description') }}</p>
    </div>
    <div class="header-actions">
      <button
        :class="allServicesRunning ? 'secondary-button' : 'primary-button'"
        :disabled="!canToggleServices"
        :title="!allServicesRunning && !canStartServices ? t('dashboard.addSiteBeforeStart') : ''"
        @click="toggleAllServices"
      >
        {{ allServicesRunning ? t('dashboard.stopAll') : t('dashboard.startAll') }}
      </button>
      <button class="secondary-button" :disabled="store.busy" @click="refreshDashboard">
        {{ t('common.refresh') }}
      </button>
    </div>
  </header>

  <div class="page-body">
    <div v-if="store.error" class="notice warning">
      <strong>{{ t('dashboard.notReady') }}</strong>
      <span>{{ localizedError }}</span>
    </div>

    <div v-if="saturatedPhpPools.length" class="notice warning">
      <strong>{{ t('dashboard.phpFpmSaturated') }}</strong>
      <span>{{ t('dashboard.phpFpmSaturatedDescription', { versions: saturatedPhpVersions }) }}</span>
    </div>

    <section
      v-if="services.length"
      class="service-grid"
      :aria-label="t('dashboard.serviceStatus')"
    >
      <article v-for="service in services" :key="service.name" class="service-card">
        <div class="service-icon">{{ service.name.slice(0, 1) }}</div>
        <div>
          <h2>{{ service.name }}</h2>
          <p>{{ service.detail }}</p>
          <div v-if="service.name === 'PHP-FPM' && phpFpmPools.length" class="php-pool-metrics">
            <div v-for="pool in phpFpmPools" :key="pool.version" class="php-pool-metric">
              <strong>PHP {{ pool.version }}</strong>
              <span>
                {{ t('dashboard.phpFpmPoolMetrics', {
                  active: pool.activeProcesses,
                  idle: pool.idleProcesses,
                  queue: pool.listenQueue,
                  slow: pool.slowRequests
                }) }}
              </span>
            </div>
          </div>
          <p
            v-else-if="service.name === 'PHP-FPM' && service.state === 'running'"
            class="php-pool-unavailable"
          >
            {{ t('dashboard.phpFpmMetricsUnavailable') }}
          </p>
        </div>
        <span class="state-pill" :data-state="service.state">{{ stateLabels[service.state] }}</span>
      </article>
    </section>

    <section class="milestone-card">
      <p class="eyebrow">{{ t('dashboard.milestone') }}</p>
      <h2>{{ t('dashboard.multiplePhp') }}</h2>
      <p>{{ t('dashboard.multiplePhpDescription') }}</p>
    </section>
  </div>
</template>
