import Foundation
import Security

enum SigningIdentity {
  static func currentTeamIdentifier() throws -> String {
    var staticCode: SecCode?
    guard SecCodeCopySelf([], &staticCode) == errSecSuccess, let staticCode else {
      throw HelperError.signingIdentityUnavailable
    }

    var staticSigningCode: SecStaticCode?
    guard SecCodeCopyStaticCode(staticCode, [], &staticSigningCode) == errSecSuccess,
      let staticSigningCode
    else {
      throw HelperError.signingIdentityUnavailable
    }

    var signingInformation: CFDictionary?
    guard
      SecCodeCopySigningInformation(staticSigningCode, [], &signingInformation) == errSecSuccess,
      let information = signingInformation as? [CFString: Any],
      let teamIdentifier = information[kSecCodeInfoTeamIdentifier] as? String,
      !teamIdentifier.isEmpty
    else {
      throw HelperError.signingIdentityUnavailable
    }

    return teamIdentifier
  }

  static func appRequirement(teamIdentifier: String) -> String {
    let escapedTeamIdentifier = teamIdentifier.replacingOccurrences(of: "\"", with: "\\\"")
    return
      "identifier \"com.fabdev.desktop\" and anchor apple generic and certificate leaf[subject.OU] = \"\(escapedTeamIdentifier)\""
  }
}
