import XCTest

@testable import SystemHelper

final class ProxyConfigurationTests: XCTestCase {
  func testProductionConfigurationUsesFixedIngressAndUnprivilegedBackends() throws {
    let configuration = ProxyConfiguration.production

    XCTAssertEqual(configuration.dnsListenPort, 53)
    XCTAssertEqual(configuration.dnsBackendPort, 53_535)
    XCTAssertEqual(configuration.httpListenPort, 80)
    XCTAssertEqual(configuration.httpBackendPort, 8_080)
    XCTAssertEqual(configuration.httpsListenPort, 443)
    XCTAssertEqual(configuration.httpsBackendPort, 8_443)
    XCTAssertNoThrow(try configuration.validate())
  }

  func testConfigurationRejectsPrivilegedBackend() {
    let configuration = ProxyConfiguration(
      dnsListenPort: 53,
      dnsBackendPort: 53,
      httpListenPort: 80,
      httpBackendPort: 8_080,
      httpsListenPort: 443,
      httpsBackendPort: 8_443
    )

    XCTAssertThrowsError(try configuration.validate())
  }

  func testConfigurationRejectsDuplicatePorts() {
    let configuration = ProxyConfiguration(
      dnsListenPort: 15_353,
      dnsBackendPort: 53_535,
      httpListenPort: 8_080,
      httpBackendPort: 8_080,
      httpsListenPort: 18_443,
      httpsBackendPort: 8_443
    )

    XCTAssertThrowsError(try configuration.validate())
  }
}
