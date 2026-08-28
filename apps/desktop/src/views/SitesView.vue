<script setup lang="ts">
import type { Site } from '@fabdev/contracts'
import { confirm, open, save } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, reactive, ref, watch } from 'vue'

import AppModal from '../components/AppModal.vue'
import {
  parseSitesImport,
  selectNewSites,
  serializeSites
} from '../utils/config-transfer'
import {
  filterAndSortSites,
  inferDomain,
  removeSites,
  type SiteListFilter,
  type SiteListSort
} from '../utils/site'
import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'
import { formatPathForDisplay, isWindowsPlatform } from '../utils/path'
import { installedPhpSeries as listInstalledPhpSeries, phpSeriesFromVersion } from '../utils/runtime'

const store = useAppStore()
const { t } = useI18n()
const isWindows = isWindowsPlatform()
const homePathPlaceholder = isWindows ? 'C:\\Users\\name\\Sites' : '/Users/name/Sites'
const projectPathPlaceholder = isWindows
  ? 'C:\\Users\\name\\Sites\\erp-demo'
  : '/Users/name/Sites/erp-demo'
const submitting = ref(false)
const transferring = ref(false)
const savingHome = ref(false)
const refreshingHome = ref(false)
const removingSiteId = ref<string | null>(null)
const switchingSiteId = ref<string | null>(null)
const securingSiteId = ref<string | null>(null)
const sharingSiteId = ref<string | null>(null)
const showAddForm = ref(false)
const editingSiteId = ref<string | null>(null)
const siteFormMessage = ref('')
const selectedSiteIds = ref(new Set<string>())
const searchQuery = ref('')
const siteFilter = ref<SiteListFilter>('all')
const siteSort = ref<SiteListSort>('domainAsc')
const message = ref('')
const homeMessage = ref('')
const siteHomePath = ref('')
const form = reactive({
  name: '',
  projectPath: '',
  domain: '',
  documentRoot: '',
  phpVersion: ''
})

const installedPhpSeries = computed(() => listInstalledPhpSeries(store.phpRuntimes.installed))
const homeSiteIds = computed(() => new Set(store.siteHome?.siteIds ?? []))
const symbolicLinkSiteIds = computed(() => new Set(store.siteHome?.symbolicLinkSiteIds ?? []))
const sharedSiteIds = computed(() => new Set(
  store.lanShare?.sites.map((shared) => shared.siteId) ?? []
))
const siteFilterOptions = computed<Array<{
  value: SiteListFilter
  label: string
  count: number
}>>(() => [
  { value: 'all', label: t('sites.filterAll'), count: store.sites.length },
  { value: 'shared', label: t('sites.filterShared'), count: sharedSiteIds.value.size },
  { value: 'home', label: t('sites.filterHome'), count: homeSiteIds.value.size },
  {
    value: 'linked',
    label: t('sites.filterLinked'),
    count: store.sites.filter((site) => !homeSiteIds.value.has(site.id)).length
  }
])
const visibleSites = computed(() => filterAndSortSites(store.sites, {
  query: searchQuery.value,
  filter: siteFilter.value,
  sort: siteSort.value,
  homeSiteIds: homeSiteIds.value,
  sharedSiteIds: sharedSiteIds.value
}))
const hasActiveSiteListFilter = computed(() => searchQuery.value.trim() !== '' || siteFilter.value !== 'all')
const selectableVisibleSites = computed(() =>
  visibleSites.value.filter((site) => !homeSiteIds.value.has(site.id))
)
const selectedSites = computed(() =>
  store.sites.filter((site) =>
    !homeSiteIds.value.has(site.id) && selectedSiteIds.value.has(site.id)
  )
)
const allSelectableVisibleSitesSelected = computed(() =>
  selectableVisibleSites.value.length > 0
    && selectableVisibleSites.value.every((site) => selectedSiteIds.value.has(site.id))
)

