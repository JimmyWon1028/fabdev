<script setup lang="ts">
import { computed } from 'vue'

import { getOperationManual } from '../utils/help'
import { useI18n } from '../utils/i18n'
import AppModal from './AppModal.vue'

const emit = defineEmits<{
  close: []
}>()

const { language } = useI18n()
const manual = computed(() => getOperationManual(language.value))
</script>

<template>
  <AppModal
    :title="manual.title"
    :description="manual.description"
    :close-label="manual.closeLabel"
    size="manual"
    @close="emit('close')"
  >
    <div class="help-manual">
      <aside class="help-contents" :aria-label="manual.contentsLabel">
        <strong>{{ manual.contentsLabel }}</strong>
        <a v-for="section in manual.sections" :key="section.id" :href="`#help-${section.id}`">
          {{ section.title }}
        </a>
      </aside>

      <article class="help-sections">
        <section
          v-for="(section, sectionIndex) in manual.sections"
          :id="`help-${section.id}`"
          :key="section.id"
          class="help-section"
        >
          <span class="help-section-number" aria-hidden="true">
            {{ String(sectionIndex + 1).padStart(2, '0') }}
          </span>
          <div>
            <h3>{{ section.title }}</h3>
            <p>{{ section.summary }}</p>
            <ol v-if="section.steps">
              <li v-for="step in section.steps" :key="step">{{ step }}</li>
            </ol>
            <ul v-if="section.notes" class="help-notes">
              <li v-for="note in section.notes" :key="note">{{ note }}</li>
            </ul>
          </div>
        </section>
      </article>
    </div>
  </AppModal>
</template>
