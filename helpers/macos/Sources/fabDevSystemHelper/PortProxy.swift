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

final class UDPProxy {
  private let listener: NWListener
  private let backendPort: NWEndpoint.Port
  private let queue: DispatchQueue
  private var sessions: [UUID: UDPClientSession] = [:]

  var listeningPort: UInt16? {
    listener.port?.rawValue
  }

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
    queue.async { [self] in
      for session in Array(sessions.values) {
        session.cancel()
      }
      sessions.removeAll()
    }
  }

  private func accept(_ client: NWConnection) {
    let id = UUID()
    let session = UDPClientSession(
      client: client,
      backendPort: backendPort,
      queue: queue
    ) { [weak self] in
      self?.sessions.removeValue(forKey: id)
    }
    sessions[id] = session
    session.start()
  }
}

private final class UDPClientSession {
  private let client: NWConnection
  private let backendPort: NWEndpoint.Port
  private let queue: DispatchQueue
  private let onClose: () -> Void
  private var exchanges: [UUID: UDPBackendExchange] = [:]
  private var idleTimeout: DispatchWorkItem?
  private var isClosed = false

  init(
    client: NWConnection,
    backendPort: NWEndpoint.Port,
    queue: DispatchQueue,
    onClose: @escaping () -> Void
  ) {
    self.client = client
    self.backendPort = backendPort
    self.queue = queue
    self.onClose = onClose
  }

  func start() {
    client.stateUpdateHandler = { [weak self] state in
      guard let self else {
        return
      }
      switch state {
      case .ready:
        self.scheduleIdleTimeout()
        self.receiveRequest()
      case .failed, .cancelled:
        self.cancel()
      default:
        break
      }
    }
    client.start(queue: queue)
  }

  func cancel() {
    guard !isClosed else {
      return
    }
    isClosed = true
    idleTimeout?.cancel()
    for exchange in Array(exchanges.values) {
      exchange.cancel()
    }
    exchanges.removeAll()
    client.cancel()
    onClose()
  }

  private func receiveRequest() {
    client.receiveMessage { [weak self] data, _, _, error in
      guard let self, !self.isClosed else {
        return
      }
      if error != nil {
        self.cancel()
        return
      }

      self.receiveRequest()
      guard let data, !data.isEmpty else {
        return
      }

      self.scheduleIdleTimeout()
      self.forward(data)
    }
  }

  private func scheduleIdleTimeout() {
    idleTimeout?.cancel()
    let idleTimeout = DispatchWorkItem { [weak self] in
      self?.cancel()
    }
    self.idleTimeout = idleTimeout
    queue.asyncAfter(deadline: .now() + 30, execute: idleTimeout)
  }

  private func forward(_ request: Data) {
    let id = UUID()
    let exchange = UDPBackendExchange(
      request: request,
      backendPort: backendPort,
      queue: queue
    ) { [weak self] response in
      guard let self, !self.isClosed else {
        return
      }
      self.exchanges.removeValue(forKey: id)
      guard let response else {
        return
      }

      self.client.send(
        content: response,
        completion: .contentProcessed { [weak self] error in
          if error != nil {
            self?.cancel()
          }
        })
    }
    exchanges[id] = exchange
    exchange.start()
  }
}

private final class UDPBackendExchange {
  private let request: Data
  private let backend: NWConnection
  private let queue: DispatchQueue
  private let onComplete: (Data?) -> Void
  private var timeout: DispatchWorkItem?
  private var isClosed = false

  init(
    request: Data,
    backendPort: NWEndpoint.Port,
    queue: DispatchQueue,
    onComplete: @escaping (Data?) -> Void
  ) {
    self.request = request
    backend = NWConnection(host: "127.0.0.1", port: backendPort, using: .udp)
    self.queue = queue
    self.onComplete = onComplete
  }

  func start() {
    let timeout = DispatchWorkItem { [weak self] in
      self?.finish(response: nil, failure: "backend timeout")
    }
    self.timeout = timeout
    queue.asyncAfter(deadline: .now() + 3, execute: timeout)

    backend.stateUpdateHandler = { [weak self] state in
      guard let self, !self.isClosed else {
        return
      }
      switch state {
      case .ready:
        self.backend.send(
          content: self.request,
          completion: .contentProcessed { [weak self] error in
            guard let self else {
              return
            }
            if let error {
              self.finish(response: nil, failure: "backend send: \(error)")
              return
            }

            self.backend.receiveMessage { [weak self] response, _, _, error in
              self?.finish(
                response: response,
                failure: error.map { "backend receive: \($0)" }
                  ?? (response == nil ? "backend returned no response" : nil)
              )
            }
          })
      case .failed(let error):
        self.finish(response: nil, failure: "backend connection: \(error)")
      case .cancelled:
        self.finish(response: nil, failure: "backend cancelled")
      default:
        break
      }
    }
    backend.start(queue: queue)
  }

  func cancel() {
    guard !isClosed else {
      return
    }
    isClosed = true
    timeout?.cancel()
    backend.cancel()
  }

  private func finish(response: Data?, failure: String?) {
    guard !isClosed else {
      return
    }
    if let failure {
      FileHandle.standardError.write(Data("fabdev-system-helper: DNS UDP \(failure)\n".utf8))
    }
    cancel()
    onComplete(response)
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