watch([() => store.sites, homeSiteIds], ([sites, currentHomeSiteIds]) => {
  const selectableIds = new Set(
    sites
      .filter((site) => !currentHomeSiteIds.has(site.id))
      .map((site) => site.id)
  )
  selectedSiteIds.value = new Set(
    [...selectedSiteIds.value].filter((siteId) => selectableIds.has(siteId))
  )
})

function globalPhpSeries() {
  return phpSeriesFromVersion(store.phpRuntimes.globalVersion)
}

onMounted(() => {
  void store.loadSiteHome()
    .then((siteHome) => Promise.all([
      Promise.resolve(siteHome),
      store.loadSites(),
      store.loadPhpRuntimes(),
      store.loadLanShare()
    ]))
    .then(([siteHome]) => {
      siteHomePath.value = formatPathForDisplay(siteHome.path, isWindows)
      const preferred = globalPhpSeries() ?? installedPhpSeries.value[0]
      if (preferred) {
        form.phpVersion = preferred
      }
    })
    .catch((error) => {
      message.value = error instanceof Error ? error.message : String(error)
    })
})

function isHomeSite(site: Site) {
  return store.siteHome?.siteIds.includes(site.id) ?? false
}

function isSymbolicLinkSite(site: Site) {
  return symbolicLinkSiteIds.value.has(site.id)
}

function isLanShared(site: Site) {
  return store.lanShare?.sites.some((shared) => shared.siteId === site.id) ?? false
}

function isSiteSelectable(site: Site) {
  return !homeSiteIds.value.has(site.id)
}

function isSiteSelected(siteId: string) {
  return selectedSiteIds.value.has(siteId)
}

function toggleSite(site: Site) {
  if (!isSiteSelectable(site)) {
    return
  }
  const selected = new Set(selectedSiteIds.value)
  if (selected.has(site.id)) {
    selected.delete(site.id)
  } else {
    selected.add(site.id)
  }
  selectedSiteIds.value = selected
}

function toggleVisibleSites() {
  const selected = new Set(selectedSiteIds.value)
  for (const site of selectableVisibleSites.value) {
    if (allSelectableVisibleSitesSelected.value) {
      selected.delete(site.id)
    } else {
      selected.add(site.id)
    }
  }
  selectedSiteIds.value = selected
}

function clearSiteListFilter() {
  searchQuery.value = ''
  siteFilter.value = 'all'
}

function resetSiteForm() {
  form.name = ''
  form.projectPath = ''
  form.domain = ''
  form.documentRoot = ''
  form.phpVersion = globalPhpSeries() ?? installedPhpSeries.value[0] ?? ''
}

function closeSiteForm() {
  showAddForm.value = false
  editingSiteId.value = null
  siteFormMessage.value = ''
  resetSiteForm()
}

function openAddSiteForm() {
  editingSiteId.value = null
  siteFormMessage.value = ''
  resetSiteForm()
  showAddForm.value = true
}

function editSite(site: Site) {
  editingSiteId.value = site.id
  siteFormMessage.value = ''
  form.name = site.name
  form.projectPath = formatPathForDisplay(site.projectPath, isWindows)
  form.domain = site.domain
  form.documentRoot = formatPathForDisplay(site.documentRoot, isWindows)
  form.phpVersion = site.phpVersion ?? ''
  showAddForm.value = true
}

async function chooseSiteHome() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: t('sites.chooseHome')
  })
  if (typeof selected === 'string') {
    siteHomePath.value = formatPathForDisplay(selected, isWindows)
  }
}

async function saveSiteHome() {
  savingHome.value = true
  homeMessage.value = ''
  try {
    const settings = await store.saveSiteHome(siteHomePath.value)
    siteHomePath.value = formatPathForDisplay(settings.path, isWindows)
    homeMessage.value = t('sites.homeSaved', {
      path: formatPathForDisplay(settings.path, isWindows)
    })
  } catch (error) {
    homeMessage.value = error instanceof Error ? error.message : String(error)
  } finally {
    savingHome.value = false
  }
}

