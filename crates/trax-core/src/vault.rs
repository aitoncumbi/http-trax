//! Secrets in the OS keychain (Windows Credential Manager, Secret Service /
//! `libsecret`, macOS Keychain) and pluggable external vaults:
//! - **OS Keychain**: `keychain://<workspace>/<env>/<key>` or `courion-secret://<workspace>/<env>/<key>`
//! - **1Password**: `op://<vault>/<item>/<field>` (via 1Password Connect API or `op` CLI)
//! - **HashiCorp Vault**: `vault://<mount>/<path>#<field>` (via Vault HTTP API KV v2/v1 or `vault` CLI)
//! - **Azure Key Vault**: `azkv://<vault-name>/<secret-name>` (via Azure Key Vault REST API or `az` CLI)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{read_json, write_json};
use crate::workspace::LOCAL_DIR;

const VAULT_CONFIG_FILE: &str = "vault.json";
const CACHE_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSettings {
    /// Keep environment secret values in the OS keychain rather than `.trax/state.json`.
    #[serde(default = "default_true")]
    pub use_os_keychain: bool,
    /// HashiCorp Vault server URL (e.g. `https://vault.example.com:8200`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hashicorp_addr: Option<String>,
    /// Optional HashiCorp Vault enterprise namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hashicorp_namespace: Option<String>,
    /// HashiCorp Vault token (stored in OS keychain on save; returned masked/present to UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hashicorp_token: Option<String>,
    /// 1Password Connect server URL (optional; otherwise uses `op` CLI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onepassword_connect_url: Option<String>,
    /// 1Password Connect token or `OP_SERVICE_ACCOUNT_TOKEN` (stored in OS keychain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onepassword_token: Option<String>,
    /// Default Azure Key Vault name (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azure_vault_name: Option<String>,
    /// Azure Key Vault bearer access token (optional; otherwise uses `az` CLI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azure_token: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultProviderStatus {
    pub id: String,
    pub name: String,
    pub scheme: String,
    pub available: bool,
    pub detail: String,
}

static SECRET_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();

fn secret_cache() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    SECRET_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn clear_secret_cache() {
    if let Ok(mut guard) = secret_cache().lock() {
        guard.clear();
    }
}

/// True when `value` is a reference to an external vault or OS keychain entry.
pub fn is_vault_reference(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("op://")
        || v.starts_with("vault://")
        || v.starts_with("azkv://")
        || v.starts_with("keychain://")
        || v.starts_with("courion-secret://")
}

/// Return a human badge label for a secret value (`1Password`, `HashiCorp Vault`, `Azure Key Vault`, `OS Keychain`, or `Local`).
pub fn secret_provider_label(value: &str, use_os_keychain: bool) -> &'static str {
    let v = value.trim();
    if v.starts_with("op://") {
        "1Password"
    } else if v.starts_with("vault://") {
        "HashiCorp Vault"
    } else if v.starts_with("azkv://") {
        "Azure Key Vault"
    } else if v.starts_with("keychain://") || v.starts_with("courion-secret://") || use_os_keychain
    {
        "OS Keychain"
    } else {
        "Local"
    }
}

// ------------------------------------------------------------------ OS Keychain

/// Service name every Courion secret is filed under in the OS keychain.
const KEYCHAIN_SERVICE: &str = "courion";

/// Values read from or written to the keychain during this run, so tree
/// reloads don't hit the keychain once per secret.
static KEYCHAIN_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Keep keychain entries in memory only (tests, throwaway runs).
static IN_MEMORY_KEYCHAIN: AtomicBool = AtomicBool::new(cfg!(test));

fn keychain_cache() -> &'static Mutex<HashMap<String, String>> {
    KEYCHAIN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Keep keychain entries in memory for the rest of this process instead of
/// the OS keychain. For tests and throwaway runs that must not touch the
/// user's real keychain.
pub fn use_in_memory_keychain() {
    IN_MEMORY_KEYCHAIN.store(true, Ordering::SeqCst);
}

