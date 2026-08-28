import Dispatch
import Foundation
import XPC

final class XPCService {
  static let machServiceName = "com.fabdev.system-helper"

  private let serviceController: SystemServiceController
  private let queue = DispatchQueue(label: "com.fabdev.system-helper.xpc")
  private var listener: xpc_connection_t?

  init(serviceController: SystemServiceController) {
    self.serviceController = serviceController
  }

  func run() throws -> Never {
    let teamIdentifier = try SigningIdentity.currentTeamIdentifier()
    let requirement = SigningIdentity.appRequirement(teamIdentifier: teamIdentifier)
    let listener = xpc_connection_create_mach_service(
      Self.machServiceName,
      queue,
      UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER)
    )

    let result = xpc_connection_set_peer_code_signing_requirement(listener, requirement)
    guard result == 0 else {
      throw HelperError.xpcSetupFailed(result)
    }

    xpc_connection_set_event_handler(listener) { [weak self] event in
      guard xpc_get_type(event) == XPC_TYPE_CONNECTION else {
        return
      }
      self?.accept(event)
    }
    xpc_connection_activate(listener)
    self.listener = listener
    dispatchMain()
  }

  private func accept(_ peer: xpc_connection_t) {
    xpc_connection_set_event_handler(peer) { [weak self, weak peer] event in
      guard let self, let peer, xpc_get_type(event) == XPC_TYPE_DICTIONARY else {
        return
      }
      self.handle(event, from: peer)
    }
    xpc_connection_activate(peer)
  }

  private func handle(_ message: xpc_object_t, from peer: xpc_connection_t) {
    guard let commandPointer = xpc_dictionary_get_string(message, "command") else {
      reply(to: message, from: peer, success: false, state: "invalid-command")
      return
    }

    let command = String(cString: commandPointer)
    do {
      switch command {
      case "start":
        try serviceController.start()
      case "stop":
        try serviceController.stop()
      case "status":
        break
      default:
        reply(to: message, from: peer, success: false, state: "invalid-command")
        return
      }

      reply(
        to: message,
        from: peer,
        success: true,
        state: try serviceController.isRunning() ? "running" : "stopped"
      )
    } catch {
      reply(to: message, from: peer, success: false, state: error.localizedDescription)
    }
  }

  private func reply(
    to message: xpc_object_t,
    from peer: xpc_connection_t,
    success: Bool,
    state: String
  ) {
    guard let reply = xpc_dictionary_create_reply(message) else {
      return
    }
    xpc_dictionary_set_bool(reply, "success", success)
    xpc_dictionary_set_string(reply, "state", state)
    xpc_connection_send_message(peer, reply)
  }
}
