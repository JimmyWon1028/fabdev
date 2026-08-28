import Darwin
import Foundation

final class ResolverManager {
  static let production = ResolverManager(directoryPath: "/etc/resolver")

  private static let managedContent = Data(
    """
    # Managed by fabDev
    domain test
    nameserver 127.0.0.1
    port 53

    """.utf8
  )

  private let directoryPath: String
  private var resolverPath: String {
    "\(directoryPath)/test"
  }

  init(directoryPath: String) {
    self.directoryPath = directoryPath
  }

  func install() throws {
    try ensureDirectory()

    if pathExists(resolverPath) {
      guard try isRegularFile(resolverPath),
        try readFile(resolverPath) == Self.managedContent
      else {
        throw HelperError.resolverConflict(resolverPath)
      }
      return
    }

    try writeManagedFile()
  }

  @discardableResult
  func prepareForLocalTest() throws -> Bool {
    try ensureDirectory()

    guard pathExists(resolverPath) else {
      try writeManagedFile()
      return true
    }
    guard try isRegularFile(resolverPath) else {
      throw HelperError.resolverConflict(resolverPath)
    }

    let content = try readFile(resolverPath)
    guard content == Self.managedContent || Self.isCompatibleResolverContent(content) else {
      throw HelperError.resolverConflict(resolverPath)
    }
    return false
  }

  func remove() throws {
    guard pathExists(resolverPath) else {
      return
    }
    guard try isRegularFile(resolverPath),
      try readFile(resolverPath) == Self.managedContent
    else {
      throw HelperError.resolverConflict(resolverPath)
    }

    guard unlink(resolverPath) == 0 else {
      throw posixError("Unable to remove \(resolverPath)")
    }
  }

  func isInstalled() throws -> Bool {
    guard pathExists(resolverPath) else {
      return false
    }
    return try isRegularFile(resolverPath)
      && readFile(resolverPath) == Self.managedContent
  }

  func removeManagedIfPresent() throws {
    guard pathExists(resolverPath) else {
      return
    }
    guard try isRegularFile(resolverPath) else {
      throw HelperError.resolverConflict(resolverPath)
    }
    guard try readFile(resolverPath) == Self.managedContent else {
      return
    }
    guard unlink(resolverPath) == 0 else {
      throw posixError("Unable to remove \(resolverPath)")
    }
  }

  static func isCompatibleResolverContent(_ content: Data) -> Bool {
    guard let value = String(data: content, encoding: .utf8) else {
      return false
    }

    var hasNameserver = false
    for rawLine in value.split(whereSeparator: \.isNewline) {
      let line =
        rawLine
        .split(separator: "#", maxSplits: 1)
        .first?
        .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
      guard !line.isEmpty else {
        continue
      }

      let fields = line.split(whereSeparator: \.isWhitespace).map(String.init)
      guard fields.count == 2 else {
        return false
      }
      switch fields[0] {
      case "nameserver":
        guard fields[1] == "127.0.0.1" else {
          return false
        }
        hasNameserver = true
      case "domain":
        guard fields[1] == "test" else {
          return false
        }
      case "port":
        guard fields[1] == "53" else {
          return false
        }
      default:
        return false
      }
    }
    return hasNameserver
  }

  private func ensureDirectory() throws {
    if pathExists(directoryPath) {
      var info = stat()
      guard lstat(directoryPath, &info) == 0,
        info.st_mode & S_IFMT == S_IFDIR
      else {
        throw HelperError.resolverConflict(directoryPath)
      }
      return
    }

    guard mkdir(directoryPath, 0o755) == 0 else {
      throw posixError("Unable to create \(directoryPath)")
    }
  }

  private func writeManagedFile() throws {
    var template = Array("\(directoryPath)/.fabdev-test.XXXXXX".utf8CString)
    let descriptor = mkstemp(&template)
    guard descriptor >= 0 else {
      throw posixError("Unable to create resolver file")
    }

    let temporaryPath = String(cString: template)
    var shouldRemoveTemporaryFile = true
    defer {
      close(descriptor)
      if shouldRemoveTemporaryFile {
        unlink(temporaryPath)
      }
    }

    try write(Self.managedContent, to: descriptor)
    guard fchmod(descriptor, 0o644) == 0 else {
      throw posixError("Unable to set resolver file permissions")
    }
    guard fsync(descriptor) == 0 else {
      throw posixError("Unable to sync resolver file")
    }
    guard rename(temporaryPath, resolverPath) == 0 else {
      throw posixError("Unable to install \(resolverPath)")
    }
    shouldRemoveTemporaryFile = false
  }

  private func write(_ data: Data, to descriptor: Int32) throws {
    try data.withUnsafeBytes { buffer in
      var offset = 0
      while offset < buffer.count {
        let result = Darwin.write(
          descriptor,
          buffer.baseAddress?.advanced(by: offset),
          buffer.count - offset
        )
        guard result > 0 else {
          throw posixError("Unable to write resolver file")
        }
        offset += result
      }
    }
  }

  private func readFile(_ path: String) throws -> Data {
    let descriptor = open(path, O_RDONLY | O_NOFOLLOW)
    guard descriptor >= 0 else {
      throw posixError("Unable to open \(path)")
    }
    defer { close(descriptor) }

    var result = Data()
    var buffer = [UInt8](repeating: 0, count: 4_096)
    while true {
      let count = Darwin.read(descriptor, &buffer, buffer.count)
      guard count >= 0 else {
        throw posixError("Unable to read \(path)")
      }
      if count == 0 {
        return result
      }
      result.append(buffer, count: count)
    }
  }

  private func pathExists(_ path: String) -> Bool {
    var info = stat()
    if lstat(path, &info) == 0 {
      return true
    }
    return errno != ENOENT
  }

  private func isRegularFile(_ path: String) throws -> Bool {
    var info = stat()
    guard lstat(path, &info) == 0 else {
      throw posixError("Unable to inspect \(path)")
    }
    return info.st_mode & S_IFMT == S_IFREG
  }

  private func posixError(_ context: String) -> HelperError {
    let message = String(cString: strerror(errno))
    return .filesystemOperation("\(context): \(message)")
  }
}