async function refreshSiteHome() {
  refreshingHome.value = true
  homeMessage.value = ''
  try {
    const settings = await store.loadSiteHome()
    siteHomePath.value = formatPathForDisplay(settings.path, isWindows)
    await store.loadSites()
    homeMessage.value = t('sites.homeRefreshed')
  } catch (error) {
    homeMessage.value = error instanceof Error ? error.message : String(error)
  } finally {
    refreshingHome.value = false
  }
}

async function chooseProject() {
  const selected = await open({
    directory: true,
    multiple: false,
    title: t('sites.chooseProject')
  })
  if (typeof selected === 'string') {
    if (form.projectPath && form.projectPath !== selected) {
      form.documentRoot = ''
    }
    form.projectPath = formatPathForDisplay(selected, isWindows)
    if (!form.domain) {
      form.domain = inferDomain(selected)
    }
  }
}

async function exportSites() {
  transferring.value = true
  message.value = ''
  try {
    const path = await save({
      title: t('sites.exportTitle'),
      defaultPath: 'fabdev-sites.json',
      filters: [{ name: t('sites.transferFilter'), extensions: ['json'] }]
    })
    if (typeof path !== 'string') {
      return
    }
    await store.loadSites()
    await store.writeConfigTransferFile(path, serializeSites(store.sites))
    message.value = t('sites.exported', { count: store.sites.length })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    transferring.value = false
  }
}

async function importSites() {
  const path = await open({
    directory: false,
    multiple: false,
    title: t('sites.importTitle'),
    filters: [{ name: t('sites.transferFilter'), extensions: ['json'] }]
  })
  if (typeof path !== 'string') {
    return
  }
  transferring.value = true
  message.value = ''
  try {
    const imported = parseSitesImport(await store.readConfigTransferFile(path))
    await store.loadSites()
    const selected = selectNewSites(imported, store.sites)
    let added = 0
    for (const site of selected.items) {
      const created = await store.addSite({
        name: site.name,
        domain: site.domain,
        projectPath: site.projectPath,
        documentRoot: site.documentRoot,
        phpVersion: site.phpVersion
      })
      if (site.secured) {
        await store.setSiteHttps(created.id, true)
      }
      added += 1
    }
    await store.loadPhpRuntimes()
    message.value = t('sites.imported', { added, skipped: selected.skipped })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    transferring.value = false
  }
}

async function submit() {
  submitting.value = true
  siteFormMessage.value = ''
  try {
    if (editingSiteId.value) {
      const updated = await store.updateSite(editingSiteId.value, {
        name: form.name,
        projectPath: form.projectPath,
        domain: form.domain,
        documentRoot: form.documentRoot || undefined
      })
      message.value = t('sites.updated', { name: updated.name, domain: updated.domain })
    } else {
      const added = await store.addSite({
        name: form.name || undefined,
        projectPath: form.projectPath,
        domain: form.domain || undefined,
        documentRoot: form.documentRoot || undefined,
        phpVersion: form.phpVersion || null
      })
      message.value = t('sites.added', { domain: added.domain })
    }
    closeSiteForm()
  } catch (error) {
    siteFormMessage.value = error instanceof Error ? error.message : String(error)
  } finally {
    submitting.value = false
  }
}

async function removeSite(site: Site) {
  const approved = await confirm(
    t('sites.removeConfirm', {
      domain: site.domain,
      projectPath: formatPathForDisplay(site.projectPath, isWindows),
      documentRoot: formatPathForDisplay(site.documentRoot, isWindows)
    }),
    {
      title: t('sites.removeTitle'),
      kind: 'warning',
      okLabel: t('sites.removeTitle'),
      cancelLabel: t('sites.cancel')
    }
  )
  if (!approved) {
    return
  }

  removingSiteId.value = site.id
  message.value = ''
  try {
    await store.removeSite(site.id)
    selectedSiteIds.value = new Set(
      [...selectedSiteIds.value].filter((siteId) => siteId !== site.id)
    )
    message.value = t('sites.removed', { domain: site.domain })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    removingSiteId.value = null
  }
}

