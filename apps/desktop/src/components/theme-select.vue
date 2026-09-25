<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId } from 'vue'

import { useI18n } from '../utils/i18n'
import type { TranslationKey } from '../utils/locales'
import { supportedThemes, type Theme } from '../utils/preferences'

const props = defineProps<{ modelValue: Theme; label: string }>()
const emit = defineEmits<{ 'update:modelValue': [theme: Theme] }>()
const { t } = useI18n()
const labels: Record<Theme, TranslationKey> = {
  default: 'settings.themeDefault',
  notion: 'settings.themeNotion',
  'neo-brutalism': 'settings.themeNeoBrutalism',
  glassmorphism: 'settings.themeGlassmorphism',
  cyberpunk: 'settings.themeCyberpunk',
  'cyberpunk-city': 'settings.themeCyberpunkCity',
  graphite: 'settings.themeGraphite',
  'retro-terminal': 'settings.themeRetroTerminal',
  porcelain: 'settings.themeSolarized',
  blueprint: 'settings.themeBlueprint'
}
const listId = useId()
const trigger = ref<HTMLButtonElement | null>(null)
const list = ref<HTMLElement | null>(null)
const open = ref(false)
const position = ref<Record<string, string>>({})
const selectedIndex = computed(() => supportedThemes.indexOf(props.modelValue))

function updatePosition() {
  if (!open.value || !trigger.value) return
  const rect = trigger.value.getBoundingClientRect()
  const below = window.innerHeight - rect.bottom - 12
  const above = rect.top - 12
  const upward = below < 340 && above > below
  position.value = {
    left: `${Math.max(8, Math.min(rect.left, window.innerWidth - rect.width - 8))}px`,
    width: `${rect.width}px`,
    maxHeight: `${Math.max(80, Math.min(420, upward ? above : below))}px`,
    ...(upward ? { bottom: `${window.innerHeight - rect.top + 6}px` } : { top: `${rect.bottom + 6}px` })
  }
}

async function revealSelected() {
  await nextTick()
  updatePosition()
  list.value?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
}

function toggle() {
  // WebKit does not always focus a button when it is clicked.
  trigger.value?.focus({ preventScroll: true })
  open.value = !open.value
  if (open.value) void revealSelected()
}

function select(theme: Theme) {
  emit('update:modelValue', theme)
  open.value = false
  trigger.value?.focus()
}

function handleKeydown(event: KeyboardEvent) {
  if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    event.preventDefault()
    open.value = true
    const index = event.key === 'Home' ? 0
      : event.key === 'End' ? supportedThemes.length - 1
        : Math.max(0, Math.min(supportedThemes.length - 1, selectedIndex.value + (event.key === 'ArrowDown' ? 1 : -1)))
    emit('update:modelValue', supportedThemes[index])
    void revealSelected()
  } else if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    toggle()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    open.value = false
  } else if (event.key === 'Tab') {
    open.value = false
  }
}

function dismissOutside(event: PointerEvent) {
  const target = event.target as Node
  if (!trigger.value?.contains(target) && !list.value?.contains(target)) open.value = false
}

onMounted(() => {
  document.addEventListener('pointerdown', dismissOutside)
  window.addEventListener('resize', updatePosition)
  window.addEventListener('scroll', updatePosition, true)
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', dismissOutside)
  window.removeEventListener('resize', updatePosition)
  window.removeEventListener('scroll', updatePosition, true)
})
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="theme-select"
    role="combobox"
    aria-haspopup="listbox"
    :aria-label="label"
    :aria-expanded="open"
    :aria-controls="open ? listId : undefined"
    :aria-activedescendant="open ? `${listId}-${modelValue}` : undefined"
    @click="toggle"
    @keydown="handleKeydown"
    @blur="open = false"
  >
    <span>{{ t(labels[modelValue]) }}</span>
    <svg width="14" height="16" viewBox="0 0 14 16" fill="none" aria-hidden="true">
      <path d="m3 6 4-4 4 4M3 10l4 4 4-4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      :id="listId"
      ref="list"
      class="theme-menu"
      role="listbox"
      :aria-label="label"
      :style="position"
      @mousedown.prevent
    >
      <div
        v-for="theme in supportedThemes"
        :id="`${listId}-${theme}`"
        :key="theme"
        class="theme-menu-option"
        role="option"
        :aria-selected="theme === modelValue"
        @click="select(theme)"
      >
        <span class="theme-menu-check" aria-hidden="true">{{ theme === modelValue ? '✓' : '' }}</span>
        {{ t(labels[theme]) }}
      </div>
    </div>
  </Teleport>
</template>
