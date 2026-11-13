//! Listas embutidas da baseline, revisáveis item a item. O conteúdo vem das
//! specs do change `add-sandbox-cli`.

/// Arquivos de senha e de privilégio de `/etc`, cobertos.
pub const ETC_PRIVILEGE: &[&str] = &[
    "/etc/shadow",
    "/etc/shadow-",
    "/etc/gshadow",
    "/etc/gshadow-",
    "/etc/security/opasswd",
    "/etc/sudoers",
    "/etc/sudoers.d",
    "/etc/environment",
];

/// Chaves e credenciais de serviço de `/etc`, cobertas. As chaves privadas de
/// host em `/etc/ssh` são acrescentadas por padrão de nome.
pub const ETC_SERVICE_SECRETS: &[&str] = &[
    "/etc/ssl/private",
    "/etc/pki/tls/private",
    "/etc/krb5.keytab",
    "/etc/ipsec.secrets",
    "/etc/ipsec.d",
    "/etc/letsencrypt",
    "/etc/apt/auth.conf",
    "/etc/apt/auth.conf.d",
    "/etc/docker",
    "/etc/kubernetes",
    "/etc/rancher",
    "/etc/libvirt",
    "/etc/iscsi",
    "/etc/samba",
    "/etc/cups",
    "/etc/davfs2",
    "/etc/msmtprc",
    "/etc/postfix/sasl_passwd",
    "/etc/crypttab",
];

/// Credenciais de rede de `/etc`, cobertas.
pub const ETC_NETWORK: &[&str] = &[
    "/etc/NetworkManager/system-connections",
    "/etc/netplan",
    "/etc/wpa_supplicant",
    "/etc/wireguard",
    "/etc/openvpn",
    "/etc/ppp",
];

/// Pastas de sistema montadas em somente leitura (ou recriadas como link
/// simbólico, quando são links no host).
pub const SYSTEM_DIRS: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/libx32", "/opt", "/etc",
];

/// Caches de pacotes do HOME, com camada de escrita.
pub const CACHES: &[&str] = &[
    ".npm",
    ".cache/pip",
    ".cache/uv",
    ".cache/go-build",
    ".cache/ms-playwright",
    ".cache/huggingface",
];

/// Arquivos cobertos dentro dos caches.
pub const CACHE_COVERS: &[&str] = &[
    ".cache/huggingface/token",
    ".cache/huggingface/stored_tokens",
];

/// Credenciais conhecidas: montar uma delas de propósito gera aviso. Caminhos
/// relativos ao HOME.
pub const KNOWN_CREDENTIALS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".config/gcloud",
    ".azure",
    ".docker",
    ".config/gh",
    ".terraform.d",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".config/git/credentials",
    ".password-store",
    ".local/share/keyrings",
    ".vault-token",
    // perfis de navegador
    ".mozilla",
    ".config/google-chrome",
    ".config/chromium",
    ".config/BraveSoftware",
    ".config/microsoft-edge",
    ".config/vivaldi",
    // estado real dos harnesses suportados
    ".hermes",
    ".config/opencode",
    ".local/share/opencode",
];

/// Chamadas de sistema negadas pelo seccomp.
pub const DENY_SYSCALLS: &[&str] = &[
    "mount",
    "umount2",
    "pivot_root",
    "move_mount",
    "open_tree",
    "fsopen",
    "fsconfig",
    "fsmount",
    "fspick",
    "mount_setattr",
    "bpf",
    "perf_event_open",
    "init_module",
    "finit_module",
    "delete_module",
    "kexec_load",
    "kexec_file_load",
    "keyctl",
    "add_key",
    "request_key",
    "userfaultfd",
    "io_uring_setup",
    "io_uring_enter",
    "io_uring_register",
];

/// Comandos de `ioctl` negados: os que injetam teclas no terminal.
pub const DENY_IOCTLS: &[&str] = &["TIOCSTI", "TIOCLINUX"];

/// Variáveis copiadas do ambiente do usuário quando existem.
pub const SAFE_ENV: &[&str] = &[
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "LANG",
    "TZ",
    "NO_COLOR",
];

/// `PATH` de sistema, depois das pastas das toolchains.
pub const SYSTEM_PATH: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];
