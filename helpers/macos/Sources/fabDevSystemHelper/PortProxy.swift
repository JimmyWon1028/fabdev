import Foundation
import Network

final class PortProxyController {
  private let configuration: ProxyConfiguration
  private let queue = DispatchQueue(label: "com.fabdev.system-helper.proxy")
  private var dnsProxy: UDPProxy?
  private var httpProxy: TCPProxy?
  private var httpsProxy: TCPProxy?

  private(set) var isRunning = false

  init(configuration: ProxyConfiguration) {
    self.configuration = configuration
  }

  func start() throws {
    guard !isRunning else {
      return
    }

    try configuration.validate()
    let dnsProxy = try UDPProxy(
      listenPort: configuration.dnsListenPort,
      backendPort: configuration.dnsBackendPort,
      queue: queue
    )
    let httpProxy = try TCPProxy(
      listenPort: configuration.httpListenPort,
      backendPort: configuration.httpBackendPort,
      queue: queue
    )
    let httpsProxy = try TCPProxy(
      listenPort: configuration.httpsListenPort,
      backendPort: configuration.httpsBackendPort,
      queue: queue
    )

    let startupGroup = DispatchGroup()
    let startupResult = ListenerStartupResult()

    startupGroup.enter()
    dnsProxy.start { result in
      startupResult.record(result)
      startupGroup.leave()
    }
    startupGroup.enter()
    httpProxy.start { result in
      startupResult.record(result)
      startupGroup.leave()
    }
    startupGroup.enter()
    httpsProxy.start { result in
      startupResult.record(result)
      startupGroup.leave()
    }

    guard startupGroup.wait(timeout: .now() + 5) == .success else {
      dnsProxy.cancel()
      httpProxy.cancel()
      httpsProxy.cancel()
      throw HelperError.listenerFailed("Timed out while starting proxy listeners")
    }
    if let error = startupResult.currentError() {
      dnsProxy.cancel()
      httpProxy.cancel()
      httpsProxy.cancel()
      throw error
    }

    self.dnsProxy = dnsProxy
    self.httpProxy = httpProxy
    self.httpsProxy = httpsProxy
    isRunning = true
  }

  func stop() {
    dnsProxy?.cancel()
    httpProxy?.cancel()
    httpsProxy?.cancel()
    dnsProxy = nil
    httpProxy = nil
    httpsProxy = nil
    isRunning = false
  }
}

private final class TCPProxy {
  private let listener: NWListener
  private let backendPort: NWEndpoint.Port
  private let queue: DispatchQueue

  init(listenPort: UInt16, backendPort: UInt16, queue: DispatchQueue) throws {
    guard let listenPort = NWEndpoint.Port(rawValue: listenPort),
      let backendPort = NWEndpoint.Port(rawValue: backendPort)
    else {
      throw HelperError.invalidConfiguration("Invalid TCP proxy port")
    }

    let parameters = NWParameters.tcp
    parameters.requiredLocalEndpoint = .hostPort(
      host: "127.0.0.1",
      port: listenPort
    )
    listener = try NWListener(using: parameters)
    self.backendPort = backendPort
    self.queue = queue
    listener.newConnectionHandler = { [weak self] inbound in
      self?.accept(inbound)
    }
  }

  func start(completion: @escaping (Result<Void, Error>) -> Void) {
    let startup = ListenerStartup(completion: completion)
    listener.stateUpdateHandler = { state in
      startup.handle(state)
    }
    listener.start(queue: queue)
  }

  func cancel() {
    listener.cancel()
  }

  private func accept(_ inbound: NWConnection) {
    let outbound = NWConnection(host: "127.0.0.1", port: backendPort, using: .tcp)
    let pair = ConnectionPair(inbound: inbound, outbound: outbound)
    pair.start(on: queue)
  }
}

private final class ConnectionPair {
  private let inbound: NWConnection
  private let outbound: NWConnection
  private var retainedSelf: ConnectionPair?

  init(inbound: NWConnection, outbound: NWConnection) {
    self.inbound = inbound
    self.outbound = outbound
  }

  func start(on queue: DispatchQueue) {
    retainedSelf = self
    inbound.stateUpdateHandler = { [weak self] state in
      self?.handle(state)
    }
    outbound.stateUpdateHandler = { [weak self] state in
      guard case .ready = state, let self else {
        self?.handle(state)
        return
      }

      self.pump(from: self.inbound, to: self.outbound)
      self.pump(from: self.outbound, to: self.inbound)
    }
    inbound.start(queue: queue)
    outbound.start(queue: queue)
  }

