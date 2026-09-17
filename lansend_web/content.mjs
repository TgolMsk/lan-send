// 官网的全部配置与文案。build.mjs 只管排版，改字、改版本号、加语言都在这里。
//
// 版本升级：改 config.version / config.released，再 `node lansend_web/build.mjs`。
// 下载链接由版本号拼出来，文件名与 .github/workflows/release.yml 的产物一致。

// 站点根地址：canonical、og:url、hreflang、sitemap 用；页面之间是相对路径，不受它影响。
const SITE_URL = process.env.SITE_URL ?? 'https://ls.mixduo.cn'

// 备案号是绑在具体域名上的，只有站点确实挂在备案域名下才能印，换域名（GitHub Pages 之类）
// 就不能带着它走。所以按域名查表，而不是写死一个默认值；`SITE_ICP` / `SITE_POLICE` 仍可覆盖。
// 公安联网备案要在上线 30 天内办，办下来后把号填到这里的 police 字段。
const BEIAN = {
  // 页脚印的是这个域名的网站备案号；App Store Connect 里填的是 APP 备案号
  // 蜀ICP备2026054850号-2A（带 A 后缀的那条），两者不是一回事，别弄混。
  'https://ls.mixduo.cn': { icp: '蜀ICP备2026054850号', police: '' },
}
const beian = BEIAN[SITE_URL] ?? {}

export const config = {
  version: '0.5.0',
  released: '2026-09-17',
  repo: 'https://github.com/TgolMsk/lan-send',
  releases: 'https://github.com/TgolMsk/lan-send/releases',
  releaseTag: (v) => `https://github.com/TgolMsk/lan-send/releases/tag/v${v}`,
  asset: (v, name) => `https://github.com/TgolMsk/lan-send/releases/download/v${v}/${name}`,
  // iOS 版在 App Store 的地址（Apple ID 6809459213，不带国家代码会跳到访问者所在的商店）。
  appStore: 'https://apps.apple.com/app/id6809459213',
  email: '511297735@qq.com',
  author: 'Wang Sheng',
  license: 'MIT',
  localsend: 'https://localsend.org',
  siteUrl: SITE_URL,
  // 工信部要求网站底部标明备案编号并链接备案系统。
  icp: process.env.SITE_ICP ?? beian.icp ?? '',
  police: process.env.SITE_POLICE ?? beian.police ?? '',
  policeUrl: process.env.SITE_POLICE_URL ?? 'https://beian.mps.gov.cn/',
}

// 每种语言一个目录：zh 在站点根，en 在 /en/。
export const langs = {
  zh: { dir: '', htmlLang: 'zh-Hans', label: '中文', other: 'en' },
  en: { dir: 'en', htmlLang: 'en', label: 'English', other: 'zh' },
}

const shots = {
  desktop: ['1-devices', '2-transfer', '3-clipboard', '4-history', '5-settings'],
  iphone: ['1-devices', '2-incoming', '3-transfer', '4-history', '5-settings'],
  ipad: ['1-devices', '2-transfer', '3-history'],
}

