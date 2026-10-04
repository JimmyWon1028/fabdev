import { confirm } from '@tauri-apps/plugin-dialog'

import { translate } from './i18n'

export function createDiscardChangesGuard(
  isDirty: () => boolean,
  isBusy: () => boolean = () => false
) {
  //2026.10.04 modify by codex, reason: reject overlapping discard actions until the user answers the current dialog.
  let confirming = false
  return async (): Promise<boolean> => {
    if (confirming || isBusy()) {
      return false
    }
    if (!isDirty()) {
      return true
    }
    confirming = true
    try {
      return await confirm(translate('common.discardConfigChangesConfirm'), {
        title: translate('common.discardConfigChangesTitle'),
        kind: 'warning',
        okLabel: translate('common.discardChanges'),
        cancelLabel: translate('common.keepEditing')
      })
    } finally {
      confirming = false
    }
  }
}