  private func pump(from source: NWConnection, to destination: NWConnection) {
    source.receive(minimumIncompleteLength: 1, maximumLength: 65_536) {
      [weak self] data, _, complete, error in
      guard let self else {
        return
      }

      if let data, !data.isEmpty {
        destination.send(
          content: data,
          completion: .contentProcessed { [weak self] sendError in
            if sendError == nil, !complete {
              self?.pump(from: source, to: destination)
            } else {
              self?.cancel()
            }
          })
      } else if complete || error != nil {
        cancel()
      } else {
        pump(from: source, to: destination)
      }
    }
  }

  private func handle(_ state: NWConnection.State) {
    if case .failed = state {
      cancel()
    } else if case .cancelled = state {
      cancel()
    }
  }

  private func cancel() {
    inbound.cancel()
    outbound.cancel()
    retainedSelf = nil
  }
}

private final class UDPProxy {
  private let listener: NWListener
  private let backendPort: NWEndpoint.Port
  private let queue: DispatchQueue

  init(listenPort: UInt16, backendPort: UInt16, queue: DispatchQueue) throws {
    guard let listenPort = NWEndpoint.Port(rawValue: listenPort),
      let backendPort = NWEndpoint.Port(rawValue: backendPort)
    else {
      throw HelperError.invalidConfiguration("Invalid UDP proxy port")
    }

    let parameters = NWParameters.udp
    parameters.requiredLocalEndpoint = .hostPort(
      host: "127.0.0.1",
      port: listenPort
    )
    listener = try NWListener(using: parameters)
    self.backendPort = backendPort
    self.queue = queue
    listener.newConnectionHandler = { [weak self] client in
      self?.accept(client)
    }
  }

  func start(completion: @escaping (Result<Void, Error>) -> Void) {
    let startup = ListenerStartup(completion: completion)
    listener.stateUpdateHandler = { state in
      startup.handle(state)
    }
    listener.start(queue: queue)
  }

  func cancel() {
    listener.cancel()
  }

  private func accept(_ client: NWConnection) {
    client.stateUpdateHandler = { [weak self, weak client] state in
      guard case .ready = state, let self, let client else {
        return
      }
      self.receiveRequest(from: client)
    }
    client.start(queue: queue)
  }

  private func receiveRequest(from client: NWConnection) {
    client.receiveMessage { [weak self, weak client] data, _, _, error in
      guard let self, let client else {
        return
      }
      guard let data, !data.isEmpty, error == nil else {
        client.cancel()
        return
      }

      self.forward(data, to: client)
    }
  }

  private func forward(_ request: Data, to client: NWConnection) {
    let backend = NWConnection(host: "127.0.0.1", port: backendPort, using: .udp)
    backend.stateUpdateHandler = { [weak backend, weak client] state in
      guard case .ready = state, let backend, let client else {
        return
      }

      backend.send(
        content: request,
        completion: .contentProcessed { sendError in
          guard sendError == nil else {
            backend.cancel()
            return
          }

          backend.receiveMessage { [weak client] response, _, _, _ in
            backend.cancel()
            guard let client, let response else {
              return
            }

            client.send(
              content: response,
              completion: .contentProcessed { _ in
                // Each DNS datagram uses a short-lived proxy flow. Closing it after the
                // response prevents abandoned health-check clients from accumulating.
                client.cancel()
              })
          }
        })
    }
    backend.start(queue: queue)
  }
}

private final class ListenerStartup {
  private let lock = NSLock()
  private let completion: (Result<Void, Error>) -> Void
  private var isComplete = false

  init(completion: @escaping (Result<Void, Error>) -> Void) {
    self.completion = completion
  }

  func handle(_ state: NWListener.State) {
    let result: Result<Void, Error>
    switch state {
    case .ready:
      result = .success(())
    case .failed(let error):
      result = .failure(HelperError.listenerFailed(error.localizedDescription))
    default:
      return
    }

    lock.lock()
    guard !isComplete else {
      lock.unlock()
      return
    }
    isComplete = true
    lock.unlock()
    completion(result)
  }
}

private final class ListenerStartupResult {
  private let lock = NSLock()
  private var error: Error?

  func record(_ result: Result<Void, Error>) {
    guard case .failure(let error) = result else {
      return
    }
    lock.lock()
    self.error = self.error ?? error
    lock.unlock()
  }

  func currentError() -> Error? {
    lock.lock()
    defer { lock.unlock() }
    return error
  }
}
