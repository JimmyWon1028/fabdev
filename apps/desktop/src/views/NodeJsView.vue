<script setup lang="ts">
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, onMounted, ref } from 'vue'

import { useAppStore } from '../stores/fabdev'
import { useI18n } from '../utils/i18n'

const store = useAppStore()
const { t } = useI18n()
const action = ref<'refresh' | 'install' | 'remove' | null>(null)
const message = ref('')

const installed = computed(() => store.nodeRuntime.installedVersion !== null)

onMounted(() => {
  void refresh()
})

async function refresh() {
  action.value = 'refresh'
  message.value = ''
  try {
    await store.loadNodeRuntime()
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function installRuntime() {
  const releasePath = await open({
    directory: false,
    multiple: false,
    title: t('node.chooseRelease'),
    filters: [{ name: t('runtimes.releaseFilter'), extensions: ['json'] }]
  })
  if (typeof releasePath !== 'string') {
    return
  }
  const artifactPath = await open({
    directory: false,
    multiple: false,
    title: t('node.chooseArtifact'),
    filters: [{ name: t('runtimes.packageFilter'), extensions: ['gz'] }]
  })
  if (typeof artifactPath !== 'string') {
    return
  }

  action.value = 'install'
  message.value = t('node.installing')
  try {
    const state = await store.installNodeRuntime(artifactPath, releasePath)
    message.value = t('node.installed', { version: state.installedVersion ?? state.stableVersion })
  } catch (error) {
    message.value = error instanceof Error ? error.message : String(error)
  } finally {
    action.value = null
  }
}

async function removeRuntime() {
  const approved = await confirm(t('node.removeConfirm'), {
    title: t('node.removeTitle'),
    kind: 'warning',
    okLabel: t('node.remove'),
    cancelLabel: t('runtimes.cancel')
  })
  if (!approved) {
    return
  }

  action.value = 'remove'
  message.value = ''
  try {
    const version = store.nodeRuntime.installedVersion
    await store.removeNodeRuntime()
    message.value = t('node.removed', { version: version ?? store.nodeRuntime.stableVersion })
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
    <div v-if="message" class="notice">
      <span>{{ message }}</span>
    </div>

    <section class="runtime-list" :aria-label="t('node.listLabel')">
      <article class="runtime-card">
        <div class="runtime-details">
          <span class="runtime-version">LTS</span>
          <div>
            <h2>Node.js {{ store.nodeRuntime.stableVersion }}</h2>
            <p v-if="installed">{{ t('node.installedDescription') }}</p>
            <p v-else>{{ t('node.notInstalledDescription') }}</p>
          </div>
        </div>

        <div class="runtime-actions">
          <span class="state-pill" :data-state="installed ? 'installed' : undefined">
            {{ installed ? t('state.installed') : t('state.notInstalled') }}
          </span>
          <button
            v-if="!installed"
            class="primary-button"
            :disabled="action !== null"
            @click="installRuntime"
          >
            {{ action === 'install' ? t('node.installingShort') : t('node.install') }}
          </button>
          <button
            v-else
            class="danger-button"
            :disabled="action !== null"
            :title="t('node.remove')"
            @click="removeRuntime"
          >
            {{ action === 'remove' ? t('node.removing') : t('node.remove') }}
          </button>
        </div>
      </article>
    </section>

    <p class="runtime-footnote">{{ t('node.isolationNote') }}</p>
  </div>
</template>
