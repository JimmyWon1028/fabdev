<script setup lang="ts">
import type { PhpRuntimeInfo } from '@fabdev/contracts'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'
import { formatPathForDisplay, isWindowsPlatform } from '../utils/path'
import { buildRuntimeRows, isBuiltInPhpSeries } from '../utils/runtime'

const store = useAppStore()
const { t } = useI18n()
const action = ref<string | null>(null)
const message = ref('')
const phpIniSeries = ref<string | null>(null)
const phpIniContents = ref('')

const rows = computed(() => buildRuntimeRows(store.phpRuntimes.installed))
const isWindows = isWindowsPlatform()
const showInFileManagerLabel = computed(() =>
  t(isWindows ? 'runtimes.showInExplorer' : 'runtimes.showInFinder')
)

onMounted(() => {
  void refresh()
})

async function refresh() {
  action.value = 'refresh'
  message.value = ''
  try {
    await store.loadPhpRuntimes()
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

async function editDefaultPhpIni() {
  action.value = 'ini:default'
  message.value = ''
  try {
    phpIniContents.value = await store.getDefaultPhpIni()
    phpIniSeries.value = 'default'
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
    if (phpIniSeries.value === 'default') {
      await store.saveDefaultPhpIni(phpIniContents.value)
      message.value = t('runtimes.defaultIniSaved')
    } else {
      await store.savePhpIni(phpIniSeries.value, phpIniContents.value)
      message.value = t('runtimes.iniSaved', { version: phpIniSeries.value })
    }
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
    const version = phpIniSeries.value === 'default' ? null : phpIniSeries.value
    phpIniContents.value = await store.getErpPhpIni(version)
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
    const path = phpIniSeries.value === 'default'
      ? await store.revealDefaultPhpIni()
      : await store.revealPhpIni(phpIniSeries.value)
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

    <div v-if="message" class="notice">
      <span>{{ message }}</span>
    </div>

    <section class="runtime-list" :aria-label="t('runtimes.listLabel')">
      <article class="runtime-card">
        <div class="runtime-details">
          <span class="runtime-version">{{ t('runtimes.default') }}</span>
          <div>
            <h2>{{ t('runtimes.defaultIniTitle') }}</h2>
            <p>{{ t('runtimes.defaultIniDescription') }}</p>
          </div>
        </div>
        <div class="runtime-actions">
          <button class="secondary-button" :disabled="action !== null" @click="editDefaultPhpIni">
            php.ini
          </button>
        </div>
      </article>
      <article v-for="row in rows" :key="row.runtime.version" class="runtime-card">
        <div class="runtime-details">
          <span class="runtime-version">{{ row.series }}</span>
          <div>
            <h2>PHP {{ row.runtime.version }}</h2>
            <p>
              macOS ARM64
              <template v-if="row.runtime.sites.length">
                {{ t('runtimes.siteCount', { count: row.runtime.sites.length, sites: row.runtime.sites.join(', ') }) }}
              </template>
              <template v-else>{{ t('runtimes.noSites') }}</template>
            </p>
          </div>
        </div>

        <div class="runtime-actions">
          <span v-if="isBuiltInPhpSeries(row.series)" class="state-pill" data-state="installed">
            {{ t('runtimes.builtIn') }}
          </span>
          <span v-if="row.runtime.active" class="state-pill" data-state="running">
            {{ t('runtimes.globalVersion') }}
          </span>
          <span v-else class="state-pill" data-state="installed">{{ t('state.installed') }}</span>
          <button
            class="secondary-button"
            :disabled="action !== null"
            @click="editPhpIni(row.runtime)"
          >
            php.ini
          </button>
          <button
            v-if="!row.runtime.active"
            class="secondary-button"
            :disabled="action !== null"
            @click="setGlobal(row.runtime)"
          >
            {{ action === `global:${row.runtime.version}` ? t('runtimes.switching') : t('runtimes.setGlobal') }}
          </button>
          <button
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

    <section v-if="phpIniSeries" class="php-ini-editor">
      <div class="php-ini-header">
        <div>
          <p class="eyebrow">
            {{ phpIniSeries === 'default' ? t('runtimes.defaultIniTitle') : `PHP ${phpIniSeries}` }}
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
          {{ phpIniSeries === 'default' ? t('runtimes.defaultIniHelp') : t('runtimes.iniHelp') }}
        </small>
      </div>
    </section>

    <p class="runtime-footnote">
      {{ t('runtimes.footnote') }}
    </p>
  </div>
</template>
