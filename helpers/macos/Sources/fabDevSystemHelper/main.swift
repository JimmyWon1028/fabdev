import Darwin
import Dispatch
import Foundation

do {
  let arguments = Array(CommandLine.arguments.dropFirst())
  if arguments == ["--development"] {
    let proxyController = PortProxyController(configuration: .development)
    try proxyController.start()
    dispatchMain()
  } else if arguments == ["--local-test"] || arguments == ["--community"] {
    guard geteuid() == 0 else {
      throw HelperError.rootPrivilegesRequired
    }
    let resolverManager = ResolverManager.production
    let installedResolver = try resolverManager.prepareForLocalTest()
    let proxyController = PortProxyController(configuration: .production)
    do {
      try proxyController.start()
    } catch {
      if installedResolver {
        try? resolverManager.removeManagedIfPresent()
      }
      throw error
    }
    dispatchMain()
  } else if arguments == ["--remove-local-test-resolver"] {
    guard geteuid() == 0 else {
      throw HelperError.rootPrivilegesRequired
    }
    try ResolverManager.production.removeManagedIfPresent()
  } else {
    let proxyController = PortProxyController(configuration: .production)
    let serviceController = SystemServiceController(
      proxyController: proxyController,
      resolverManager: .production
    )
    let service = XPCService(serviceController: serviceController)
    try service.run()
  }
} catch {
  FileHandle.standardError.write(Data("fabdev-system-helper: \(error.localizedDescription)\n".utf8))
  exit(EXIT_FAILURE)
}
