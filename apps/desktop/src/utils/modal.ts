type OverlayPointerEvent = Pick<PointerEvent, 'currentTarget' | 'pointerId' | 'target'>

export function createOverlayDismissController(onDismiss: () => void) {
  let overlayPointerId: number | null = null

  return {
    handlePointerDown(event: OverlayPointerEvent) {
      overlayPointerId = event.target === event.currentTarget ? event.pointerId : null
    },
    handlePointerUp(event: OverlayPointerEvent) {
      const shouldDismiss = overlayPointerId === event.pointerId
        && event.target === event.currentTarget
      overlayPointerId = null

      if (shouldDismiss) {
        onDismiss()
      }
    },
    handlePointerCancel() {
      overlayPointerId = null
    }
  }
}
