final class SystemServiceController {
  private let proxyController: PortProxyController
  private let resolverManager: ResolverManager

  init(
    proxyController: PortProxyController,
    resolverManager: ResolverManager
  ) {
    self.proxyController = proxyController
    self.resolverManager = resolverManager
  }

  func start() throws {
    try proxyController.start()
    do {
      try resolverManager.install()
    } catch {
      proxyController.stop()
      throw error
    }
  }

  func stop() throws {
    proxyController.stop()
    try resolverManager.remove()
  }

  func isRunning() throws -> Bool {
    guard proxyController.isRunning else {
      return false
    }
    return try resolverManager.isInstalled()
  }
}