/// Store a secret in the OS keychain: Windows Credential Manager, the macOS
/// Keychain, or the Secret Service (`secret-tool`, libsecret) on Linux.
/// Fails when no keychain is reachable, so callers can fall back to local
/// state.
pub fn keychain_set(account: &str, secret: &str) -> Result<()> {
    if !IN_MEMORY_KEYCHAIN.load(Ordering::SeqCst) {
        os_keychain::set(account, secret)
            .map_err(|e| Error::Invalid(format!("OS keychain: {e}")))?;
    }
    if let Ok(mut cache) = keychain_cache().lock() {
        cache.insert(account.to_string(), secret.to_string());
    }
    Ok(())
}

/// Read a secret from the OS keychain.
pub fn keychain_get(account: &str) -> Option<String> {
    if let Some(v) = keychain_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(account).cloned())
    {
        return Some(v);
    }
    if IN_MEMORY_KEYCHAIN.load(Ordering::SeqCst) {
        return None;
    }
    let value = os_keychain::get(account)?;
    if let Ok(mut cache) = keychain_cache().lock() {
        cache.insert(account.to_string(), value.clone());
    }
    Some(value)
}

/// Remove a secret from the OS keychain. Missing entries are not an error.
pub fn keychain_delete(account: &str) -> Result<()> {
    if let Ok(mut cache) = keychain_cache().lock() {
        cache.remove(account);
    }
    if IN_MEMORY_KEYCHAIN.load(Ordering::SeqCst) {
        return Ok(());
    }
    os_keychain::delete(account).map_err(|e| Error::Invalid(format!("OS keychain: {e}")))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod os_keychain {
    use super::KEYCHAIN_SERVICE;

    fn entry(account: &str) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account)
    }

    pub fn set(account: &str, secret: &str) -> Result<(), String> {
        entry(account)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| e.to_string())
    }

    pub fn get(account: &str) -> Option<String> {
        entry(account).ok()?.get_password().ok()
    }

    pub fn delete(account: &str) -> Result<(), String> {
        match entry(account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Linux: libsecret's `secret-tool`, with the secret passed on stdin so it
/// never shows up in the process list.
#[cfg(target_os = "linux")]
mod os_keychain {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use super::KEYCHAIN_SERVICE;

    pub fn set(account: &str, secret: &str) -> Result<(), String> {
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label=Courion secret",
                "service",
                KEYCHAIN_SERVICE,
                "account",
                account,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("secret-tool is not available ({e})"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(secret.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    pub fn get(account: &str) -> Option<String> {
        let out = Command::new("secret-tool")
            .args(["lookup", "service", KEYCHAIN_SERVICE, "account", account])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .trim_end_matches('\n')
                .to_string()
        })
    }

    pub fn delete(account: &str) -> Result<(), String> {
        // `clear` succeeds whether or not the entry exists, and without the
        // tool nothing can have been stored.
        let _ = Command::new("secret-tool")
            .args(["clear", "service", KEYCHAIN_SERVICE, "account", account])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod os_keychain {
    pub fn set(_account: &str, _secret: &str) -> Result<(), String> {
        Err("no OS keychain on this platform".into())
    }

    pub fn get(_account: &str) -> Option<String> {
        None
    }

    pub fn delete(_account: &str) -> Result<(), String> {
        Ok(())
    }
}

// ------------------------------------------------------------------ Vault Settings

fn vault_config_path(ws_root: &Path) -> PathBuf {
    ws_root.join(LOCAL_DIR).join(VAULT_CONFIG_FILE)
}

/// Keychain account prefix for a workspace: a stable hash of its folder, so
/// two checkouts of the same repository keep separate secrets.
pub(crate) fn ws_key_prefix(ws_root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(ws_root.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("ws-{hex}")
}

pub fn load_vault_settings(ws_root: &Path) -> Result<VaultSettings> {
    let path = vault_config_path(ws_root);
    let mut settings: VaultSettings = if path.exists() {
        read_json(&path)?
    } else {
        VaultSettings {
            use_os_keychain: true,
            ..Default::default()
        }
    };
    let prefix = ws_key_prefix(ws_root);
    if settings.hashicorp_token.is_none() {
        settings.hashicorp_token = keychain_get(&format!("{prefix}:vault:hashicorp_token"));
    }
    if settings.onepassword_token.is_none() {
        settings.onepassword_token = keychain_get(&format!("{prefix}:vault:onepassword_token"));
    }
    if settings.azure_token.is_none() {
        settings.azure_token = keychain_get(&format!("{prefix}:vault:azure_token"));
    }
    Ok(settings)
}

pub fn save_vault_settings(ws_root: &Path, settings: &VaultSettings) -> Result<()> {
    let prefix = ws_key_prefix(ws_root);
    // Tokens go to the OS keychain; without one they stay in the local,
    // git-ignored `.trax/vault.json` like other local secrets.
    let mut on_disk = settings.clone();
    for (name, slot) in [
        ("hashicorp_token", &mut on_disk.hashicorp_token),
        ("onepassword_token", &mut on_disk.onepassword_token),
        ("azure_token", &mut on_disk.azure_token),
    ] {
        let key = format!("{prefix}:vault:{name}");
        match slot.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(secret) => {
                if settings.use_os_keychain && keychain_set(&key, secret).is_ok() {
                    *slot = None;
                }
            }
            None => {
                let _ = keychain_delete(&key);
                *slot = None;
            }
        }
    }
    write_json(&vault_config_path(ws_root), &on_disk)?;
    clear_secret_cache();
    Ok(())
}

/// Inspect available secret backends on this machine & workspace.
pub fn vault_providers_status(ws_root: &Path) -> Vec<VaultProviderStatus> {
    let settings = load_vault_settings(ws_root).unwrap_or_default();

    let os_name = if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else {
        "Secret Service (libsecret)"
    };

    let op_cli = cli("op").arg("--version").output().is_ok();
    let op_connect = settings
        .onepassword_connect_url
        .as_deref()
        .is_some_and(|u| !u.trim().is_empty());

    let vault_cli = cli("vault").arg("version").output().is_ok();
    let vault_api = settings
        .hashicorp_addr
        .as_deref()
        .or_else(|| std::env::var("VAULT_ADDR").ok().as_deref().map(|_| "env"))
        .is_some();

    let az_cli = cli("az").arg("--version").output().is_ok();
    let az_api = settings
        .azure_token
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty())
        || std::env::var("AZURE_ACCESS_TOKEN").is_ok();

    // Windows and macOS always have one; Linux needs libsecret's tool.
    let keychain =
        !cfg!(target_os = "linux") || Command::new("secret-tool").arg("--help").output().is_ok();

    vec![
        VaultProviderStatus {
            id: "os-keychain".into(),
            name: os_name.into(),
            scheme: "keychain://".into(),
            available: keychain,
            detail: match (keychain, settings.use_os_keychain) {
                (true, true) => format!("Environment secrets are stored in {os_name}"),
                (true, false) => format!("Off: secrets stay in .trax/state.json ({os_name} available)"),
                (false, _) => "Install libsecret-tools (secret-tool); until then secrets stay in .trax/state.json".into(),
            },
        },
        VaultProviderStatus {
            id: "1password".into(),
            name: "1Password".into(),
            scheme: "op://vault/item/field".into(),
            available: op_cli || op_connect,
            detail: if op_connect {
                "Configured via 1Password Connect API".into()
            } else if op_cli {
                "1Password CLI (`op`) detected".into()
            } else {
                "Install `op` CLI or configure 1Password Connect URL".into()
            },
        },
        VaultProviderStatus {
            id: "hashicorp-vault".into(),
            name: "HashiCorp Vault".into(),
            scheme: "vault://mount/path#field".into(),
            available: vault_api || vault_cli,
            detail: if let Some(addr) = &settings.hashicorp_addr {
                format!("Configured ({addr})")
            } else if std::env::var("VAULT_ADDR").is_ok() {
                "Configured via VAULT_ADDR".into()
            } else if vault_cli {
                "Vault CLI (`vault`) detected".into()
            } else {
                "Set HashiCorp Vault URL & token below or VAULT_ADDR".into()
            },
        },
        VaultProviderStatus {
            id: "azure-keyvault".into(),
            name: "Azure Key Vault".into(),
            scheme: "azkv://vault-name/secret-name".into(),
            available: az_api || az_cli,
            detail: if az_api {
                "Configured with Azure access token".into()
            } else if az_cli {
                "Azure CLI (`az`) detected".into()
            } else {
                "Set Azure Key Vault token below or sign in with `az login`".into()
            },
        },
    ]
}

// ------------------------------------------------------------------ Resolution

/// A helper CLI (`op`, `vault`, `az`, `gh`) without a console window popping
/// up on Windows, where the Azure CLI is a `.cmd` script.
pub(crate) fn cli(program: &str) -> Command {
    let program = if cfg!(windows) && program == "az" {
        "az.cmd"
    } else {
        program
    };
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Resolve a secret reference (`op://...`, `vault://...`, `azkv://...`,
/// `keychain://...`, `courion-secret://...`). Non-reference strings are
/// returned unchanged.
pub fn resolve_secret_value(ws_root: &Path, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if !is_vault_reference(trimmed) {
        return Ok(raw.to_string());
    }

    if let Ok(guard) = secret_cache().lock() {
        if let Some((cached, at)) = guard.get(trimmed) {
            if at.elapsed() < CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }

    let resolved = if let Some(rest) = trimmed
        .strip_prefix("keychain://")
        .or_else(|| trimmed.strip_prefix("courion-secret://"))
    {
        keychain_get(rest)
            .ok_or_else(|| Error::NotFound(format!("secret `{rest}` not found in OS keychain")))?
    } else if trimmed.starts_with("op://") {
        resolve_onepassword(ws_root, trimmed)?
    } else if let Some(rest) = trimmed.strip_prefix("vault://") {
        resolve_hashicorp_vault(ws_root, rest)?
    } else if let Some(rest) = trimmed.strip_prefix("azkv://") {
        resolve_azure_keyvault(ws_root, rest)?
    } else {
        raw.to_string()
    };

    if let Ok(mut guard) = secret_cache().lock() {
        guard.insert(trimmed.to_string(), (resolved.clone(), Instant::now()));
    }
    Ok(resolved)
}

fn resolve_onepassword(ws_root: &Path, uri: &str) -> Result<String> {
    let settings = load_vault_settings(ws_root).unwrap_or_default();
    // 1. If 1Password Connect URL + token are configured, query the Connect REST API.
    if let (Some(base_url), Some(token)) = (
        settings
            .onepassword_connect_url
            .as_deref()
            .filter(|s| !s.is_empty()),
        settings
            .onepassword_token
            .as_deref()
            .filter(|s| !s.is_empty()),
    ) {
        // uri = op://<vault>/<item>/<field>
        let parts: Vec<&str> = uri.trim_start_matches("op://").splitn(3, '/').collect();
        if parts.len() == 3 {
            let (vault, item, field) = (
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].to_string(),
            );
            let token = token.to_string();
            let url = format!(
                "{}/v1/vaults/{}/items/{}",
                base_url.trim_end_matches('/'),
                vault,
                item
            );
            let val = std::thread::spawn(move || -> Result<String> {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| Error::Http(e.to_string()))?;
                rt.block_on(async {
                    let client = reqwest::Client::new();
                    let resp = client
                        .get(&url)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(|e| Error::Http(format!("1Password Connect error: {e}")))?;
                    if !resp.status().is_success() {
                        return Err(Error::Http(format!(
                            "1Password Connect HTTP {}",
                            resp.status()
                        )));
                    }
                    let text = resp.text().await.map_err(|e| Error::Http(e.to_string()))?;
                    let json: serde_json::Value = serde_json::from_str(&text)?;
                    if let Some(fields) = json.get("fields").and_then(|f| f.as_array()) {
                        for f in fields {
                            let label_match = f
                                .get("label")
                                .and_then(|l| l.as_str())
                                .is_some_and(|l| l.eq_ignore_ascii_case(&field));
                            let id_match = f
                                .get("id")
                                .and_then(|l| l.as_str())
                                .is_some_and(|l| l.eq_ignore_ascii_case(&field));
                            if label_match || id_match {
                                if let Some(v) = f.get("value").and_then(|v| v.as_str()) {
                                    return Ok(v.to_string());
                                }
                            }
                        }
                    }
                    Err(Error::NotFound(format!(
                        "field `{field}` not found in 1Password item `{item}`"
                    )))
                })
            })
            .join()
            .map_err(|_| Error::Http("1Password Connect thread panicked".into()))??;
            return Ok(val);
        }
    }

    // 2. Otherwise use the `op` CLI (`op read op://vault/item/field`).
    let mut cmd = cli("op");
    cmd.args(["read", "--no-newline", uri]);
    if let Some(tok) = settings
        .onepassword_token
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        cmd.env("OP_SERVICE_ACCOUNT_TOKEN", tok);
    }
    let out = cmd
        .output()
        .map_err(|e| Error::Invalid(format!("failed to run 1Password `op` CLI: {e}")))?;
    if !out.status.success() {
        return Err(Error::Invalid(format!(
            "1Password `op read` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn resolve_hashicorp_vault(ws_root: &Path, rest: &str) -> Result<String> {
    // rest = <mount>/<path>#<field>  or  <mount>/<path> (defaults to field "value")
    let (mount_and_path, field) = match rest.split_once('#') {
        Some((mp, f)) if !f.trim().is_empty() => (mp.trim_matches('/'), f.trim()),
        _ => (rest.trim_matches('/'), "value"),
    };
    let (mount, secret_path) = mount_and_path.split_once('/').ok_or_else(|| {
        Error::Invalid(format!(
            "invalid HashiCorp Vault URI `vault://{rest}` (expected `vault://mount/path#field`)"
        ))
    })?;

    let settings = load_vault_settings(ws_root).unwrap_or_default();
    let addr = settings
        .hashicorp_addr
        .or_else(|| std::env::var("VAULT_ADDR").ok())
        .filter(|s| !s.trim().is_empty());
    let token = settings
        .hashicorp_token
        .or_else(|| std::env::var("VAULT_TOKEN").ok())
        .filter(|s| !s.trim().is_empty());
    let namespace = settings
        .hashicorp_namespace
        .or_else(|| std::env::var("VAULT_NAMESPACE").ok())
        .filter(|s| !s.trim().is_empty());

    if let (Some(addr), Some(token)) = (addr, token) {
        let addr = addr.trim_end_matches('/').to_string();
        let mount = mount.to_string();
        let secret_path = secret_path.to_string();
        let field = field.to_string();
        return std::thread::spawn(move || -> Result<String> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| Error::Http(e.to_string()))?;
            rt.block_on(async {
                let client = reqwest::Client::new();
                // Try KV v2 first (`/v1/{mount}/data/{secret_path}`), then KV v1 (`/v1/{mount}/{secret_path}`).
                let urls = [
                    format!("{addr}/v1/{mount}/data/{secret_path}"),
                    format!("{addr}/v1/{mount}/{secret_path}"),
                ];
                for url in urls {
                    let mut req = client.get(&url).header("X-Vault-Token", &token);
                    if let Some(ns) = &namespace {
                        req = req.header("X-Vault-Namespace", ns);
                    }
                    let resp = req
                        .send()
                        .await
                        .map_err(|e| Error::Http(format!("HashiCorp Vault request failed: {e}")))?;
                    if !resp.status().is_success() {
                        continue;
                    }
                    let text = resp.text().await.map_err(|e| Error::Http(e.to_string()))?;
                    let json: serde_json::Value = serde_json::from_str(&text)?;
                    // KV v2 wraps under `data.data.<field>`; KV v1 wraps under `data.<field>`.
                    let val = json
                        .pointer(&format!("/data/data/{field}"))
                        .or_else(|| json.pointer(&format!("/data/{field}")));
                    if let Some(v) = val {
                        return Ok(match v.as_str() {
                            Some(s) => s.to_string(),
                            None => v.to_string(),
                        });
                    }
                }
                Err(Error::NotFound(format!(
                    "secret `vault://{mount}/{secret_path}#{field}` not found"
                )))
            })
        })
        .join()
        .map_err(|_| Error::Http("HashiCorp Vault thread panicked".into()))?;
    }

    // Fallback to `vault` CLI
    let out = cli("vault")
        .args([
            "kv",
            "get",
            &format!("-field={field}"),
            &format!("{mount}/{secret_path}"),
        ])
        .output()
        .map_err(|e| {
            Error::Invalid(format!(
                "HashiCorp Vault not configured and `vault` CLI failed: {e}"
            ))
        })?;
    if !out.status.success() {
        return Err(Error::Invalid(format!(
            "`vault kv get` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn resolve_azure_keyvault(ws_root: &Path, rest: &str) -> Result<String> {
    // rest = <vault-name>/<secret-name>  or  https://<host>/secrets/<secret-name>
    let settings = load_vault_settings(ws_root).unwrap_or_default();
    let (vault_url, secret_name) = if rest.starts_with("http://") || rest.starts_with("https://") {
        let trimmed = rest.trim_end_matches('/');
        let (base, name) = trimmed.rsplit_once('/').ok_or_else(|| {
            Error::Invalid(format!("invalid Azure Key Vault URI `azkv://{rest}`"))
        })?;
        let base = base.trim_end_matches("/secrets");
        (base.to_string(), name.to_string())
    } else {
        let (vault, name) = rest.split_once('/').ok_or_else(|| {
            Error::Invalid(format!(
                "invalid Azure Key Vault URI `azkv://{rest}` (expected `azkv://vault-name/secret-name`)"
            ))
        })?;
        (format!("https://{vault}.vault.azure.net"), name.to_string())
    };

    let token = settings
        .azure_token
        .or_else(|| std::env::var("AZURE_ACCESS_TOKEN").ok())
        .filter(|s| !s.trim().is_empty());

    if let Some(token) = token {
        let url = format!("{vault_url}/secrets/{secret_name}?api-version=7.4");
        return std::thread::spawn(move || -> Result<String> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| Error::Http(e.to_string()))?;
            rt.block_on(async {
                let client = reqwest::Client::new();
                let resp = client
                    .get(&url)
                    .bearer_auth(&token)
                    .send()
                    .await
                    .map_err(|e| Error::Http(format!("Azure Key Vault request failed: {e}")))?;
                if !resp.status().is_success() {
                    return Err(Error::Http(format!(
                        "Azure Key Vault HTTP {}",
                        resp.status()
                    )));
                }
                let text = resp.text().await.map_err(|e| Error::Http(e.to_string()))?;
                let json: serde_json::Value = serde_json::from_str(&text)?;
                json.get("value")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        Error::NotFound(format!(
                            "Azure Key Vault secret `{secret_name}` had no value"
                        ))
                    })
            })
        })
        .join()
        .map_err(|_| Error::Http("Azure Key Vault thread panicked".into()))?;
    }

    let vault_name = vault_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('.')
        .next()
        .unwrap_or("");
    let out = cli("az")
        .args([
            "keyvault",
            "secret",
            "show",
            "--vault-name",
            vault_name,
            "--name",
            &secret_name,
            "--query",
            "value",
            "-o",
            "tsv",
        ])
        .output()
        .map_err(|e| {
            Error::Invalid(format!(
                "Azure Key Vault token not set and `az` CLI failed: {e}"
            ))
        })?;
    if !out.status.success() {
        return Err(Error::Invalid(format!(
            "`az keyvault secret show` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn os_keychain_and_vault_settings_round_trip() {
        let ws = tempfile::tempdir().unwrap();

        keychain_set("test-ws/dev/apiKey", "super-secret-value").unwrap();
        assert_eq!(
            keychain_get("test-ws/dev/apiKey").as_deref(),
            Some("super-secret-value")
        );
        let resolved = resolve_secret_value(ws.path(), "keychain://test-ws/dev/apiKey").unwrap();
        assert_eq!(resolved, "super-secret-value");

        let settings = VaultSettings {
            use_os_keychain: true,
            hashicorp_addr: Some("http://127.0.0.1:8200".into()),
            hashicorp_namespace: None,
            hashicorp_token: Some("hvs.test-token".into()),
            onepassword_connect_url: None,
            onepassword_token: None,
            azure_vault_name: Some("my-kv".into()),
            azure_token: None,
        };
        save_vault_settings(ws.path(), &settings).unwrap();
        // Token must not appear in plaintext in `.trax/vault.json`.
        let on_disk = std::fs::read_to_string(ws.path().join(".trax/vault.json")).unwrap();
        assert!(!on_disk.contains("hvs.test-token"));

        let loaded = load_vault_settings(ws.path()).unwrap();
        assert_eq!(loaded.hashicorp_token.as_deref(), Some("hvs.test-token"));
    }

    /// Talks to the real OS keychain, so it only runs on request:
    /// `cargo test -p trax-core -- --ignored os_keychain_round_trip`.
    #[test]
    #[ignore]
    fn os_keychain_round_trip() {
        let account = format!("trax-core-test:{}", uuid::Uuid::new_v4());
        os_keychain::set(&account, "s3cr3t value").unwrap();
        assert_eq!(os_keychain::get(&account).as_deref(), Some("s3cr3t value"));
        os_keychain::delete(&account).unwrap();
        assert_eq!(os_keychain::get(&account), None);
        // Deleting again is fine.
        os_keychain::delete(&account).unwrap();
    }

    #[tokio::test]
    async fn resolves_hashicorp_vault_and_1password_connect() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/secret/data/payments/stripe"))
            .and(header("X-Vault-Token", "hvs.tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "data": {
                        "apiKey": "sk_live_vault_123"
                    }
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/vaults/eng/items/stripe"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "fields": [
                    { "label": "credential", "value": "op_secret_456" }
                ]
            })))
            .mount(&server)
            .await;

        let ws = tempfile::tempdir().unwrap();
        let settings = VaultSettings {
            use_os_keychain: true,
            hashicorp_addr: Some(server.uri()),
            hashicorp_namespace: None,
            hashicorp_token: Some("hvs.tok".into()),
            onepassword_connect_url: Some(server.uri()),
            onepassword_token: Some("op.tok".into()),
            azure_vault_name: None,
            azure_token: None,
        };
        save_vault_settings(ws.path(), &settings).unwrap();

        let v1 = resolve_secret_value(ws.path(), "vault://secret/payments/stripe#apiKey").unwrap();
        assert_eq!(v1, "sk_live_vault_123");

        let v2 = resolve_secret_value(ws.path(), "op://eng/stripe/credential").unwrap();
        assert_eq!(v2, "op_secret_456");
    }
}
