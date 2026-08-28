use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::mem::zeroed;
use std::net::SocketAddr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use fabdev_connect::{
  decode_settings, encode_settings, parse_domains, update_hosts_contents, ClientProxy,
  ConnectSettings,
};
use windows_sys::Win32::Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{UpdateWindow, COLOR_WINDOW};
use windows_sys::Win32::Storage::FileSystem::{
  MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WINDOW_CLASS: &str = "fabDevConnectWindow";
const ID_SERVER: i32 = 101;
const ID_DOMAIN: i32 = 102;
const ID_CONNECT: i32 = 103;
const ID_DISCONNECT: i32 = 104;
const ID_OPEN: i32 = 105;
const ID_STATUS: i32 = 106;

#[derive(Default)]
struct State {
  proxy: Option<ClientProxy>,
  domains: Vec<String>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

pub fn run() -> Result<()> {
  if !std::env::args().any(|argument| argument == "--elevated") {
    elevate()?;
    return Ok(());
  }
  STATE.set(Mutex::new(State::default())).ok();
  unsafe { run_window() }
}

fn elevate() -> Result<()> {
  let source = std::env::current_exe().context("無法取得 fabdev-connect.exe 路徑")?;
  let executable = stage_elevation_executable(&source)?;
  let verb = wide("runas");
  let executable = wide(executable.as_os_str());
  let parameters = wide("--elevated");
  let result = unsafe {
    ShellExecuteW(
      null_mut(),
      verb.as_ptr(),
      executable.as_ptr(),
      parameters.as_ptr(),
      null(),
      SW_SHOWNORMAL,
    )
  };
  if result as usize <= 32 {
    bail!("UAC 管理員授權被拒絕或啟動失敗");
  }
  Ok(())
}

fn stage_elevation_executable(source: &Path) -> Result<PathBuf> {
  let directory = local_app_dir()?;
  std::fs::create_dir_all(&directory)
    .with_context(|| format!("無法建立本機執行目錄：{}", directory.display()))?;
  let target = directory.join("fabdev-connect-runtime.exe");
  std::fs::copy(source, &target).with_context(|| {
    format!(
      "無法將 fabDev Connect 複製到 Windows 本機路徑：{}",
      target.display()
    )
  })?;
  Ok(target)
}

unsafe fn run_window() -> Result<()> {
  let instance = GetModuleHandleW(null());
  if instance.is_null() {
    bail!("無法取得 Windows Module Handle: {}", GetLastError());
  }
  let class_name = wide(WINDOW_CLASS);
  let title = wide("fabDev Connect");
  let window_class = WNDCLASSW {
    lpfnWndProc: Some(window_proc),
    hInstance: instance,
    lpszClassName: class_name.as_ptr(),
    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
    hbrBackground: (COLOR_WINDOW + 1) as usize as _,
    ..zeroed()
  };
  if RegisterClassW(&window_class) == 0 {
    bail!("無法註冊 fabDev Connect 視窗: {}", GetLastError());
  }
  let window = CreateWindowExW(
    0,
    class_name.as_ptr(),
    title.as_ptr(),
    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
    CW_USEDEFAULT,
    CW_USEDEFAULT,
    520,
    280,
    null_mut(),
    null_mut(),
    instance,
    null_mut(),
  );
  if window.is_null() {
    bail!("無法建立 fabDev Connect 視窗: {}", GetLastError());
  }
  ShowWindow(window, SW_SHOW);
  UpdateWindow(window);
  let mut message: MSG = zeroed();
  while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
    TranslateMessage(&message);
    DispatchMessageW(&message);
  }
  Ok(())
}

unsafe extern "system" fn window_proc(
  window: HWND,
  message: u32,
  wparam: WPARAM,
  lparam: LPARAM,
) -> LRESULT {
  match message {
    WM_CREATE => {
      create_controls(window);
      0
    }
    WM_COMMAND => {
      let control_id = (wparam & 0xffff) as i32;
      match control_id {
        ID_CONNECT => connect(window),
        ID_DISCONNECT => disconnect(window, true),
        ID_OPEN => open_browser(window),
        _ => {}
      }
      0
    }
    WM_CLOSE => {
      let _ = save_control_settings(window);
      disconnect(window, false);
      DestroyWindow(window);
      0
    }
    WM_DESTROY => {
      PostQuitMessage(0);
      0
    }
    _ => DefWindowProcW(window, message, wparam, lparam),
  }
}

unsafe fn create_controls(window: HWND) {
  let settings = load_connect_settings();
  create_control(
    window,
    "STATIC",
    "fabDev 主機（IP:Port）",
    24,
    24,
    190,
    24,
    0,
    0,
  );
  create_control(
    window,
    "EDIT",
    &settings.server,
    220,
    20,
    260,
    28,
    ID_SERVER,
    WS_BORDER | ES_AUTOHSCROLL as u32,
  );
  create_control(
    window,
    "STATIC",
    "Sites（空白或逗號分隔）",
    24,
    68,
    190,
    24,
    0,
    0,
  );
  create_control(
    window,
    "EDIT",
    &settings.domains,
    220,
    64,
    260,
    28,
    ID_DOMAIN,
    WS_BORDER | ES_AUTOHSCROLL as u32,
  );
  create_control(
    window,
    "BUTTON",
    "連線",
    24,
    116,
    120,
    34,
    ID_CONNECT,
    BS_PUSHBUTTON as u32,
  );
  create_control(
    window,
    "BUTTON",
    "中斷",
    154,
    116,
    120,
    34,
    ID_DISCONNECT,
    BS_PUSHBUTTON as u32,
  );
  create_control(
    window,
    "BUTTON",
    "開啟網站",
    284,
    116,
    120,
    34,
    ID_OPEN,
    BS_PUSHBUTTON as u32,
  );
  create_control(window, "STATIC", "尚未連線", 24, 174, 456, 42, ID_STATUS, 0);
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_control(
  parent: HWND,
  class_name: &str,
  text: &str,
  x: i32,
  y: i32,
  width: i32,
  height: i32,
  id: i32,
  style: u32,
) -> HWND {
  let class_name = wide(class_name);
  let text = wide(text);
  CreateWindowExW(
    0,
    class_name.as_ptr(),
    text.as_ptr(),
    WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
    x,
    y,
    width,
    height,
    parent,
    id as usize as _,
    GetModuleHandleW(null()),
    null_mut(),
  )
}

unsafe fn connect(window: HWND) {
  let result = (|| -> Result<Vec<String>> {
    let settings = control_settings(window);
    save_connect_settings(&settings)?;
    let remote: SocketAddr = settings
      .server
      .parse()
      .context("fabDev 主機必須使用 IP:Port，例如 192.168.1.10:18080")?;
    let domains = parse_domains(&settings.domains)?;
    let mut state = STATE.get().unwrap().lock().unwrap();
    if state.proxy.is_some() {
      bail!("fabDev Connect 已經連線");
    }
    let proxy = ClientProxy::start(remote)?;
    if let Err(error) = write_managed_hosts(Some(&domains)) {
      drop(proxy);
      return Err(error);
    }
    state.proxy = Some(proxy);
    state.domains = domains.clone();
    Ok(domains)
  })();
  match result {
    Ok(domains) => set_status(window, &format!("已連線：{}", domains.join(", "))),
    Err(error) => show_error(window, &format!("連線失敗\n\n{error:#}")),
  }
}

unsafe fn disconnect(window: HWND, notify: bool) {
  let result = {
    let mut state = STATE.get().unwrap().lock().unwrap();
    if let Some(mut proxy) = state.proxy.take() {
      proxy.stop();
    }
    state.domains.clear();
    write_managed_hosts(None)
  };
  match result {
    Ok(()) => {
      set_status(window, "尚未連線");
      if notify {
        show_message(window, "fabDev Connect 已中斷");
      }
    }
    Err(error) if notify => show_error(window, &format!("中斷連線時發生錯誤\n\n{error:#}")),
    Err(_) => {}
  }
}

unsafe fn open_browser(window: HWND) {
  let domain = STATE
    .get()
    .unwrap()
    .lock()
    .unwrap()
    .domains
    .first()
    .cloned();
  let Some(domain) = domain else {
    show_error(window, "請先連線");
    return;
  };
  let verb = wide("open");
  let explorer = wide("explorer.exe");
  let url = wide(format!("http://{domain}"));
  let result = ShellExecuteW(
    window,
    verb.as_ptr(),
    explorer.as_ptr(),
    url.as_ptr(),
    null(),
    SW_SHOWNORMAL,
  );
  if result as usize <= 32 {
    show_error(window, "無法開啟預設瀏覽器");
  }
}

fn hosts_path() -> Result<PathBuf> {
  let system_root = std::env::var_os("SystemRoot").context("SystemRoot 未定義")?;
  Ok(PathBuf::from(system_root).join("System32/drivers/etc/hosts"))
}

fn settings_path() -> Result<PathBuf> {
  Ok(local_app_dir()?.join("fabdev-connect.json"))
}

fn local_app_dir() -> Result<PathBuf> {
  let local_app_data =
    std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA 未定義，無法儲存連線設定")?;
  Ok(PathBuf::from(local_app_data).join("FabDev"))
}

fn load_connect_settings() -> ConnectSettings {
  let Ok(path) = settings_path() else {
    return ConnectSettings::default();
  };
  let Ok(contents) = std::fs::read(path) else {
    return ConnectSettings::default();
  };
  decode_settings(&contents).unwrap_or_default()
}

fn save_connect_settings(settings: &ConnectSettings) -> Result<()> {
  let path = settings_path()?;
  let parent = path.parent().context("無法取得 fabDev Connect 設定目錄")?;
  std::fs::create_dir_all(parent)
    .with_context(|| format!("無法建立設定目錄：{}", parent.display()))?;
  std::fs::write(&path, encode_settings(settings)?)
    .with_context(|| format!("無法儲存連線設定：{}", path.display()))
}

fn write_managed_hosts(domains: Option<&[String]>) -> Result<()> {
  let path = hosts_path()?;
  let existing = std::fs::read_to_string(&path)
    .with_context(|| format!("無法讀取 hosts：{}", path.display()))?;
  let contents = update_hosts_contents(&existing, domains)?;
  replace_file(&path, contents.as_bytes())
}

fn replace_file(path: &Path, contents: &[u8]) -> Result<()> {
  let backup = path.with_file_name("hosts.fabdev-connect.backup");
  std::fs::copy(path, &backup).with_context(|| format!("無法備份 hosts：{}", backup.display()))?;
  let pending = path.with_file_name("hosts.fabdev-connect.pending");
  let mut file = OpenOptions::new()
    .create(true)
    .truncate(true)
    .write(true)
    .open(&pending)?;
  file.write_all(contents)?;
  file.sync_all()?;
  drop(file);
  let pending = wide(pending.as_os_str());
  let target = wide(path.as_os_str());
  let moved = unsafe {
    MoveFileExW(
      pending.as_ptr(),
      target.as_ptr(),
      MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
    )
  };
  if moved == 0 {
    bail!("無法更新 hosts：{}", unsafe { GetLastError() });
  }
  Ok(())
}

unsafe fn control_text(window: HWND, id: i32) -> String {
  let control = GetDlgItem(window, id);
  let length = GetWindowTextLengthW(control);
  let mut buffer = vec![0_u16; length as usize + 1];
  GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
  String::from_utf16_lossy(&buffer[..length as usize])
}

unsafe fn control_settings(window: HWND) -> ConnectSettings {
  ConnectSettings {
    server: control_text(window, ID_SERVER),
    domains: control_text(window, ID_DOMAIN),
  }
}

unsafe fn save_control_settings(window: HWND) -> Result<()> {
  save_connect_settings(&control_settings(window))
}

unsafe fn set_status(window: HWND, value: &str) {
  let value = wide(value);
  SetWindowTextW(GetDlgItem(window, ID_STATUS), value.as_ptr());
}

unsafe fn show_message(window: HWND, value: &str) {
  let value = wide(value);
  let title = wide("fabDev Connect");
  MessageBoxW(
    window,
    value.as_ptr(),
    title.as_ptr(),
    MB_OK | MB_ICONINFORMATION,
  );
}

unsafe fn show_error(window: HWND, value: &str) {
  let value = wide(value);
  let title = wide("fabDev Connect");
  MessageBoxW(window, value.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
  value
    .as_ref()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect()
}