async function removeSelectedSites() {
  const selected = selectedSites.value
  if (selected.length === 0) {
    return
  }
  const approved = await confirm(
    t('sites.removeSelectedConfirm', {
      count: selected.length,
      domains: selected.map((site) => site.domain).join(', ')
    }),
    {
      title: t('sites.removeSelectedTitle'),
      kind: 'warning',
      okLabel: t('sites.removeSelected'),
      cancelLabel: t('sites.cancel')
    }
  )
  if (!approved) {
    return
  }

  removingSiteId.value = 'selected'
  message.value = ''
  try {
    const result = await removeSites(
      selected.map((site) => site.id),
      (siteId) => store.removeSite(siteId)
    )
    selectedSiteIds.value = new Set(result.failed.map((failure) => failure.id))
    message.value = result.failed.length > 0
      ? t('sites.removeSelectedPartial', {
          removed: result.removed.length,
          failed: result.failed.length,
          details: result.failed
            .map((failure) => `${failure.id}: ${failure.message}`)
            .join('; ')
        })
      : t('sites.removeSelectedCompleted', { count: result.removed.length })
  } finally {
    removingSiteId.value = null
  }
}

async function switchSitePhp(site: Site, event: Event) {
  const select = event.target as HTMLSelectElement
  const phpVersion = select.value
  if (phpVersion === (site.phpVersion ?? '')) {
    return
  }
  switchingSiteId.value = site.id
  message.value = ''
  try {
    await store.setSitePhp(site.id, phpVersion || null)
    message.value = phpVersion
      ? t('sites.switched', { domain: site.domain, version: phpVersion })
      : t('sites.switchedNoPhp', { domain: site.domain })
  } catch (error) {
    select.value = site.phpVersion ?? ''
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    switchingSiteId.value = null
  }
}

async function openSite(site: Site) {
  message.value = ''
  try {
    await store.openSite(site.domain, site.secured)
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  }
}

async function toggleHttps(site: Site) {
  securingSiteId.value = site.id
  message.value = ''
  try {
    const updated = await store.setSiteHttps(site.id, !site.secured)
    message.value = updated.secured
      ? t('sites.httpsEnabled', { domain: site.domain })
      : t('sites.httpsDisabled', { domain: site.domain })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    securingSiteId.value = null
  }
}

async function toggleLanShare(site: Site) {
  sharingSiteId.value = site.id
  message.value = ''
  try {
    if (isLanShared(site)) {
      await store.stopLanShareSite(site.id)
      message.value = t('sites.lanShareStopped', { domain: site.domain })
      return
    }
    const share = await store.startLanShare(site.id)
    if (!share) {
      throw new Error('Agent returned an unexpected response')
    }
    message.value = t('sites.lanShareStarted', {
      domain: site.domain,
      host: share.host,
      port: share.port
    })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    sharingSiteId.value = null
  }
}
</script>

