export class RequestGate<T> {
  private generation = 0
  private pending: Promise<T> | null = null

  invalidate() {
    this.generation += 1
    this.pending = null
  }

  run(
    request: () => Promise<T>,
    apply: (value: T) => void,
    onError?: (error: unknown) => void
  ): Promise<T> {
    if (this.pending) {
      return this.pending
    }
    const generation = ++this.generation
    const pending = request().then((value) => {
      if (generation === this.generation) {
        apply(value)
      }
      return value
    }).catch((error: unknown) => {
      if (generation === this.generation) {
        onError?.(error)
      }
      throw error
    }).finally(() => {
      if (this.pending === pending) {
        this.pending = null
      }
    })
    this.pending = pending
    return pending
  }
}
