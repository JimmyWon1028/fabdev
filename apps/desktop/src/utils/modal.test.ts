import { describe, expect, it, vi } from 'vitest'

import { createOverlayDismissController } from './modal'

function pointerEvent(target: EventTarget, currentTarget: EventTarget, pointerId = 1) {
  return { target, currentTarget, pointerId }
}

describe('modal overlay dismissal', () => {
  it('dismisses when the pointer starts and ends on the overlay', () => {
    const dismiss = vi.fn()
    const controller = createOverlayDismissController(dismiss)
    const overlay = new EventTarget()

    controller.handlePointerDown(pointerEvent(overlay, overlay))
    controller.handlePointerUp(pointerEvent(overlay, overlay))

    expect(dismiss).toHaveBeenCalledOnce()
  })

  it('keeps the modal open when text selection starts inside the dialog', () => {
    const dismiss = vi.fn()
    const controller = createOverlayDismissController(dismiss)
    const overlay = new EventTarget()
    const input = new EventTarget()

    controller.handlePointerDown(pointerEvent(input, overlay))
    controller.handlePointerUp(pointerEvent(overlay, overlay))

    expect(dismiss).not.toHaveBeenCalled()
  })

  it('keeps the modal open after a cancelled overlay gesture', () => {
    const dismiss = vi.fn()
    const controller = createOverlayDismissController(dismiss)
    const overlay = new EventTarget()

    controller.handlePointerDown(pointerEvent(overlay, overlay))
    controller.handlePointerCancel()
    controller.handlePointerUp(pointerEvent(overlay, overlay))

    expect(dismiss).not.toHaveBeenCalled()
  })
})