<template>
  <header class="page-header sites-page-header">
    <div>
      <p class="eyebrow">{{ t('sites.eyebrow') }}</p>
      <h1>{{ t('nav.sites') }}</h1>
      <p>{{ t('sites.description') }}</p>
    </div>
    <div class="header-actions">
      <button
        type="button"
        class="secondary-button"
        :disabled="transferring || !store.connected"
        @click="importSites"
      >
        {{ t('sites.import') }}
      </button>
      <button
        type="button"
        class="secondary-button"
        :disabled="transferring || !store.connected"
        @click="exportSites"
      >
        {{ t('sites.export') }}
      </button>
      <button
        type="button"
        class="primary-button site-add-toggle"
        :disabled="transferring"
        aria-haspopup="dialog"
        @click="openAddSiteForm"
      >
        ＋ {{ t('sites.addTitle') }}
      </button>
    </div>
  </header>

  <div class="page-body">
    <section class="form-card site-home-card">
      <form class="site-home-form" @submit.prevent="saveSiteHome">
        <div class="site-home-path-control">
          <label>
            {{ t('sites.homePath') }}
            <input v-model="siteHomePath" required :placeholder="homePathPlaceholder" />
          </label>
          <button type="button" class="secondary-button" @click="chooseSiteHome">
            {{ t('sites.choose') }}
          </button>
        </div>
        <div class="site-home-actions">
          <button class="primary-button" :disabled="savingHome || !store.connected">
            {{ savingHome ? t('sites.savingHome') : t('sites.saveHome') }}
          </button>
          <button
            type="button"
            class="secondary-button"
            :disabled="refreshingHome || !store.connected"
            @click="refreshSiteHome"
          >
            {{ refreshingHome ? t('sites.refreshingHome') : t('sites.refreshHome') }}
          </button>
        </div>
        <small class="site-home-description">{{ t('sites.homeDescription') }}</small>
        <p v-if="homeMessage" class="form-message">{{ homeMessage }}</p>
      </form>
    </section>

    <div v-if="message" class="notice site-notice">
      <span>{{ message }}</span>
    </div>

    <section class="site-list-toolbar" :aria-label="t('sites.listToolsLabel')">
      <div class="site-list-search-row">
        <label class="site-search-field">
          <span class="visually-hidden">{{ t('sites.searchLabel') }}</span>
          <input
            v-model="searchQuery"
            type="search"
            :placeholder="t('sites.searchPlaceholder')"
          />
        </label>
        <label class="site-sort-select">
          <span class="visually-hidden">{{ t('sites.sortLabel') }}</span>
          <select v-model="siteSort" :aria-label="t('sites.sortLabel')">
            <option value="domainAsc">{{ t('sites.sortDomainAsc') }}</option>
            <option value="domainDesc">{{ t('sites.sortDomainDesc') }}</option>
            <option value="php">{{ t('sites.sortPhp') }}</option>
            <option value="shared">{{ t('sites.sortShared') }}</option>
          </select>
        </label>
      </div>
      <div class="site-list-filter-row">
        <div class="site-filter-buttons">
          <button
            v-for="option in siteFilterOptions"
            :key="option.value"
            type="button"
            class="site-filter-button"
            :class="{ active: siteFilter === option.value }"
            :aria-pressed="siteFilter === option.value"
            @click="siteFilter = option.value"
          >
            {{ option.label }}
            <span>{{ option.count }}</span>
          </button>
        </div>
        <small>
          {{ t('sites.resultsCount', { visible: visibleSites.length, total: store.sites.length }) }}
        </small>
      </div>
    </section>

    <section class="site-list">
      <div v-if="visibleSites.length > 0" class="site-selection-toolbar">
        <label>
          <input
            type="checkbox"
            :checked="allSelectableVisibleSitesSelected"
            :indeterminate="selectableVisibleSites.some((site) => isSiteSelected(site.id))
              && !allSelectableVisibleSitesSelected"
            :disabled="selectableVisibleSites.length === 0 || removingSiteId !== null"
            @change="toggleVisibleSites"
          >
          <span>
            {{ allSelectableVisibleSitesSelected
              ? t('sites.clearSelection')
              : t('sites.selectVisible') }}
          </span>
        </label>
        <span>{{ t('sites.selectedCount', { count: selectedSites.length }) }}</span>
        <button
          type="button"
          class="danger-button"
          :disabled="selectedSites.length === 0 || removingSiteId !== null"
          @click="removeSelectedSites"
        >
          {{ removingSiteId === 'selected' ? t('sites.removing') : t('sites.removeSelected') }}
        </button>
      </div>
      <article
        v-for="site in visibleSites"
        :key="site.id"
        class="site-row"
        :class="{
          'is-shared': isLanShared(site),
          selected: isSiteSelected(site.id)
        }"
      >
        <label class="site-select" :title="isHomeSite(site) ? t('sites.homeManaged') : undefined">
          <input
            type="checkbox"
            :checked="isSiteSelected(site.id)"
            :disabled="!isSiteSelectable(site) || removingSiteId !== null"
            :aria-label="isHomeSite(site)
              ? t('sites.homeManaged')
              : t('sites.selectLabel', { domain: site.domain })"
            @change="toggleSite(site)"
          >
        </label>
        <div class="site-identity">
          <span class="status-dot" :data-state="site.enabled ? 'running' : 'stopped'" />
          <div>
            <strong class="site-name">{{ site.name }}</strong>
            <small class="site-domain">{{ site.domain }}</small>
          </div>
          <button
            type="button"
            class="open-link-icon"
            :disabled="removingSiteId !== null || switchingSiteId !== null || securingSiteId !== null"
            :title="t('sites.openLabel', { domain: site.domain })"
            :aria-label="t('sites.openLabel', { domain: site.domain })"
            @click="openSite(site)"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M14 5h5v5M19 5l-8 8M18 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
            </svg>
          </button>
        </div>

        <div class="site-paths">
          <div>
            <small>{{ t('sites.projectFolder') }}</small>
            <code :title="formatPathForDisplay(site.projectPath, isWindows)">
              {{ formatPathForDisplay(site.projectPath, isWindows) }}
            </code>
          </div>
          <div>
            <small>{{ t('sites.webRootColumn') }}</small>
            <code :title="formatPathForDisplay(site.documentRoot, isWindows)">
              {{ formatPathForDisplay(site.documentRoot, isWindows) }}
            </code>
          </div>
        </div>

        <div class="site-runtime">
          <small>{{ t('sites.phpRuntime') }}</small>
          <div class="site-runtime-controls">
            <label class="site-php-select">
              <span class="visually-hidden">{{ site.domain }} PHP Runtime</span>
              <select
                :value="site.phpVersion ?? ''"
                :disabled="switchingSiteId !== null || removingSiteId !== null || securingSiteId !== null"
                :aria-label="`${site.domain} PHP Runtime`"
                @change="switchSitePhp(site, $event)"
              >
                <option value="">-</option>
                <option v-for="series in installedPhpSeries" :key="series" :value="series">
                  PHP {{ series }}
                </option>
              </select>
              <svg class="site-php-select-icon" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4 6l4-4 4 4M4 10l4 4 4-4" />
              </svg>
            </label>
            <span v-if="switchingSiteId === site.id" class="state-pill">
              {{ t('sites.switching') }}
            </span>
          </div>
        </div>

        <div class="site-state">
          <div class="site-badges">
            <span v-if="isHomeSite(site)" class="state-pill">{{ t('sites.homeBadge') }}</span>
            <span
              v-if="isSymbolicLinkSite(site)"
              class="state-pill"
              :title="t('sites.symbolicLinkDescription')"
            >
              {{ t('sites.symbolicLinkBadge') }}
            </span>
            <span v-if="site.secured" class="state-pill" data-state="running">HTTPS</span>
            <span class="state-pill" :data-state="isLanShared(site) ? 'running' : undefined">
              {{ isLanShared(site) ? t('sites.lanShareActive') : t('sites.lanShareInactive') }}
            </span>
          </div>
          <small :class="['site-share-address', { 'is-inactive': !isLanShared(site) }]">
            {{ isLanShared(site) && store.lanShare
              ? t('sites.lanShareAddress', {
                  host: store.lanShare.host,
                  port: store.lanShare.port
                })
              : t('sites.lanShareUnavailable') }}
          </small>
        </div>

        <div class="site-actions">
          <button
            v-if="!isHomeSite(site)"
            type="button"
            class="secondary-button"
            :disabled="submitting || removingSiteId !== null || switchingSiteId !== null || securingSiteId !== null"
            :aria-label="t('sites.editLabel', { name: site.name })"
            @click="editSite(site)"
          >
            {{ t('sites.edit') }}
          </button>
          <button
            type="button"
            :class="site.secured ? 'share-stop-button' : 'secondary-button'"
            :disabled="securingSiteId !== null || removingSiteId !== null || switchingSiteId !== null"
            :aria-pressed="site.secured"
            @click="toggleHttps(site)"
          >
            {{ securingSiteId === site.id
              ? t('sites.httpsWorking')
              : site.secured
                ? t('sites.httpsDisable')
                : t('sites.httpsEnable') }}
          </button>
          <button
            type="button"
            :class="isLanShared(site) ? 'share-stop-button' : 'secondary-button'"
            :disabled="sharingSiteId !== null || removingSiteId !== null || switchingSiteId !== null || securingSiteId !== null"
            :aria-pressed="isLanShared(site)"
            @click="toggleLanShare(site)"
          >
            {{ sharingSiteId === site.id
              ? t('sites.lanShareWorking')
              : isLanShared(site)
                ? t('sites.lanShareStop')
                : t('sites.lanShareStart') }}
          </button>
          <button
            v-if="!isHomeSite(site)"
            type="button"
            class="danger-button"
            :disabled="removingSiteId !== null || switchingSiteId !== null || securingSiteId !== null"
            :aria-label="t('sites.removeLabel', { domain: site.domain })"
            @click="removeSite(site)"
          >
            {{ removingSiteId === site.id ? t('sites.removing') : t('sites.remove') }}
          </button>
        </div>
      </article>
      <div v-if="visibleSites.length === 0" class="site-empty-state">
        <strong>{{ t('sites.noResults') }}</strong>
        <button
          v-if="hasActiveSiteListFilter"
          type="button"
          class="secondary-button"
          @click="clearSiteListFilter"
        >
          {{ t('sites.clearSearch') }}
        </button>
      </div>
    </section>
  </div>

  <AppModal
    v-if="showAddForm"
    :title="editingSiteId ? t('sites.editTitle') : t('sites.addTitle')"
    :close-label="t('sites.cancel')"
    :busy="submitting"
    @close="closeSiteForm"
  >
    <form class="modal-form site-modal-form" @submit.prevent="submit">
      <label>
        {{ t('sites.name') }}
        <input
          v-model="form.name"
          :required="editingSiteId !== null"
          placeholder="ERP Demo"
          autofocus
        />
      </label>
      <label>
        {{ t('sites.projectFolder') }}
        <div class="input-action">
          <input v-model="form.projectPath" required :placeholder="projectPathPlaceholder" />
          <button type="button" class="secondary-button" @click="chooseProject">
            {{ t('sites.choose') }}
          </button>
        </div>
      </label>
      <label>
        {{ t('sites.domain') }}
        <input v-model="form.domain" required placeholder="erp-demo.test" />
      </label>
      <label>
        {{ t('sites.webRoot') }}
        <input v-model="form.documentRoot" placeholder="public" />
      </label>
      <label v-if="editingSiteId === null">
        {{ t('sites.phpRuntime') }}
        <select v-model="form.phpVersion">
          <option value="">-</option>
          <option v-for="series in installedPhpSeries" :key="series" :value="series">
            PHP {{ series }}{{ series === globalPhpSeries() ? t('sites.globalSuffix') : '' }}
          </option>
        </select>
      </label>
      <p v-if="siteFormMessage" class="modal-message" role="alert">
        {{ siteFormMessage }}
      </p>
      <div class="modal-actions">
        <button
          type="button"
          class="secondary-button"
          :disabled="submitting"
          @click="closeSiteForm"
        >
          {{ t('sites.cancel') }}
        </button>
        <button class="primary-button" :disabled="submitting || !store.connected">
          {{ editingSiteId ? t('sites.saveChanges') : t('sites.add') }}
        </button>
      </div>
    </form>
  </AppModal>
</template>
