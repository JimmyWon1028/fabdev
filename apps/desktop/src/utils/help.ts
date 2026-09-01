import type { Language } from './preferences'

export interface OperationManualSection {
  id: string
  title: string
  summary: string
  steps?: string[]
  notes?: string[]
}

export interface OperationManual {
  title: string
  description: string
  closeLabel: string
  contentsLabel: string
  shortcutLabel: string
  sections: OperationManualSection[]
}

const manuals: Record<Language, OperationManual> = {
  en: {
    title: 'fabDev User Guide',
    description: 'Local ERP Web development environment · Press F1 anywhere to open this guide.',
    closeLabel: 'Close user guide',
    contentsLabel: 'Contents',
    shortcutLabel: 'F1 User Guide',
    sections: [
      {
        id: 'quick-start',
        title: 'Quick start',
        summary: 'Create a Site, assign PHP, start the Web stack, and open its .test domain.',
        steps: [
          'Confirm that “Agent connected” appears at the bottom of the sidebar.',
          'Open Sites. Use Site Home to scan first-level folders, or add one linked Site manually.',
          'Confirm the domain, Web Root, and PHP Runtime. Laravel projects normally use public as the Web Root.',
          'Return to Overview and select Start All, then open the Site from the Sites page.'
        ],
        notes: [
          'Start All manages DNS, Nginx, and the PHP-FPM instances required by enabled Sites. MariaDB and Proxy connections are managed independently.'
        ]
      },
      {
        id: 'overview',
        title: 'Overview and service status',
        summary: 'Overview shows the current state of DNS, Nginx, PHP, MariaDB, Node.js, and Proxy Manager.',
        steps: [
          'Use Start All or Stop All for the Web stack.',
          'Select Refresh after changing a service outside the App or when checking recovery from an error.',
          'If PHP-FPM reports queued or slow requests, inspect the project and slow log before increasing worker capacity.'
        ]
      },
      {
        id: 'sites',
        title: 'Sites',
        summary: 'Manage project folders, .test domains, Web Roots, PHP versions, HTTPS, and temporary LAN sharing.',
        steps: [
          'Site Home turns each first-level folder or directory symlink into a matching .test Site. Hidden and nested folders are ignored.',
          'Use Add Site for a project outside Site Home. A linked Site takes precedence if its domain conflicts with a scanned Site.',
          'Use Edit to change the name, project path, domain, or Web Root. Select the PHP Runtime directly on the Site row.',
          'Secure enables HTTPS for that Site. Trust the fabDev CA when prompted before expecting browsers to accept the certificate.',
          'Share to LAN exposes selected Sites through fabDev Connect for short, trusted-network testing.',
          'Import and Export transfer Site configuration. Duplicate entries are skipped.'
        ],
        notes: [
          'Removing a Site only removes its fabDev registration. It never deletes the project folder.',
          'LAN sharing has no TLS or user authentication. Do not use it on an untrusted network or as a production server.'
        ]
      },
      {
        id: 'php',
        title: 'PHP settings',
        summary: 'Install optional PHP Runtimes, select a global version, and maintain validated php.ini files.',
        steps: [
          'Available PHP versions come from installed packages and the Runtime Catalog. Install package validates a matching descriptor and archive before installation.',
          'Set as global changes the default PHP series. A Site with an explicit version keeps its own selection.',
          'Open php.ini to edit one installed version. Save and apply validates the file with the matching PHP-FPM first.',
          'Default php.ini is a template for future installs; editing it does not overwrite existing version-specific files.',
          'ERP parameters loads the recommended preset into the editor. Review it, then save to apply.'
        ],
        notes: [
          'A Runtime cannot be removed while a Site still uses that PHP series.'
        ]
      },
      {
        id: 'mariadb',
        title: 'MariaDB',
        summary: 'Install and manage fabDev MariaDB without taking over an existing System or Homebrew service.',
        steps: [
          'Install MariaDB with the matching manifest and Runtime package, then start the service from this page.',
          'Stop MariaDB before changing its TCP port, data directory, or advanced configuration.',
          'The data directory must be empty or already contain a valid MariaDB system database.',
          'Set Root password synchronizes root access over localhost Socket and 127.0.0.1 TCP. fabDev does not store the password.',
          'Additional my.cnf or my.ini options are validated before saving; managed paths, ports, sockets, logs, and loopback binding remain controlled by fabDev.'
        ],
        notes: [
          'When managed MariaDB is stopped or unavailable, PHP automatically uses a detected System or Homebrew MariaDB Socket.',
          'Removing the Runtime preserves MariaDB configuration, data, and logs.'
        ]
      },
      {
        id: 'nodejs',
        title: 'Node.js',
        summary: 'Install the optional stable LTS Runtime managed independently by fabDev.',
        steps: [
          'Select Install stable LTS, then choose the matching manifest and Runtime package.',
          'Select Remove when the Runtime is no longer required.'
        ],
        notes: [
          'This Runtime does not replace Homebrew, nvm, Herd, the system Node.js, or your shell PATH.',
          'fabDev does not currently start project npm scripts automatically.'
        ]
      },
      {
        id: 'proxy',
        title: 'Proxy Manager',
        summary: 'Create independent loopback proxies for remote ERP HTTP APIs.',
        steps: [
          'Add a connection with a unique ID, local .test domain, port, and absolute http:// target.',
          'List exact Credential Origins when browser requests need CORS credentials. Leaving the field blank allows the Proxy domain over HTTP and HTTPS.',
          'Start, Stop, or Restart one connection, or use the page-wide Start All and Stop All controls.',
          'Editing a running connection restarts it automatically. Import and Export transfer connection definitions, not running state.'
        ],
        notes: [
          'Listeners bind only to 127.0.0.1. One failed connection does not stop other proxies.',
          'Removing a running connection stops it and releases its port first.'
        ]
      },
      {
        id: 'settings-shutdown',
        title: 'Settings and shutdown',
        summary: 'Choose the display language and whether the Web stack starts when fabDev opens.',
        steps: [
          'Language changes immediately and is kept for the next launch.',
          'Auto-start controls DNS, Nginx, and required PHP-FPM services. Turning it off does not stop currently running services.',
          'Closing the main window leaves the Agent and managed services running.',
          'Use Quit fabDev from the menu bar when you want fabDev to stop managed services and clean up its background processes.'
        ]
      },
      {
        id: 'troubleshooting',
        title: 'Troubleshooting',
        summary: 'Check the narrowest layer first: Agent, Site, Runtime, service, then browser.',
        steps: [
          'Agent disconnected: select Refresh. If it remains disconnected, quit and reopen fabDev.',
          'A .test Site does not open: confirm the Site is registered and enabled, PHP is installed, then Stop All and Start All.',
          'HTTPS warning: confirm HTTPS is enabled for the Site and the fabDev CA is trusted in the current user Login Keychain.',
          'MariaDB cannot start: check whether another service already uses port 3306 and verify the selected data directory.',
          'Proxy failed: verify that the local port is free, the target uses http://, and the remote server is reachable.'
        ],
        notes: [
          'Copy the visible error message before restarting. It is usually the fastest clue to the failing layer.'
        ]
      }
    ]
  },
  'zh-TW': {
    title: 'fabDev 操作手冊',
    description: 'ERP Web 本機開發環境 · 在 App 任一畫面按 F1 都可開啟本手冊。',
    closeLabel: '關閉操作手冊',
    contentsLabel: '手冊目錄',
    shortcutLabel: 'F1 操作手冊',
    sections: [
      {
        id: 'quick-start',
        title: '快速開始',
        summary: '建立 Site、指定 PHP、啟動 Web 服務，然後開啟 `.test` 網站。',
        steps: [
          '先確認左側底部顯示「Agent 已連線」。',
          '進入 Sites。可用 Site Home 掃描第一層資料夾，或手動新增一個 linked Site。',
          '確認網域、Web Root 與 PHP Runtime；Laravel 專案的 Web Root 通常是 `public`。',
          '回到總覽按「全部啟動」，再到 Sites 頁面按「開啟」。'
        ],
        notes: [
          '「全部啟動」只管理 DNS、Nginx 與 Sites 所需的 PHP-FPM；MariaDB 與 Proxy 連線各自獨立管理。'
        ]
      },
      {
        id: 'overview',
        title: '總覽與服務狀態',
        summary: '總覽顯示 DNS、Nginx、PHP、MariaDB、Node.js 與 Proxy Manager 的目前狀態。',
        steps: [
          '使用「全部啟動／全部停止」管理 Web Stack。',
          '若曾在 App 外變更服務，或要確認錯誤是否已恢復，請按「重新整理」。',
          'PHP-FPM 顯示排隊或慢請求時，先檢查專案與 Slow Log，再決定是否增加 Worker。'
        ]
      },
      {
        id: 'sites',
        title: 'Sites',
        summary: '管理專案資料夾、`.test` 網域、Web Root、PHP 版本、HTTPS 與臨時局網分享。',
        steps: [
          'Site Home 會把第一層資料夾或指向資料夾的 symbolic link 建立為同名 `.test` Site；隱藏及更深層資料夾不會加入。',
          '不在 Site Home 內的專案可用「新增 Site」加入；網域衝突時，手動加入的 linked Site 優先。',
          '按「編輯」可修改名稱、專案路徑、網域與 Web Root；PHP Runtime 可直接在 Site 列上切換。',
          '按「啟用 HTTPS」為該 Site 建立憑證；瀏覽器要正確認可，還需依提示信任 fabDev CA。',
          '按「局網分享」可透過 fabDev Connect 讓受信任區網內的裝置短時間測試。',
          '匯入／匯出可轉移 Site 設定；重複項目會略過。'
        ],
        notes: [
          '移除 Site 只會刪除 fabDev 的登錄資料，不會刪除專案資料夾。',
          '局網分享沒有 TLS 與使用者登入保護，請勿用於不受信任網路或正式環境。'
        ]
      },
      {
        id: 'php',
        title: 'PHP 設定',
        summary: '安裝選用 PHP Runtime、設定全域版本，並管理經驗證的 `php.ini`。',
        steps: [
          '可用 PHP 版本來自已安裝套件與 Runtime Catalog；「安裝本機套件」會先驗證相符的描述檔與套件。',
          '「設為全域」會變更預設 PHP 系列；已明確指定版本的 Site 不會跟著改變。',
          '開啟個別版本的 `php.ini` 後，「儲存並套用」會先交給對應 PHP-FPM 驗證。',
          '「Default php.ini」是日後新安裝版本的範本，不會覆蓋現有各版本設定。',
          '「ERP 參數」只會先把建議值載入編輯器；檢查後仍須儲存才會套用。'
        ],
        notes: [
          '仍有 Site 使用某個 PHP 系列時，該 Runtime 不可移除。'
        ]
      },
      {
        id: 'mariadb',
        title: 'MariaDB',
        summary: '管理 fabDev 專用 MariaDB，同時不接管既有 System／Homebrew MariaDB。',
        steps: [
          '用相符的描述檔與 Runtime 套件完成安裝，再從本頁啟動服務。',
          '要修改 TCP Port、Data Directory 或進階設定前，必須先停止 MariaDB。',
          'Data Directory 必須是空目錄，或已包含有效 MariaDB 系統資料庫的既有目錄。',
          '設定 Root 密碼會同步 `localhost` Socket 與 `127.0.0.1` TCP 的 root 密碼；fabDev 不會保存密碼。',
          '額外 `my.cnf`／`my.ini` 選項會先驗證再儲存；路徑、Port、Socket、PID、Log 與 loopback 綁定仍由 fabDev 管理。'
        ],
        notes: [
          'Managed MariaDB 停止或未安裝時，PHP 會自動改用偵測到的 System／Homebrew MariaDB Socket。',
          '移除 Runtime 會保留 MariaDB 設定、資料與 Log。'
        ]
      },
      {
        id: 'nodejs',
        title: 'Node.js',
        summary: '安裝由 fabDev 獨立管理的選用 Stable LTS Runtime。',
        steps: [
          '按「安裝穩定 LTS」，依序選擇相符的描述檔與 Runtime 套件。',
          '專案不再需要時，可按「移除」刪除這份 Runtime。'
        ],
        notes: [
          '此 Runtime 不會取代 Homebrew、nvm、Herd、系統 Node.js，也不會修改 Shell PATH。',
          'fabDev 目前不會自動啟動專案的 npm scripts。'
        ]
      },
      {
        id: 'proxy',
        title: 'Proxy Manager',
        summary: '為遠端 ERP HTTP API 建立彼此獨立、只綁 loopback 的本機 Proxy。',
        steps: [
          '新增連線時填入唯一 ID、本機 `.test` 網域、Port 與完整 `http://` Target。',
          '瀏覽器請求需要 CORS Credentials 時，逐行列出允許的精確 Origin；留空則允許此 Proxy 網域的 HTTP 與 HTTPS。',
          '可單獨啟動、停止、重新啟動，也可使用頁面上的「全部啟動／全部停止」。',
          '修改運行中的連線會自動重新啟動；匯入／匯出只轉移定義，不匯入運行狀態。'
        ],
        notes: [
          'Listener 只綁 `127.0.0.1`；單一遠端故障不會停止其他 Proxy。',
          '移除運行中的連線時，fabDev 會先停止並釋放 Port。'
        ]
      },
      {
        id: 'settings-shutdown',
        title: '設定與正確關閉方式',
        summary: '選擇顯示語言，以及 fabDev 開啟時是否自動啟動 Web Stack。',
        steps: [
          '語言變更會立即套用，並保留至下次啟動。',
          '自動啟動控制 DNS、Nginx 與 Sites 所需的 PHP-FPM；關閉此選項不會停止目前服務。',
          '只關閉主視窗時，Agent 與受管服務會繼續運行。',
          '若要停止 fabDev 受管服務並清理背景程序，請從 menu bar 選擇「Quit fabDev」。'
        ]
      },
      {
        id: 'troubleshooting',
        title: '疑難排解',
        summary: '依序確認 Agent、Site、Runtime、服務與瀏覽器，可較快找到失敗層。',
        steps: [
          'Agent 未連線：先按「重新整理」；仍未連線時，完整退出再重新開啟 fabDev。',
          '`.test` Site 無法開啟：確認 Site 已登錄且啟用、PHP 已安裝，再執行「全部停止 → 全部啟動」。',
          'HTTPS 警告：確認 Site 已啟用 HTTPS，且目前使用者的 Login Keychain 已信任 fabDev CA。',
          'MariaDB 無法啟動：檢查 3306 是否已被其他服務占用，並確認所選 Data Directory 有效。',
          'Proxy 啟動失敗：確認本機 Port 未占用、Target 使用 `http://`，且遠端主機可連線。'
        ],
        notes: [
          '重新啟動前先複製畫面上的錯誤訊息，通常能最快辨識問題所在層。'
        ]
      }
    ]
  },
  'zh-CN': {
    title: 'fabDev 操作手册',
    description: 'ERP Web 本地开发环境 · 在 App 任一页面按 F1 都可打开本手册。',
    closeLabel: '关闭操作手册',
    contentsLabel: '手册目录',
    shortcutLabel: 'F1 操作手册',
    sections: [
      {
        id: 'quick-start',
        title: '快速开始',
        summary: '建立 Site、指定 PHP、启动 Web 服务，然后打开 `.test` 网站。',
        steps: [
          '先确认左侧底部显示“Agent 已连接”。',
          '进入 Sites。可用 Site Home 扫描第一层文件夹，或手动新增一个 linked Site。',
          '确认域名、Web Root 与 PHP Runtime；Laravel 项目的 Web Root 通常是 `public`。',
          '回到总览按“全部启动”，再到 Sites 页面按“打开”。'
        ],
        notes: [
          '“全部启动”只管理 DNS、Nginx 与 Sites 所需的 PHP-FPM；MariaDB 与 Proxy 连接各自独立管理。'
        ]
      },
      {
        id: 'overview',
        title: '总览与服务状态',
        summary: '总览显示 DNS、Nginx、PHP、MariaDB、Node.js 与 Proxy Manager 的当前状态。',
        steps: [
          '使用“全部启动／全部停止”管理 Web Stack。',
          '若曾在 App 外变更服务，或要确认错误是否已恢复，请按“刷新”。',
          'PHP-FPM 显示排队或慢请求时，先检查项目与 Slow Log，再决定是否增加 Worker。'
        ]
      },
      {
        id: 'sites',
        title: 'Sites',
        summary: '管理项目文件夹、`.test` 域名、Web Root、PHP 版本、HTTPS 与临时局域网共享。',
        steps: [
          'Site Home 会把第一层文件夹或指向文件夹的 symbolic link 建立为同名 `.test` Site；隐藏及更深层文件夹不会加入。',
          '不在 Site Home 内的项目可用“新增 Site”加入；域名冲突时，手动加入的 linked Site 优先。',
          '按“编辑”可修改名称、项目路径、域名与 Web Root；PHP Runtime 可直接在 Site 行上切换。',
          '按“启用 HTTPS”为该 Site 建立证书；浏览器要正确认可，还需依提示信任 fabDev CA。',
          '按“局域网共享”可通过 fabDev Connect 让受信任局域网内的设备短时间测试。',
          '导入／导出可转移 Site 设置；重复项目会跳过。'
        ],
        notes: [
          '移除 Site 只会删除 fabDev 的登记资料，不会删除项目文件夹。',
          '局域网共享没有 TLS 与用户登录保护，请勿用于不受信任网络或正式环境。'
        ]
      },
      {
        id: 'php',
        title: 'PHP 设置',
        summary: '安装可选 PHP Runtime、设置全局版本，并管理经过验证的 `php.ini`。',
        steps: [
          '可用 PHP 版本来自已安装包与 Runtime Catalog；“安装本地包”会先验证相符的描述文件与包。',
          '“设为全局”会变更默认 PHP 系列；已明确指定版本的 Site 不会随之改变。',
          '打开个别版本的 `php.ini` 后，“保存并应用”会先交给对应 PHP-FPM 验证。',
          '“Default php.ini”是日后新安装版本的模板，不会覆盖现有各版本设置。',
          '“ERP 参数”只会先把建议值载入编辑器；检查后仍须保存才会应用。'
        ],
        notes: ['仍有 Site 使用某个 PHP 系列时，该 Runtime 不可移除。']
      },
      {
        id: 'mariadb',
        title: 'MariaDB',
        summary: '管理 fabDev 专用 MariaDB，同时不接管现有 System／Homebrew MariaDB。',
        steps: [
          '用相符的描述文件与 Runtime 包完成安装，再从本页启动服务。',
          '要修改 TCP Port、Data Directory 或高级设置前，必须先停止 MariaDB。',
          'Data Directory 必须是空目录，或已包含有效 MariaDB 系统数据库的现有目录。',
          '设置 Root 密码会同步 `localhost` Socket 与 `127.0.0.1` TCP 的 root 密码；fabDev 不会保存密码。',
          '额外 `my.cnf`／`my.ini` 选项会先验证再保存；路径、Port、Socket、PID、Log 与 loopback 绑定仍由 fabDev 管理。'
        ],
        notes: [
          'Managed MariaDB 停止或未安装时，PHP 会自动改用检测到的 System／Homebrew MariaDB Socket。',
          '移除 Runtime 会保留 MariaDB 设置、数据与 Log。'
        ]
      },
      {
        id: 'nodejs',
        title: 'Node.js',
        summary: '安装由 fabDev 独立管理的可选 Stable LTS Runtime。',
        steps: [
          '按“安装稳定 LTS”，依次选择相符的描述文件与 Runtime 包。',
          '项目不再需要时，可按“移除”删除这份 Runtime。'
        ],
        notes: [
          '此 Runtime 不会取代 Homebrew、nvm、Herd、系统 Node.js，也不会修改 Shell PATH。',
          'fabDev 目前不会自动启动项目的 npm scripts。'
        ]
      },
      {
        id: 'proxy',
        title: 'Proxy Manager',
        summary: '为远端 ERP HTTP API 建立彼此独立、只绑定 loopback 的本地 Proxy。',
        steps: [
          '新增连接时填写唯一 ID、本地 `.test` 域名、Port 与完整 `http://` Target。',
          '浏览器请求需要 CORS Credentials 时，逐行列出允许的精确 Origin；留空则允许此 Proxy 域名的 HTTP 与 HTTPS。',
          '可单独启动、停止、重新启动，也可使用页面上的“全部启动／全部停止”。',
          '修改运行中的连接会自动重新启动；导入／导出只转移定义，不导入运行状态。'
        ],
        notes: [
          'Listener 只绑定 `127.0.0.1`；单一远端故障不会停止其他 Proxy。',
          '移除运行中的连接时，fabDev 会先停止并释放 Port。'
        ]
      },
      {
        id: 'settings-shutdown',
        title: '设置与正确关闭方式',
        summary: '选择显示语言，以及 fabDev 打开时是否自动启动 Web Stack。',
        steps: [
          '语言变更会立即应用，并保留到下次启动。',
          '自动启动控制 DNS、Nginx 与 Sites 所需的 PHP-FPM；关闭此选项不会停止当前服务。',
          '只关闭主窗口时，Agent 与受管服务会继续运行。',
          '若要停止 fabDev 受管服务并清理后台进程，请从 menu bar 选择“Quit fabDev”。'
        ]
      },
      {
        id: 'troubleshooting',
        title: '疑难排解',
        summary: '依次确认 Agent、Site、Runtime、服务与浏览器，可较快找到失败层。',
        steps: [
          'Agent 未连接：先按“刷新”；仍未连接时，完整退出再重新打开 fabDev。',
          '`.test` Site 无法打开：确认 Site 已登记且启用、PHP 已安装，再执行“全部停止 → 全部启动”。',
          'HTTPS 警告：确认 Site 已启用 HTTPS，且当前用户的 Login Keychain 已信任 fabDev CA。',
          'MariaDB 无法启动：检查 3306 是否已被其他服务占用，并确认所选 Data Directory 有效。',
          'Proxy 启动失败：确认本地 Port 未占用、Target 使用 `http://`，且远端主机可连接。'
        ],
        notes: ['重新启动前先复制页面上的错误信息，通常能最快辨识问题所在层。']
      }
    ]
  }
}

export function isHelpShortcut(event: Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey'>) {
  return event.key === 'F1'
    && !event.altKey
    && !event.ctrlKey
    && !event.metaKey
    && !event.shiftKey
}

export function getOperationManual(language: Language): OperationManual {
  return manuals[language]
}
