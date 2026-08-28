import Foundation
import XCTest

@testable import SystemHelper

final class ResolverManagerTests: XCTestCase {
  private var temporaryDirectory: URL!

  override func setUpWithError() throws {
    temporaryDirectory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(
      at: temporaryDirectory,
      withIntermediateDirectories: false
    )
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: temporaryDirectory)
  }

  func testInstallAndRemoveManagedResolver() throws {
    let resolverDirectory = temporaryDirectory.appendingPathComponent("resolver")
    let manager = ResolverManager(directoryPath: resolverDirectory.path)

    try manager.install()

    XCTAssertTrue(try manager.isInstalled())
    let content = try String(
      contentsOf: resolverDirectory.appendingPathComponent("test"),
      encoding: .utf8
    )
    XCTAssertTrue(content.contains("# Managed by fabDev"))
    XCTAssertTrue(content.contains("nameserver 127.0.0.1"))

    try manager.remove()
    XCTAssertFalse(try manager.isInstalled())
  }

  func testInstallDoesNotReplaceForeignResolver() throws {
    let resolverDirectory = temporaryDirectory.appendingPathComponent("resolver")
    try FileManager.default.createDirectory(
      at: resolverDirectory,
      withIntermediateDirectories: true
    )
    let resolverFile = resolverDirectory.appendingPathComponent("test")
    try Data("nameserver 10.0.0.1\n".utf8).write(to: resolverFile)
    let manager = ResolverManager(directoryPath: resolverDirectory.path)

    XCTAssertThrowsError(try manager.install())
    XCTAssertEqual(
      try String(contentsOf: resolverFile, encoding: .utf8),
      "nameserver 10.0.0.1\n"
    )
  }

  func testRemoveDoesNotFollowSymbolicLink() throws {
    let resolverDirectory = temporaryDirectory.appendingPathComponent("resolver")
    try FileManager.default.createDirectory(
      at: resolverDirectory,
      withIntermediateDirectories: true
    )
    let foreignFile = temporaryDirectory.appendingPathComponent("foreign")
    try Data("keep\n".utf8).write(to: foreignFile)
    try FileManager.default.createSymbolicLink(
      at: resolverDirectory.appendingPathComponent("test"),
      withDestinationURL: foreignFile
    )
    let manager = ResolverManager(directoryPath: resolverDirectory.path)

    XCTAssertThrowsError(try manager.remove())
    XCTAssertEqual(
      try String(contentsOf: foreignFile, encoding: .utf8),
      "keep\n"
    )
  }

  func testLocalTestAcceptsCompatibleExistingResolverWithoutTakingOwnership() throws {
    let resolverDirectory = temporaryDirectory.appendingPathComponent("resolver")
    try FileManager.default.createDirectory(
      at: resolverDirectory,
      withIntermediateDirectories: true
    )
    let resolverFile = resolverDirectory.appendingPathComponent("test")
    let existingContent = "nameserver 127.0.0.1\n"
    try Data(existingContent.utf8).write(to: resolverFile)
    let manager = ResolverManager(directoryPath: resolverDirectory.path)

    XCTAssertFalse(try manager.prepareForLocalTest())
    try manager.removeManagedIfPresent()
    XCTAssertEqual(
      try String(contentsOf: resolverFile, encoding: .utf8),
      existingContent
    )
  }

  func testLocalTestRejectsResolverUsingAnotherPort() {
    let content = Data("nameserver 127.0.0.1\nport 15353\n".utf8)

    XCTAssertFalse(ResolverManager.isCompatibleResolverContent(content))
  }
}
