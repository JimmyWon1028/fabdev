import Foundation
import Network
import XCTest

@testable import SystemHelper

final class UDPProxyTests: XCTestCase {
  func testRepeatedDatagramsOnOneClientConnection() throws {
    let queue = DispatchQueue(label: "com.fabdev.helper.udp-proxy-test")
    let backend = try NWListener(using: .udp, on: .any)
    var backendConnections: [NWConnection] = []
    backend.newConnectionHandler = { connection in
      backendConnections.append(connection)
      connection.stateUpdateHandler = { state in
        if case .ready = state {
          self.receiveAndEcho(on: connection)
        }
      }
      connection.start(queue: queue)
    }
    let backendReady = expectation(description: "UDP backend ready")
    backend.stateUpdateHandler = { state in
      if case .ready = state {
        backendReady.fulfill()
      }
    }
    backend.start(queue: queue)
    defer {
      backendConnections.forEach { $0.cancel() }
      backend.cancel()
    }
    wait(for: [backendReady], timeout: 3)

    let backendPort = try XCTUnwrap(backend.port?.rawValue)
    let proxy = try UDPProxy(listenPort: 0, backendPort: backendPort, queue: queue)
    let proxyReady = expectation(description: "UDP proxy ready")
    proxy.start { result in
      if case .failure(let error) = result {
        XCTFail("UDP proxy startup failed: \(error)")
      }
      proxyReady.fulfill()
    }
    defer { proxy.cancel() }
    wait(for: [proxyReady], timeout: 3)

    let proxyPort = try XCTUnwrap(proxy.listeningPort)
    let client = NWConnection(
      host: "127.0.0.1",
      port: try XCTUnwrap(NWEndpoint.Port(rawValue: proxyPort)),
      using: .udp
    )
    let clientReady = expectation(description: "UDP client ready")
    client.stateUpdateHandler = { state in
      if case .ready = state {
        clientReady.fulfill()
      }
    }
    client.start(queue: queue)
    defer { client.cancel() }
    wait(for: [clientReady], timeout: 3)

    for sequence in 1...5 {
      let packet = Data([UInt8(sequence), 0x42])
      let reply = expectation(description: "UDP reply \(sequence)")
      client.receiveMessage { response, _, _, error in
        XCTAssertNil(error)
        XCTAssertEqual(response, packet)
        reply.fulfill()
      }
      client.send(
        content: packet,
        completion: .contentProcessed { error in
          XCTAssertNil(error)
        })
      wait(for: [reply], timeout: 2)
    }

    client.send(
      content: Data([0]),
      completion: .contentProcessed { error in
        XCTAssertNil(error)
      })
    let backendTimeout = expectation(description: "Dropped backend request expires")
    queue.asyncAfter(deadline: .now() + 3.5) {
      backendTimeout.fulfill()
    }
    wait(for: [backendTimeout], timeout: 4)

    let recoveredPacket = Data([6, 0x42])
    let recoveredReply = expectation(description: "UDP reply after backend timeout")
    client.receiveMessage { response, _, _, error in
      XCTAssertNil(error)
      XCTAssertEqual(response, recoveredPacket)
      recoveredReply.fulfill()
    }
    client.send(
      content: recoveredPacket,
      completion: .contentProcessed { error in
        XCTAssertNil(error)
      })
    wait(for: [recoveredReply], timeout: 2)

    XCTAssertEqual(activeSessionCount(of: proxy, on: queue), 1)
    client.cancel()
    let idleCleanup = expectation(description: "Closed UDP client session is released")
    queue.asyncAfter(deadline: .now() + 31) {
      idleCleanup.fulfill()
    }
    wait(for: [idleCleanup], timeout: 32)
    XCTAssertEqual(activeSessionCount(of: proxy, on: queue), 0)
  }

  private func activeSessionCount(of proxy: UDPProxy, on queue: DispatchQueue) -> Int? {
    queue.sync {
      guard
        let sessions = Mirror(reflecting: proxy).children.first(where: { $0.label == "sessions" })
      else {
        return nil
      }
      return Mirror(reflecting: sessions.value).children.count
    }
  }

  private func receiveAndEcho(on connection: NWConnection) {
    connection.receiveMessage { [weak self] request, _, _, error in
      guard error == nil, let request else {
        return
      }
      if request != Data([0]) {
        connection.send(content: request, completion: .contentProcessed { _ in })
      }
      self?.receiveAndEcho(on: connection)
    }
  }
}
