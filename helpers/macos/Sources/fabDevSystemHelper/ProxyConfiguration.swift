import Foundation

struct ProxyConfiguration: Equatable {
  let dnsListenPort: UInt16
  let dnsBackendPort: UInt16
  let httpListenPort: UInt16
  let httpBackendPort: UInt16
  let httpsListenPort: UInt16
  let httpsBackendPort: UInt16

  static let production = ProxyConfiguration(
    dnsListenPort: 53,
    dnsBackendPort: 53_535,
    httpListenPort: 80,
    httpBackendPort: 8_080,
    httpsListenPort: 443,
    httpsBackendPort: 8_443
  )

  static let development = ProxyConfiguration(
    dnsListenPort: 15_353,
    dnsBackendPort: 53_535,
    httpListenPort: 18_080,
    httpBackendPort: 8_080,
    httpsListenPort: 18_443,
    httpsBackendPort: 8_443
  )

  func validate() throws {
    guard dnsBackendPort >= 1_024, httpBackendPort >= 1_024, httpsBackendPort >= 1_024 else {
      throw HelperError.invalidConfiguration("Backends must use unprivileged ports")
    }

    let ports = [
      dnsListenPort,
      dnsBackendPort,
      httpListenPort,
      httpBackendPort,
      httpsListenPort,
      httpsBackendPort,
    ]
    guard Set(ports).count == ports.count else {
      throw HelperError.invalidConfiguration("Proxy ports must be unique")
    }
  }
}

enum HelperError: LocalizedError {
  case invalidConfiguration(String)
  case listenerFailed(String)
  case resolverConflict(String)
  case filesystemOperation(String)
  case rootPrivilegesRequired
  case signingIdentityUnavailable
  case xpcSetupFailed(Int32)

  var errorDescription: String? {
    switch self {
    case .invalidConfiguration(let message):
      return message
    case .listenerFailed(let message):
      return message
    case .resolverConflict(let path):
      return "Refusing to replace resolver configuration at \(path)"
    case .filesystemOperation(let message):
      return message
    case .rootPrivilegesRequired:
      return "The local test helper must run as root"
    case .signingIdentityUnavailable:
      return "The helper requires an Apple-issued code signing identity"
    case .xpcSetupFailed(let code):
      return "Failed to apply the XPC peer requirement (error \(code))"
    }
  }
}