export const content = {
  zh: {
    meta: {
      title: 'LanSend —— 局域网文件与剪贴板互传，不经云端',
      description:
        '在同一 Wi‑Fi 下的 iPhone、Mac 和 Windows 之间发送照片、视频、整个文件夹和剪贴板。不用账号，不经云端，没有大小限制，兼容 LocalSend 协议。MIT 开源。',
      ogAlt: 'LanSend：局域网文件与剪贴板互传',
    },
    nav: [
      ['#features', '特性'],
      ['#screens', '界面'],
      ['#how', '工作原理'],
      ['#download', '下载'],
      ['#cli', '命令行'],
      ['#faq', '常见问题'],
    ],
    header: { menu: '菜单', github: 'GitHub', langSwitch: 'English' },
    hero: {
      chip: (v, d) => `版本 v${v} · ${d}`,
      chipHint: '更新内容',
      lead: '局域网文件与剪贴板互传，数据不出你的网络',
      sub: '在同一 Wi‑Fi 下的 iPhone、Mac 和 Windows 之间发送照片、视频、整个文件夹与剪贴板。不用账号，不经云端，没有大小限制——数据直接从一台设备到另一台。',
      download: (v) => `下载 v${v}`,
      appStore: 'App Store（iOS）',
      source: '查看源码',
      meta: 'macOS 12+ · Windows 10+ · iOS 15+ · MIT 开源 · 无遥测',
      shotAlt: '桌面端设备页：列出同一局域网里可以收文件的设备',
      phoneAlt: 'iPhone 上收到文件请求的弹窗',
      windowTitle: 'LanSend —— 设备',
    },
    features: {
      eyebrow: '特性',
      title: '传文件该有的样子',
      sub: '一个局域网、一次点击，剩下的交给它。',
      items: [
        ['link', '兼容 LocalSend', '实现 LocalSend 协议 v2.2，与官方客户端互相发现、互相收发，也就能和 Android、Linux 上的 LocalSend 互传。'],
        ['cloud-off', '不经云端', '设备之间直连，没有服务器、没有账号、没有遥测，也没有文件大小限制。断网的局域网里照样能用。'],
        ['devices', '三端一套界面', 'macOS、Windows 与 iOS 同一套设计：桌面端侧栏，手机端自动换成底部标签栏。'],
        ['shield', '加密与校验', '双向 TLS（mTLS）加密，指纹核对；可要求对方输入 PIN，每个文件收完核对 SHA‑256。'],
        ['clipboard', '剪贴板同步', '配对后，文本、图片与文件列表在电脑之间自动同步（macOS 与 Windows）。'],
        ['resume', '断点续传', '传输中断、设备睡眠、Wi‑Fi 抖动之后从断点继续，不用从头再来。'],
        ['folder', '文件夹与自动分类', '整个目录一起发，目录结构原样保留；接收端可按设备、日期或类型分目录存放。'],
        ['image', '媒体缩略图', '按内容识别类型，历史里直接显示图片缩略图（HEIC / AVIF 走系统解码器），音乐显示标题与时长。'],
        ['history', '传输历史', '收发记录可查、可重发、可删除，也可以一键清空。'],
        ['terminal', '命令行工具', '`lan-send` 带发现、收发、配对、剪贴板与历史子命令，macOS、Windows 与 Linux 都有。'],
        ['globe', '八种语言 · 深浅色', '简中、繁中、英、日、韩、德、法、西；跟随系统深浅色，也可以手动指定。'],
        ['bolt', 'Rust 内核', '流式读写，几十 GB 的文件也不占内存；文件名净化，接收目录之外零写入。'],
      ],
    },
    screens: {
      eyebrow: '界面',
      title: '三端同一套设计',
      sub: '截图取自应用本身，未经修饰。',
      tabs: [
        ['desktop', '桌面端'],
        ['iphone', 'iPhone'],
        ['ipad', 'iPad'],
      ],
      captions: {
        desktop: {
          '1-devices': ['设备', '同一局域网里的设备，在线状态、配对与收藏一目了然；网络禁止发现时可以直接输入 IP。'],
          '2-transfer': ['传输', '大数字总进度，外加逐个文件的进度、速度与剩余时间。'],
          '3-clipboard': ['剪贴板', '与已配对的电脑双向同步，文本与图片都可以，历史里能重新复制。'],
          '4-history': ['历史', '收发过的文件，带缩略图、校验结果与去向。'],
          '5-settings': ['设置', '设备名、接收目录、自动接收、PIN、分类规则与语言。'],
        },
        iphone: {
          '1-devices': ['设备', '打开就能看到附近的电脑与手机。'],
          '2-incoming': ['接收请求', '对方发过来时先问你，可以只收其中几个文件。'],
          '3-transfer': ['传输', '进度、速度与逐文件明细，切到后台也会提示。'],
          '4-history': ['历史', '收到的文件存在“文件”App 的 LanSend 目录里。'],
          '5-settings': ['设置', '自动接收、PIN、语言与外观。'],
        },
        ipad: {
          '1-devices': ['设备', 'iPad 上用更宽的布局，卡片自动铺满。'],
          '2-transfer': ['传输', '大屏上进度与文件列表一起看。'],
          '3-history': ['历史', '缩略图更大，翻找更快。'],
        },
      },
      shots,
    },
    how: {
      eyebrow: '工作原理',
      title: '三步，全程不出局域网',
      sub: '和官方 LocalSend 一样的协议：UDP 组播发现 + HTTPS 直传。',
      steps: [
        ['发现', '每台设备用 UDP 组播在局域网里播报自己的别名与指纹，几百毫秒就互相看见。公司或公共 Wi‑Fi 屏蔽组播时，直接输入对方 IP 也能发。'],
        ['确认', '接收端先看到文件清单再决定收不收，可以只收其中几个。可以要求对方输 PIN，或者先配对一次（两端显示同一个 6 位码）。'],
        ['直传', '两台设备之间建 mTLS 加密连接，文件流式写入你指定的目录，收完逐个核对 SHA‑256。中途断了下次接着传。'],
      ],
      diagram: {
        phone: 'iPhone',
        laptop: 'MacBook',
        router: 'Wi‑Fi 路由器',
        direct: 'mTLS 直连传输',
        cloud: '不经过任何服务器',
        packet: 'IMG_2041.HEIC',
      },
      note: '协议细节见 [LocalSend 接口清单](https://github.com/TgolMsk/lan-send/blob/main/docs/localsend-v2-interface-checklist.md) 与 [协议扩展说明](https://github.com/TgolMsk/lan-send/blob/main/docs/protocol-extensions.md)：断点续传、配对与剪贴板同步是私有扩展，官方客户端会自动忽略。',
    },
    download: {
      eyebrow: '下载',
      title: (v) => `下载 LanSend v${v}`,
      sub: '免费、开源，安装包由 GitHub Actions 构建、签名并公证。',
      tabs: [
        ['macos', 'macOS'],
        ['windows', 'Windows'],
        ['ios', 'iOS'],
        ['cli', '命令行'],
      ],
      recommended: '推荐',
      all: '全部安装包与校验和',
      checksums: '校验和 SHA256SUMS.txt',
      notes: {
        macos: '通用二进制，Apple Silicon 与 Intel 都能装。要求 macOS 12 或更新。',
        windows: '要求 Windows 10 或更新（系统自带 WebView2；没有的话安装程序会提示）。',
        ios: '要求 iOS 15 或更新，iPhone 与 iPad 通用。首次启动请允许“本地网络”，否则看不到其他设备。',
        cli: '同一套 Rust 核心，不带界面，适合脚本与服务器。Linux 只有命令行版。',
      },
      files: {
        macos: [
          ['dmg', 'lan-send-{v}-macos-universal.dmg', 'LanSend.dmg', '19.5 MB', true],
          ['zip', 'lan-send-{v}-macos-universal.app.zip', '免安装 .app.zip', '18.9 MB', false],
        ],
        windows: [
          ['exe', 'lan-send-{v}-windows-x86_64-setup.exe', '安装程序 .exe', '6.4 MB', true],
          ['msi', 'lan-send-{v}-windows-x86_64.msi', '静默部署 .msi', '9.1 MB', false],
        ],
        cli: [
          ['pkg', 'lan-send-cli-{v}-macos-universal.pkg', 'macOS .pkg（装到 /usr/local/bin）', '11.1 MB', true],
          ['msi', 'lan-send-cli-{v}-windows-x86_64.msi', 'Windows .msi（加入 PATH）', '6.0 MB', true],
          ['tar', 'lan-send-cli-{v}-linux-x86_64.tar.gz', 'Linux x86_64 .tar.gz', '6.1 MB', false],
          ['tar', 'lan-send-cli-{v}-linux-aarch64.tar.gz', 'Linux aarch64 .tar.gz', '6.0 MB', false],
        ],
      },
      ios: {
        title: '在 App Store 上获取',
        body: 'iPhone 与 iPad 版已经上架 App Store，免费、无内购、无广告。',
        badge: 'App Store',
        ipa: '开发者也可以从 Releases 取 .ipa 自行安装。',
      },
    },
    cli: {
      eyebrow: '命令行',
      title: '没有界面的时候用 lan-send',
      sub: '和应用共用一套核心与配置：同样的发现、同样的加密、同样的历史。',
      copy: '复制',
      copied: '已复制',
      lines: [
        ['lan-send discover', '列出局域网里的设备'],
        ['lan-send send "Nice Orange" a.jpg photos/', '按别名、指纹前缀或 IP 发送文件与文件夹'],
        ['lan-send receive --dir ~/Downloads', '前台接收，逐个请求确认'],
        ['lan-send receive --organize device,type', '按设备与类型分目录存放'],
        ['lan-send pair "Nice Orange"', '配对：两端核对同一个 6 位码'],
        ['lan-send clip watch', '与已配对设备双向同步剪贴板'],
        ['lan-send history --limit 20', '查看传输历史'],
      ],
      more: '完整用法见 `lan-send --help` 与仓库 README。',
    },
    faq: {
      eyebrow: '常见问题',
      title: '你可能想先知道',
      items: [
        ['能和官方 LocalSend 互传吗？', '可以。LanSend 实现的是同一套 LocalSend 协议 v2.2，双方互相发现、互相收发都没问题，也包括 Android 和 Linux 上的 LocalSend。断点续传、配对和剪贴板同步是私有扩展，官方客户端会忽略这些字段，不影响正常传输。'],
        ['文件会经过服务器吗？要账号吗？', '都不会。文件在两台设备之间直连传输，没有中转服务器，不需要注册和登录，应用也不收集任何数据、不含广告与内购。'],
        ['找不到对方设备怎么办？', '先确认两台设备连的是同一个 Wi‑Fi、都打开了 LanSend 或 LocalSend；iOS 首次使用要允许“本地网络”权限。有些公司或公共 Wi‑Fi 会禁止设备之间的组播，这时在“设备”页的“按地址发送”里直接输入对方 IP 即可。'],
        ['收到的文件存在哪里？', 'macOS 与 Windows 默认放在“下载”文件夹，可以在设置里改；还能按设备、日期或类型自动分目录。iOS 上在“文件”App 的“我的 iPhone › LanSend”里。'],
        ['iOS 支持剪贴板同步吗？', '不支持。iOS 不允许应用在后台读剪贴板，强行做出来也不可靠，所以剪贴板同步只在 macOS 与 Windows 上提供。'],
        ['开源吗？用的什么许可证？', 'MIT。核心库、桌面应用、iOS 应用与命令行工具的完整源码都在 GitHub 上，欢迎提 issue 和 PR。'],
      ],
    },
    links: {
      eyebrow: '资源',
      title: '接着看',
      groups: [
        ['项目', [
          ['GitHub 仓库', 'https://github.com/TgolMsk/lan-send'],
          ['全部版本与安装包', 'https://github.com/TgolMsk/lan-send/releases'],
          ['更新日志', 'https://github.com/TgolMsk/lan-send/blob/main/CHANGELOG.md'],
          ['提交问题', 'https://github.com/TgolMsk/lan-send/issues'],
        ]],
        ['文档', [
          ['LocalSend 接口清单', 'https://github.com/TgolMsk/lan-send/blob/main/docs/localsend-v2-interface-checklist.md'],
          ['协议扩展与差异', 'https://github.com/TgolMsk/lan-send/blob/main/docs/protocol-extensions.md'],
          ['三端平台约束', 'https://github.com/TgolMsk/lan-send/blob/main/docs/platforms.md'],
          ['架构决策记录（ADR）', 'https://github.com/TgolMsk/lan-send/tree/main/docs/adr'],
        ]],
        ['支持', [
          ['技术支持与 FAQ', 'support/'],
          ['隐私政策', 'privacy/'],
          ['写邮件给我们', 'mailto:511297735@qq.com'],
          ['LocalSend 官网', 'https://localsend.org'],
        ]],
      ],
    },
    footer: {
      tagline: '局域网文件与剪贴板互传，不经云端。',
      rights: (y, who) => `© ${y} ${who}`,
      license: 'MIT 许可证',
      privacy: '隐私政策',
      support: '技术支持',
      credit:
        '协议来自 <a href="https://github.com/localsend/localsend">LocalSend</a>（Apache‑2.0）。本项目是独立实现，不复用其代码。',
      backTop: '回到顶部',
    },
    page: { home: '首页' },
  },

  en: {
    meta: {
      title: 'LanSend — Wi‑Fi file and clipboard transfer, no cloud',
      description:
        'Send photos, videos, whole folders and your clipboard between iPhone, Mac and Windows on the same Wi‑Fi. No account, no cloud, no size limits. Speaks the LocalSend protocol. MIT licensed.',
      ogAlt: 'LanSend: local network file and clipboard transfer',
    },
    nav: [
      ['#features', 'Features'],
      ['#screens', 'Screenshots'],
      ['#how', 'How it works'],
      ['#download', 'Download'],
      ['#cli', 'CLI'],
      ['#faq', 'FAQ'],
    ],
    header: { menu: 'Menu', github: 'GitHub', langSwitch: '中文' },
    hero: {
      chip: (v, d) => `Version v${v} · ${d}`,
      chipHint: 'What’s new',
      lead: 'Files and clipboard across your devices, never across the internet',
      sub: 'Send photos, videos, whole folders and your clipboard between iPhone, Mac and Windows on the same Wi‑Fi. No account, no cloud, no size limits — the data goes straight from one device to the other.',
      download: (v) => `Download v${v}`,
      appStore: 'App Store (iOS)',
      source: 'View source',
      meta: 'macOS 12+ · Windows 10+ · iOS 15+ · MIT licensed · no telemetry',
      shotAlt: 'Desktop devices page listing the devices on the same network',
      phoneAlt: 'Incoming file request on iPhone',
      windowTitle: 'LanSend — Devices',
    },
    features: {
      eyebrow: 'Features',
      title: 'What sending a file should feel like',
      sub: 'One network, one tap, and it is on the other device.',
      items: [
        ['link', 'Speaks LocalSend', 'Implements LocalSend protocol v2.2, so it discovers and exchanges files with the official clients — including LocalSend on Android and Linux.'],
        ['cloud-off', 'No cloud in the middle', 'Devices talk directly to each other. No server, no account, no telemetry, no size limit. It works on a network with no internet at all.'],
        ['devices', 'One design, three platforms', 'macOS, Windows and iOS share the same interface: a sidebar on the desktop, a tab bar on the phone.'],
        ['shield', 'Encrypted and verified', 'Mutual TLS with fingerprint checks, an optional PIN before anyone can send to you, and a SHA‑256 check on every file received.'],
        ['clipboard', 'Clipboard sync', 'Once two computers are paired, text, images and file lists follow you between them (macOS and Windows).'],
        ['resume', 'Resumable transfers', 'A dropped Wi‑Fi, a sleeping laptop or a cancelled transfer picks up where it left off instead of starting over.'],
        ['folder', 'Folders, sorted on arrival', 'Send a whole directory with its structure intact; the receiving side can file everything by device, date or type.'],
        ['image', 'Media aware', 'Types are detected from content. History shows real thumbnails (HEIC / AVIF through the system decoders) and music shows title and duration.'],
        ['history', 'Transfer history', 'Everything you sent and received, ready to re-send, remove or clear.'],
        ['terminal', 'Command line tool', '`lan-send` covers discovery, transfers, pairing, clipboard and history on macOS, Windows and Linux.'],
        ['globe', 'Eight languages · light & dark', 'English, 简体中文, 繁體中文, 日本語, 한국어, Deutsch, Français, Español — following the system theme or your choice.'],
        ['bolt', 'Rust core', 'Streamed end to end, so a 50 GB file costs no more memory than a small one. File names are sanitised and nothing is written outside your receive folder.'],
      ],
    },
    screens: {
      eyebrow: 'Screenshots',
      title: 'The same app on every screen',
      sub: 'Taken from the app itself, not retouched.',
      tabs: [
        ['desktop', 'Desktop'],
        ['iphone', 'iPhone'],
        ['ipad', 'iPad'],
      ],
      captions: {
        desktop: {
          '1-devices': ['Devices', 'Everything on your network with its online, paired and favourite state — or type an address when discovery is blocked.'],
          '2-transfer': ['Transfers', 'One big number for the whole batch, plus progress, speed and time left per file.'],
          '3-clipboard': ['Clipboard', 'Two-way sync with paired computers for text and images, with a history you can copy from again.'],
          '4-history': ['History', 'What you sent and received, with thumbnails, checksums and where it went.'],
          '5-settings': ['Settings', 'Device name, receive folder, auto-accept, PIN, sorting rules and language.'],
        },
        iphone: {
          '1-devices': ['Devices', 'Open the app and the computers and phones nearby are already there.'],
          '2-incoming': ['Incoming', 'You see what is coming before you accept — and can take only some of it.'],
          '3-transfer': ['Transfer', 'Progress, speed and per-file detail, with a notification when you switch away.'],
          '4-history': ['History', 'Received files land in the Files app under LanSend.'],
          '5-settings': ['Settings', 'Auto-accept, PIN, language and appearance.'],
        },
        ipad: {
          '1-devices': ['Devices', 'The wider layout fills the screen with device cards.'],
          '2-transfer': ['Transfers', 'Progress and the file list side by side.'],
          '3-history': ['History', 'Bigger thumbnails make finding an old transfer quick.'],
        },
      },
      shots,
    },
    how: {
      eyebrow: 'How it works',
      title: 'Three steps, all of them on your network',
      sub: 'The same protocol as the official LocalSend: UDP multicast discovery, then a direct HTTPS transfer.',
      steps: [
        ['Discover', 'Each device announces its alias and fingerprint over UDP multicast and shows up within a few hundred milliseconds. Where a corporate or public Wi‑Fi blocks multicast, typing the other device’s IP still works.'],
        ['Accept', 'The receiving side sees the file list before anything is written and can accept only part of it. You can require a PIN, or a one-time pairing where both ends confirm the same six-digit code.'],
        ['Transfer', 'The two devices open a mutually authenticated TLS connection, stream the files into the folder you chose, and verify every one with SHA‑256. An interrupted transfer resumes later.'],
      ],
      diagram: {
        phone: 'iPhone',
        laptop: 'MacBook',
        router: 'Wi‑Fi router',
        direct: 'direct mTLS transfer',
        cloud: 'no server involved',
        packet: 'IMG_2041.HEIC',
      },
      note: 'The protocol details are in the [LocalSend interface checklist](https://github.com/TgolMsk/lan-send/blob/main/docs/localsend-v2-interface-checklist.md) and [protocol extensions](https://github.com/TgolMsk/lan-send/blob/main/docs/protocol-extensions.md): resume, pairing and clipboard sync are private extensions that official clients simply ignore.',
    },
    download: {
      eyebrow: 'Download',
      title: (v) => `Download LanSend v${v}`,
      sub: 'Free and open source. Every installer is built, signed and notarised by GitHub Actions.',
      tabs: [
        ['macos', 'macOS'],
        ['windows', 'Windows'],
        ['ios', 'iOS'],
        ['cli', 'CLI'],
      ],
      recommended: 'recommended',
      all: 'All installers and checksums',
      checksums: 'Checksums SHA256SUMS.txt',
      notes: {
        macos: 'Universal binary for Apple Silicon and Intel. Requires macOS 12 or newer.',
        windows: 'Requires Windows 10 or newer (WebView2 ships with the system; the installer offers it if missing).',
        ios: 'Requires iOS 15 or newer, on iPhone and iPad. Allow the Local Network permission on first launch or no devices will show up.',
        cli: 'The same Rust core without a window — for scripts and servers. Linux is command line only.',
      },
      files: {
        macos: [
          ['dmg', 'lan-send-{v}-macos-universal.dmg', 'LanSend.dmg', '19.5 MB', true],
          ['zip', 'lan-send-{v}-macos-universal.app.zip', 'Portable .app.zip', '18.9 MB', false],
        ],
        windows: [
          ['exe', 'lan-send-{v}-windows-x86_64-setup.exe', 'Installer .exe', '6.4 MB', true],
          ['msi', 'lan-send-{v}-windows-x86_64.msi', 'Silent deploy .msi', '9.1 MB', false],
        ],
        cli: [
          ['pkg', 'lan-send-cli-{v}-macos-universal.pkg', 'macOS .pkg (installs to /usr/local/bin)', '11.1 MB', true],
          ['msi', 'lan-send-cli-{v}-windows-x86_64.msi', 'Windows .msi (adds to PATH)', '6.0 MB', true],
          ['tar', 'lan-send-cli-{v}-linux-x86_64.tar.gz', 'Linux x86_64 .tar.gz', '6.1 MB', false],
          ['tar', 'lan-send-cli-{v}-linux-aarch64.tar.gz', 'Linux aarch64 .tar.gz', '6.0 MB', false],
        ],
      },
      ios: {
        title: 'Get it on the App Store',
        body: 'The iPhone and iPad app is on the App Store — free, no in-app purchases, no ads.',
        badge: 'App Store',
        ipa: 'Developers can also grab the .ipa from the GitHub release.',
      },
    },
    cli: {
      eyebrow: 'Command line',
      title: 'When there is no window, there is lan-send',
      sub: 'Same core, same config as the app: same discovery, same encryption, same history.',
      copy: 'Copy',
      copied: 'Copied',
      lines: [
        ['lan-send discover', 'list the devices on this network'],
        ['lan-send send "Nice Orange" a.jpg photos/', 'send files and folders by alias, fingerprint or IP'],
        ['lan-send receive --dir ~/Downloads', 'receive in the foreground, confirming each request'],
        ['lan-send receive --organize device,type', 'file everything by device and type'],
        ['lan-send pair "Nice Orange"', 'pair: both ends confirm the same six-digit code'],
        ['lan-send clip watch', 'two-way clipboard sync with paired devices'],
        ['lan-send history --limit 20', 'show the transfer history'],
      ],
      more: 'Full usage in `lan-send --help` and the repository README.',
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Before you download',
      items: [
        ['Does it really work with the official LocalSend?', 'Yes. LanSend implements the same LocalSend protocol v2.2, so both sides discover each other and transfer in either direction — including LocalSend on Android and Linux. Resume, pairing and clipboard sync are private extensions; official clients ignore those fields and transfer normally.'],
        ['Do files pass through a server? Do I need an account?', 'Neither. Files go straight from one device to the other with no relay in between. There is no sign-up, no sign-in, no telemetry, no ads and no in-app purchases.'],
        ['The other device does not show up.', 'Check that both devices are on the same Wi‑Fi with LanSend or LocalSend open, and that iOS was granted the Local Network permission. Some corporate and public networks block multicast between clients; in that case use “Send by address” on the Devices page and type the other device’s IP.'],
        ['Where do received files go?', 'To the Downloads folder on macOS and Windows by default, configurable in Settings, optionally sorted into folders by device, date or type. On iOS they are in the Files app under “On My iPhone › LanSend”.'],
        ['Is clipboard sync available on iOS?', 'No. iOS does not let an app read the clipboard in the background, and a half-working version would be worse than none, so clipboard sync is macOS and Windows only.'],
        ['Is it open source? Under which licence?', 'MIT. The core library, the desktop and iOS app and the command line tool are all on GitHub; issues and pull requests are welcome.'],
      ],
    },
    links: {
      eyebrow: 'Resources',
      title: 'Read on',
      groups: [
        ['Project', [
          ['GitHub repository', 'https://github.com/TgolMsk/lan-send'],
          ['All releases', 'https://github.com/TgolMsk/lan-send/releases'],
          ['Changelog', 'https://github.com/TgolMsk/lan-send/blob/main/CHANGELOG.md'],
          ['Report an issue', 'https://github.com/TgolMsk/lan-send/issues'],
        ]],
        ['Documentation', [
          ['LocalSend interface checklist', 'https://github.com/TgolMsk/lan-send/blob/main/docs/localsend-v2-interface-checklist.md'],
          ['Protocol extensions', 'https://github.com/TgolMsk/lan-send/blob/main/docs/protocol-extensions.md'],
          ['Platform constraints', 'https://github.com/TgolMsk/lan-send/blob/main/docs/platforms.md'],
          ['Architecture decision records', 'https://github.com/TgolMsk/lan-send/tree/main/docs/adr'],
        ]],
        ['Support', [
          ['Support and FAQ', '../support/'],
          ['Privacy policy', '../privacy/'],
          ['Email us', 'mailto:511297735@qq.com'],
          ['LocalSend', 'https://localsend.org'],
        ]],
      ],
    },
    footer: {
      tagline: 'Files and clipboard across your own network, never the cloud.',
      rights: (y, who) => `© ${y} ${who}`,
      license: 'MIT licence',
      privacy: 'Privacy',
      support: 'Support',
      credit:
        'The protocol comes from <a href="https://github.com/localsend/localsend">LocalSend</a> (Apache‑2.0). This is an independent implementation and reuses none of its code.',
      backTop: 'Back to top',
    },
    page: { home: 'Home' },
  },
}
