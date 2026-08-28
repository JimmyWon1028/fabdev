import XCTest

@testable import SystemHelper

final class SigningIdentityTests: XCTestCase {
  func testAppRequirementRestrictsIdentifierAndTeam() {
    let requirement = SigningIdentity.appRequirement(teamIdentifier: "TEAM123")

    XCTAssertTrue(requirement.contains("identifier \"com.fabdev.desktop\""))
    XCTAssertTrue(requirement.contains("certificate leaf[subject.OU] = \"TEAM123\""))
  }
}
