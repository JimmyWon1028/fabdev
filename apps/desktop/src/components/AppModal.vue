<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId } from 'vue'

const props = withDefaults(defineProps<{
  title: string
  description?: string
  closeLabel: string
  busy?: boolean
  size?: 'default' | 'wide' | 'manual'
}>(), {
  description: '',
  busy: false,
  size: 'default'
})

const emit = defineEmits<{
  close: []
}>()

const dialog = ref<HTMLElement | null>(null)
const titleId = useId()
const descriptionId = useId()
let previouslyFocused: HTMLElement | null = null

const focusableSelector = [
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[href]',
  '[tabindex]:not([tabindex="-1"])'
].join(',')

function requestClose() {
  if (!props.busy) {
    emit('close')
  }
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    requestClose()
    return
  }

  if (event.key !== 'Tab' || !dialog.value) {
    return
  }

  const focusable = Array.from(
    dialog.value.querySelectorAll<HTMLElement>(focusableSelector)
  ).filter((element) => element.getAttribute('aria-hidden') !== 'true')

  if (focusable.length === 0) {
    event.preventDefault()
    dialog.value.focus()
    return
  }

  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

onMounted(() => {
  previouslyFocused = document.activeElement instanceof HTMLElement
    ? document.activeElement
    : null
  void nextTick(() => {
    const preferred = dialog.value?.querySelector<HTMLElement>('[autofocus]')
    const first = dialog.value?.querySelector<HTMLElement>(focusableSelector)
    ;(preferred ?? first ?? dialog.value)?.focus()
  })
})

onBeforeUnmount(() => {
  previouslyFocused?.focus()
})
</script>

<template>
  <Teleport to="body">
    <div class="modal-overlay" @click.self="requestClose">
      <section
        ref="dialog"
        class="modal-dialog"
        :class="`modal-dialog--${size}`"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        :aria-describedby="description ? descriptionId : undefined"
        :aria-busy="busy"
        tabindex="-1"
        @keydown="handleKeydown"
      >
        <header class="modal-header">
          <div>
            <h2 :id="titleId">{{ title }}</h2>
            <p v-if="description" :id="descriptionId">{{ description }}</p>
          </div>
          <button
            type="button"
            class="modal-close-button"
            :disabled="busy"
            :aria-label="closeLabel"
            @click="requestClose"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </header>
        <div class="modal-content">
          <slot />
        </div>
      </section>
    </div>
  </Teleport>
</template>
